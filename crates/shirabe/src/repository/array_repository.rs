//! ref: composer/src/Composer/Repository/ArrayRepository.php

use std::any::Any;
use std::cell::RefCell;

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_php_shim::{
    Countable, InvalidArgumentException, LogicException, implode, preg_quote, spl_object_hash,
    strtolower,
};
use shirabe_semver::constraint::Constraint;
use shirabe_semver::constraint::ConstraintInterface;

use crate::package::AliasPackage;
use crate::package::BasePackage;
use crate::package::CompleteAliasPackage;
use crate::package::CompletePackage;
use crate::package::CompletePackageInterface;
use crate::package::PackageInterface;
use crate::package::version::StabilityFilter;
use crate::package::version::VersionParser;
use crate::repository::{
    AbandonedInfo, FindPackageConstraint, LoadPackagesResult, ProviderInfo, RepositoryInterface,
    SearchResult,
};

/// A repository implementation that simply stores packages in an array
#[derive(Debug)]
pub struct ArrayRepository {
    /// @var ?array<BasePackage>
    // TODO(phase-b): RefCell models PHP's lazy init via getPackages()/count() under &self
    pub(crate) packages: RefCell<Option<Vec<Box<dyn BasePackage>>>>,

    /// @var ?array<BasePackage> indexed by package unique name and used to cache hasPackage calls
    pub(crate) package_map: RefCell<Option<IndexMap<String, Box<dyn BasePackage>>>>,
}

impl ArrayRepository {
    /// @param array<PackageInterface> $packages
    pub fn new(packages: Vec<Box<dyn PackageInterface>>) -> Result<Self> {
        let this = Self {
            packages: RefCell::new(None),
            package_map: RefCell::new(None),
        };
        for package in packages {
            this.add_package(package)?;
        }
        Ok(this)
    }

    /// Adds a new package to the repository
    pub fn add_package(&self, package: Box<dyn PackageInterface>) -> Result<()> {
        // PHP: if (!$package instanceof BasePackage) throw new \InvalidArgumentException(...)
        // TODO(phase-b): need a real `instanceof BasePackage` check on dyn PackageInterface;
        // dyn-trait downcast requires Sized. Defer until BasePackage exposes an `as_base_package`.
        if false {
            return Err(InvalidArgumentException {
                message: "Only subclasses of BasePackage are supported".to_string(),
                code: 0,
            }
            .into());
        }
        // TODO(phase-b): convert Box<dyn PackageInterface> to Box<dyn BasePackage>
        let mut package: Box<dyn BasePackage> =
            todo!("downcast Box<dyn PackageInterface> to Box<dyn BasePackage>");

        if self.packages.borrow().is_none() {
            self.initialize();
        }
        // TODO(phase-b): pass a reference to self, not a clone
        package.set_repository(todo!("self as Box<dyn RepositoryInterface>"))?;

        let aliased_package: Option<Box<dyn BasePackage>> =
            if let Some(alias) = package.as_any().downcast_ref::<AliasPackage>() {
                Some(alias.get_alias_of().clone_box())
            } else {
                None
            };

        self.packages.borrow_mut().as_mut().unwrap().push(package);

        if let Some(aliased_package) = aliased_package {
            if aliased_package.get_repository().is_none() {
                // TODO(phase-b): pass aliased_package as Box<dyn PackageInterface>
                self.add_package(todo!("Box<BasePackage> -> Box<dyn PackageInterface>"))?;
            }
        }

        // invalidate package map cache
        *self.package_map.borrow_mut() = None;
        Ok(())
    }

    /// @return AliasPackage|CompleteAliasPackage
    pub(crate) fn create_alias_package(
        &self,
        mut package: Box<dyn BasePackage>,
        alias: String,
        pretty_alias: String,
    ) -> Box<dyn BasePackage> {
        while let Some(alias_pkg) = package.as_any().downcast_ref::<AliasPackage>() {
            package = alias_pkg.get_alias_of().clone_box();
        }

        if package.as_any().downcast_ref::<CompletePackage>().is_some() {
            // TODO(phase-b): construct CompleteAliasPackage/AliasPackage and return as Box<BasePackage>
            return todo!("new CompleteAliasPackage(package, alias, pretty_alias)");
        }

        todo!("new AliasPackage(package, alias, pretty_alias)")
    }

    /// Removes package from repository.
    pub fn remove_package(&self, package: &dyn PackageInterface) {
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

impl Countable for ArrayRepository {
    /// Returns the number of packages in this repository
    ///
    /// @return 0|positive-int Number of packages
    fn count(&self) -> i64 {
        if self.packages.borrow().is_none() {
            self.initialize();
        }

        self.packages.borrow().as_ref().unwrap().len() as i64
    }
}

impl RepositoryInterface for ArrayRepository {
    fn get_repo_name(&self) -> String {
        format!(
            "array repo (defining {} package{})",
            self.count(),
            if self.count() > 1 { "s" } else { "" },
        )
    }

    fn load_packages(
        &self,
        package_name_map: IndexMap<String, Option<Box<dyn ConstraintInterface>>>,
        acceptable_stabilities: IndexMap<String, i64>,
        stability_flags: IndexMap<String, i64>,
        already_loaded: IndexMap<String, IndexMap<String, Box<dyn PackageInterface>>>,
    ) -> LoadPackagesResult {
        let packages = self.get_packages();

        let mut result: IndexMap<String, Box<dyn BasePackage>> = IndexMap::new();
        let mut names_found: IndexMap<String, bool> = IndexMap::new();
        for package in &packages {
            if package_name_map.contains_key(PackageInterface::get_name(package.as_ref())) {
                let constraint_opt = package_name_map
                    .get(PackageInterface::get_name(package.as_ref()))
                    .unwrap();
                let constraint_matches = match constraint_opt {
                    None => true,
                    Some(c) => c.matches(&Constraint::new("==", package.get_version())),
                };
                if constraint_matches
                    && StabilityFilter::is_package_acceptable(
                        &acceptable_stabilities,
                        &stability_flags,
                        &PackageInterface::get_names(package.as_ref(), true),
                        package.get_stability(),
                    )
                    && !already_loaded
                        .get(PackageInterface::get_name(package.as_ref()))
                        .map(|v| v.contains_key(package.get_version()))
                        .unwrap_or(false)
                {
                    // add selected packages which match stability requirements
                    result.insert(spl_object_hash(package.as_ref()), package.clone_box());
                    // add the aliased package for packages where the alias matches
                    if let Some(alias) = package.as_any().downcast_ref::<AliasPackage>() {
                        let aliased = alias.get_alias_of();
                        if !result.contains_key(&spl_object_hash(aliased)) {
                            result.insert(spl_object_hash(aliased), aliased.clone_box());
                        }
                    }
                }

                names_found.insert(
                    PackageInterface::get_name(package.as_ref()).to_string(),
                    true,
                );
            }
        }

        // add aliases of packages that were selected, even if the aliases did not match
        for package in &packages {
            if let Some(alias) = package.as_any().downcast_ref::<AliasPackage>() {
                let aliased = alias.get_alias_of();
                if result.contains_key(&spl_object_hash(aliased)) {
                    result.insert(spl_object_hash(package.as_ref()), package.clone_box());
                }
            }
        }

        LoadPackagesResult {
            names_found: names_found.into_keys().collect(),
            packages: result,
        }
    }

    fn find_package(
        &self,
        name: &str,
        constraint: FindPackageConstraint,
    ) -> Option<Box<dyn BasePackage>> {
        let name = strtolower(name);

        let constraint: Box<dyn ConstraintInterface> = match constraint {
            FindPackageConstraint::Constraint(c) => c,
            FindPackageConstraint::String(s) => {
                let version_parser = VersionParser::new();
                version_parser.parse_constraints(&s).unwrap().clone_box()
            }
        };

        for package in self.get_packages() {
            if name == PackageInterface::get_name(package.as_ref()) {
                let pkg_constraint = Constraint::new("==", package.get_version());
                if constraint.matches(&pkg_constraint) {
                    return Some(package);
                }
            }
        }

        None
    }

    fn find_packages(
        &self,
        name: &str,
        constraint: Option<FindPackageConstraint>,
    ) -> Vec<Box<dyn BasePackage>> {
        // normalize name
        let name = strtolower(name);
        let mut packages = vec![];

        let constraint: Option<Box<dyn ConstraintInterface>> = match constraint {
            None => None,
            Some(FindPackageConstraint::Constraint(c)) => Some(c),
            Some(FindPackageConstraint::String(s)) => {
                let version_parser = VersionParser::new();
                Some(version_parser.parse_constraints(&s).unwrap().clone_box())
            }
        };

        for package in self.get_packages() {
            if name == PackageInterface::get_name(package.as_ref()) {
                if constraint.is_none()
                    || constraint
                        .as_ref()
                        .unwrap()
                        .matches(&Constraint::new("==", package.get_version()))
                {
                    packages.push(package);
                }
            }
        }

        packages
    }

    fn search(&self, query: String, mode: i64, r#type: Option<String>) -> Vec<SearchResult> {
        let regex = if mode == crate::repository::SEARCH_FULLTEXT {
            let parts = Preg::split("{\\s+}", &preg_quote(&query, None)).unwrap_or_default();
            format!("{{(?:{})}}i", implode("|", &parts))
        } else {
            // vendor/name searches expect the caller to have preg_quoted the query
            let parts = Preg::split("{\\s+}", &query).unwrap_or_default();
            format!("{{(?:{})}}i", implode("|", &parts))
        };

        let mut matches: IndexMap<String, SearchResult> = IndexMap::new();
        for package in self.get_packages() {
            let mut name = PackageInterface::get_name(package.as_ref()).to_string();
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

            let complete = package.as_any().downcast_ref::<CompletePackage>();

            let fulltext_match = mode == crate::repository::SEARCH_FULLTEXT
                && complete.is_some()
                && Preg::is_match(
                    &regex,
                    &format!(
                        "{} {}",
                        implode(" ", &complete.unwrap().get_keywords()),
                        complete.unwrap().get_description().unwrap_or("")
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
                    let description = complete.and_then(|c| c.get_description().map(String::from));
                    let abandoned = if let Some(c) = complete {
                        if c.is_abandoned() {
                            // PHP: $package->getReplacementPackage() ?: true
                            match c.get_replacement_package() {
                                Some(s) if !s.is_empty() => {
                                    Some(AbandonedInfo::Replacement(s.to_string()))
                                }
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
                            name: package.get_pretty_name().to_string(),
                            description,
                            abandoned,
                            url: None,
                        },
                    );
                }
            }
        }

        matches.into_values().collect()
    }

    fn has_package(&self, package: &dyn PackageInterface) -> bool {
        if self.package_map.borrow().is_none() {
            let mut map: IndexMap<String, Box<dyn BasePackage>> = IndexMap::new();
            for repo_package in self.get_packages() {
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

    fn get_providers(&self, package_name: String) -> IndexMap<String, ProviderInfo> {
        let mut result: IndexMap<String, ProviderInfo> = IndexMap::new();

        'candidates: for candidate in self.get_packages() {
            if result.contains_key(PackageInterface::get_name(candidate.as_ref())) {
                continue;
            }
            for link in candidate.get_provides().values() {
                if package_name == link.get_target() {
                    let complete = candidate.as_any().downcast_ref::<CompletePackage>();
                    let description = complete.and_then(|c| c.get_description().map(String::from));
                    result.insert(
                        PackageInterface::get_name(candidate.as_ref()).to_string(),
                        ProviderInfo {
                            name: PackageInterface::get_name(candidate.as_ref()).to_string(),
                            description,
                            r#type: candidate.get_type().to_string(),
                        },
                    );
                    continue 'candidates;
                }
            }
        }

        result
    }

    fn get_packages(&self) -> Vec<Box<dyn BasePackage>> {
        if self.packages.borrow().is_none() {
            self.initialize();
        }

        if self.packages.borrow().is_none() {
            // PHP: throw new \LogicException('initialize failed to initialize the packages array')
            panic!(
                "{}",
                LogicException {
                    message: "initialize failed to initialize the packages array".to_string(),
                    code: 0,
                }
                .message
            );
        }

        // TODO(phase-b): return references rather than cloning the whole vector
        self.packages
            .borrow()
            .as_ref()
            .unwrap()
            .iter()
            .map(|p| p.clone_box())
            .collect()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
