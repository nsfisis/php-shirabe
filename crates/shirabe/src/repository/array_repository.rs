//! ref: composer/src/Composer/Repository/ArrayRepository.php

use std::any::Any;
use std::cell::RefCell;
use std::rc::Weak;

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_php_shim::{implode, preg_quote, strtolower};
use shirabe_semver::constraint::AnyConstraint;
use shirabe_semver::constraint::SimpleConstraint;

use crate::package::BasePackageHandle;
use crate::package::PackageHandle;
use crate::package::PackageInterfaceHandle;
use crate::package::version::StabilityFilter;
use crate::package::version::VersionParser;
use crate::repository::{
    AbandonedInfo, FindPackageConstraint, LoadPackagesResult, ProviderInfo, RepositoryInterface,
    RepositoryInterfaceHandle, RepositoryInterfaceWeakHandle, SearchResult,
};

/// A repository implementation that simply stores packages in an array
#[derive(Debug)]
pub struct ArrayRepository {
    pub(crate) packages: RefCell<Option<Vec<BasePackageHandle>>>,

    /// @var ?array<BasePackage> indexed by package unique name and used to cache hasPackage calls
    pub(crate) package_map: RefCell<Option<IndexMap<String, BasePackageHandle>>>,

    /// Weak reference to the outermost repository handle wrapping this `ArrayRepository`,
    /// injected via `set_self_handle`. Used to wire package -> repository back-references.
    self_weak: RefCell<Option<RepositoryInterfaceWeakHandle>>,
}

impl ArrayRepository {
    fn get_packages_internal(&self) -> Vec<BasePackageHandle> {
        if self.packages.borrow().is_none() {
            self.initialize();
        }
        assert!(
            self.packages.borrow().is_some(),
            "initialize failed to initialize the packages array"
        );

        self.packages
            .borrow()
            .as_ref()
            .unwrap()
            .iter()
            .map(|p| p.clone())
            .collect()
    }

    /// @param array<PackageInterface> $packages
    pub fn new(packages: Vec<PackageInterfaceHandle>) -> Result<Self> {
        let this = Self {
            packages: RefCell::new(None),
            package_map: RefCell::new(None),
            self_weak: RefCell::new(None),
        };
        for package in packages {
            this.add_package(package)?;
        }
        Ok(this)
    }

    /// Adds a new package to the repository
    pub fn add_package(&self, package: PackageInterfaceHandle) -> Result<()> {
        if self.packages.borrow().is_none() {
            self.initialize();
        }
        // PHP: $package->setRepository($this). The back-reference is wired only once this
        // repository (or its outermost wrapper) has been put behind a RepositoryInterfaceHandle;
        // packages added during construction are wired later in set_self_handle.
        if let Some(handle) = self.self_weak.borrow().as_ref().and_then(Weak::upgrade) {
            package.set_repository(RepositoryInterfaceHandle::from_rc(handle))?;
        }

        let aliased_package: Option<PackageHandle> =
            package.as_alias().map(|alias| alias.get_alias_of());

        self.packages
            .borrow_mut()
            .as_mut()
            .unwrap()
            .push(package.clone());

        if let Some(aliased_package) = aliased_package {
            // PHP: if ($aliasedPackage->getRepository() === null) $this->addPackage($aliasedPackage);
            if aliased_package.get_repository().is_none() {
                self.add_package(aliased_package.into())?;
            }
        }

        // invalidate package map cache
        *self.package_map.borrow_mut() = None;
        Ok(())
    }

    /// @return AliasPackage|CompleteAliasPackage
    pub(crate) fn create_alias_package(
        &self,
        package: BasePackageHandle,
        alias: String,
        pretty_alias: String,
    ) -> BasePackageHandle {
        let mut package = package;
        while let Some(alias_pkg) = package.as_alias() {
            package = alias_pkg.get_alias_of().into();
        }

        let _ = (&package, &alias, &pretty_alias);
        if package.as_complete_package().is_some() {
            // TODO(phase-b): construct CompleteAliasPackage/AliasPackage and return as a handle
            return todo!("new CompleteAliasPackage(package, alias, pretty_alias)");
        }

        todo!("new AliasPackage(package, alias, pretty_alias)")
    }

    /// Removes package from repository.
    pub fn remove_package(&self, package: PackageInterfaceHandle) {
        let package_id = package.get_unique_name();

        let mut packages_borrow = self.packages.borrow_mut();
        let packages = packages_borrow.as_mut().expect("packages initialized");
        for (key, repo_package) in packages.iter().enumerate() {
            if package_id == repo_package.get_unique_name() {
                // PHP: array_splice($this->packages, $key, 1)
                packages.remove(key);

                drop(packages_borrow);
                // invalidate package map cache
                *self.package_map.borrow_mut() = None;

                return;
            }
        }
    }

    /// Initializes the packages array. Mostly meant as an extension point.
    pub(crate) fn initialize(&self) {
        *self.packages.borrow_mut() = Some(vec![]);
    }
}

impl RepositoryInterface for ArrayRepository {
    /// Returns the number of packages in this repository
    fn count(&self) -> anyhow::Result<usize> {
        if self.packages.borrow().is_none() {
            self.initialize();
        }

        Ok(self.packages.borrow().as_ref().unwrap().len())
    }

    fn get_repo_name(&self) -> String {
        let count = self.count().expect("ArrayRepository::count is infallible");
        format!(
            "array repo (defining {} package{})",
            count,
            if count > 1 { "s" } else { "" },
        )
    }

    fn load_packages(
        &mut self,
        package_name_map: IndexMap<String, Option<AnyConstraint>>,
        acceptable_stabilities: IndexMap<String, i64>,
        stability_flags: IndexMap<String, i64>,
        already_loaded: IndexMap<String, IndexMap<String, PackageInterfaceHandle>>,
    ) -> anyhow::Result<LoadPackagesResult> {
        let packages = self.get_packages_internal();

        let mut result: IndexMap<String, BasePackageHandle> = IndexMap::new();
        let mut names_found: IndexMap<String, bool> = IndexMap::new();
        for package in &packages {
            if package_name_map.contains_key(&package.get_name()) {
                let constraint_opt = package_name_map.get(&package.get_name()).unwrap();
                let constraint_matches = match constraint_opt {
                    None => true,
                    Some(c) => c.matches(
                        &SimpleConstraint::new(
                            "==".to_string(),
                            package.get_version().to_string(),
                            None,
                        )
                        .into(),
                    ),
                };
                if constraint_matches
                    && StabilityFilter::is_package_acceptable(
                        &acceptable_stabilities,
                        &stability_flags,
                        &package.get_names(true),
                        &package.get_stability(),
                    )
                    && !already_loaded
                        .get(&package.get_name())
                        .map(|v| v.contains_key(&package.get_version()))
                        .unwrap_or(false)
                {
                    // add selected packages which match stability requirements
                    result.insert(package.ptr_id().to_string(), package.clone());
                    // add the aliased package for packages where the alias matches
                    if let Some(alias) = package.as_alias() {
                        let aliased = alias.get_alias_of();
                        if !result.contains_key(&aliased.ptr_id().to_string()) {
                            result.insert(aliased.ptr_id().to_string(), aliased.into());
                        }
                    }
                }

                names_found.insert(package.get_name(), true);
            }
        }

        // add aliases of packages that were selected, even if the aliases did not match
        for package in &packages {
            if let Some(alias) = package.as_alias() {
                let aliased = alias.get_alias_of();
                if result.contains_key(&aliased.ptr_id().to_string()) {
                    result.insert(package.ptr_id().to_string(), package.clone());
                }
            }
        }

        Ok(LoadPackagesResult {
            names_found: names_found.into_keys().collect(),
            packages: result,
        })
    }

    fn find_package(
        &mut self,
        name: &str,
        constraint: FindPackageConstraint,
    ) -> anyhow::Result<Option<BasePackageHandle>> {
        let name = strtolower(name);

        let constraint: AnyConstraint = match constraint {
            FindPackageConstraint::Constraint(c) => c,
            FindPackageConstraint::String(s) => {
                let version_parser = VersionParser::new();
                version_parser.parse_constraints(&s).unwrap().clone()
            }
        };

        for package in self.get_packages_internal() {
            if name == package.get_name() {
                let pkg_constraint = SimpleConstraint::new(
                    "==".to_string(),
                    package.get_version().to_string(),
                    None,
                );
                if constraint.matches(&pkg_constraint.into()) {
                    return Ok(Some(package));
                }
            }
        }

        Ok(None)
    }

    fn find_packages(
        &mut self,
        name: &str,
        constraint: Option<FindPackageConstraint>,
    ) -> anyhow::Result<Vec<BasePackageHandle>> {
        // normalize name
        let name = strtolower(name);
        let mut packages = vec![];

        let constraint: Option<AnyConstraint> = match constraint {
            None => None,
            Some(FindPackageConstraint::Constraint(c)) => Some(c),
            Some(FindPackageConstraint::String(s)) => {
                let version_parser = VersionParser::new();
                Some(version_parser.parse_constraints(&s).unwrap().clone())
            }
        };

        for package in self.get_packages_internal() {
            if name == package.get_name() {
                if constraint.is_none()
                    || constraint.as_ref().unwrap().matches(
                        &SimpleConstraint::new(
                            "==".to_string(),
                            package.get_version().to_string(),
                            None,
                        )
                        .into(),
                    )
                {
                    packages.push(package);
                }
            }
        }

        Ok(packages)
    }

    fn search(
        &mut self,
        query: String,
        mode: i64,
        r#type: Option<String>,
    ) -> anyhow::Result<Vec<SearchResult>> {
        let regex = if mode == crate::repository::SEARCH_FULLTEXT {
            let parts = Preg::split("{\\s+}", &preg_quote(&query, None)).unwrap_or_default();
            format!("{{(?:{})}}i", implode("|", &parts))
        } else {
            // vendor/name searches expect the caller to have preg_quoted the query
            let parts = Preg::split("{\\s+}", &query).unwrap_or_default();
            format!("{{(?:{})}}i", implode("|", &parts))
        };

        let mut matches: IndexMap<String, SearchResult> = IndexMap::new();
        for package in self.get_packages_internal() {
            let mut name = package.get_name();
            if mode == crate::repository::SEARCH_VENDOR {
                // PHP: [$name] = explode('/', $name);
                let parts: Vec<&str> = name.splitn(2, '/').collect();
                name = parts[0].to_string();
            }
            if matches.contains_key(&name) {
                continue;
            }
            if let Some(t) = &r#type {
                if package.get_type() != *t {
                    continue;
                }
            }

            let complete = package.as_complete();

            let fulltext_match = mode == crate::repository::SEARCH_FULLTEXT
                && complete.is_some()
                && Preg::is_match(
                    &regex,
                    &format!(
                        "{} {}",
                        implode(" ", &complete.as_ref().unwrap().get_keywords()),
                        complete
                            .as_ref()
                            .unwrap()
                            .get_description()
                            .unwrap_or_default()
                    ),
                )
                .unwrap_or(false);

            if Preg::is_match(&regex, &name).unwrap_or(false) || fulltext_match {
                if mode == crate::repository::SEARCH_VENDOR {
                    matches.insert(
                        name.clone(),
                        SearchResult {
                            name,
                            description: None,
                            abandoned: None,
                            url: None,
                        },
                    );
                } else {
                    let description = complete.as_ref().and_then(|c| c.get_description());
                    let abandoned = if let Some(c) = &complete {
                        if c.is_abandoned() {
                            // PHP: $package->getReplacementPackage() ?: true
                            match c.get_replacement_package() {
                                Some(s) if !s.is_empty() => Some(AbandonedInfo::Replacement(s)),
                                _ => Some(AbandonedInfo::Abandoned),
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    matches.insert(
                        name.clone(),
                        SearchResult {
                            name: package.get_pretty_name(),
                            description,
                            abandoned,
                            url: None,
                        },
                    );
                }
            }
        }

        Ok(matches.into_values().collect())
    }

    fn has_package(&self, package: PackageInterfaceHandle) -> bool {
        if self.package_map.borrow().is_none() {
            let mut map: IndexMap<String, BasePackageHandle> = IndexMap::new();
            for repo_package in self.get_packages_internal() {
                map.insert(repo_package.get_unique_name(), repo_package);
            }
            *self.package_map.borrow_mut() = Some(map);
        }

        self.package_map
            .borrow()
            .as_ref()
            .unwrap()
            .contains_key(&package.get_unique_name())
    }

    fn get_providers(
        &mut self,
        package_name: String,
    ) -> anyhow::Result<IndexMap<String, ProviderInfo>> {
        let mut result: IndexMap<String, ProviderInfo> = IndexMap::new();

        'candidates: for candidate in self.get_packages_internal() {
            if result.contains_key(&candidate.get_name()) {
                continue;
            }
            for link in candidate.get_provides().values() {
                if package_name == link.get_target() {
                    let complete = candidate.as_complete();
                    let description = complete.and_then(|c| c.get_description());
                    result.insert(
                        candidate.get_name(),
                        ProviderInfo {
                            name: candidate.get_name(),
                            description,
                            r#type: candidate.get_type(),
                        },
                    );
                    continue 'candidates;
                }
            }
        }

        Ok(result)
    }

    fn get_packages(&mut self) -> anyhow::Result<Vec<BasePackageHandle>> {
        Ok(self.get_packages_internal())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn set_self_handle(&self, weak: RepositoryInterfaceWeakHandle) {
        *self.self_weak.borrow_mut() = Some(weak.clone());
        // Wire back-references for packages added before this repository was wrapped in a handle
        // (e.g. those passed to ArrayRepository::new). Reads the field directly to avoid
        // triggering lazy initialization in wrapper repositories.
        if let Some(handle) = weak.upgrade() {
            let handle = RepositoryInterfaceHandle::from_rc(handle);
            let pkgs: Vec<BasePackageHandle> = self
                .packages
                .borrow()
                .as_ref()
                .map(|v| v.clone())
                .unwrap_or_default();
            for pkg in pkgs {
                let _ = pkg.set_repository(handle.clone());
            }
        }
    }
}
