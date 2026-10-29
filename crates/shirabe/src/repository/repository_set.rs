//! ref: composer/src/Composer/Repository/RepositorySet.php

use crate::advisory::AnySecurityAdvisory;
use crate::dependency_resolver::{
    Pool, PoolBuilder, PoolOptimizer, Request, SecurityAdvisoryPoolFilter,
};
use crate::downloader::TransportException;
use crate::event_dispatcher::EventDispatcherInterface;
use crate::io::{IOInterface, NullIO};
use crate::package::version::StabilityFilter;
use crate::package::{
    AliasPackageHandle, BasePackageHandle, CompleteAliasPackageHandle, PackageInterfaceHandle,
};
use crate::repository::{
    CompositeRepository, FindPackageConstraint, InstalledRepository, LockArrayRepositoryHandle,
    PlatformRepository, RepositoryInterfaceHandle,
};
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_shim::{
    Catch as _, LogicException, RuntimeException, ksort, legacy_lossy, strtolower,
};
use shirabe_semver::constraint::{
    AnyConstraint, MatchAllConstraint, MultiConstraint, SimpleConstraint,
};

#[derive(Debug, Clone)]
pub struct RootAliasEntry {
    pub alias: BString,
    pub alias_normalized: BString,
}

#[derive(Debug, Clone)]
pub struct RootAliasInput {
    pub package: BString,
    pub version: BString,
    pub alias: BString,
    pub alias_normalized: BString,
}

#[derive(Debug)]
pub struct RepositorySet {
    /// Packages are returned even though their stability does not match the required stability
    /// PHP: ALLOW_UNACCEPTABLE_STABILITIES = 1
    root_aliases: IndexMap<BString, IndexMap<BString, RootAliasEntry>>,

    root_references: IndexMap<BString, BString>,

    repositories: Vec<RepositoryInterfaceHandle>,

    acceptable_stabilities: IndexMap<BString, i64>,

    stability_flags: IndexMap<BString, i64>,

    root_requires: IndexMap<BString, AnyConstraint>,

    temporary_constraints: IndexMap<BString, AnyConstraint>,

    locked: bool,
    allow_installed_repositories: bool,
}

impl RepositorySet {
    /// Packages are returned even though their stability does not match the required stability
    pub const ALLOW_UNACCEPTABLE_STABILITIES: i64 = 1;
    /// Packages will be looked up in all repositories, even after they have been found in a higher prio one
    pub const ALLOW_SHADOWED_REPOSITORIES: i64 = 2;

    /// In most cases if you are looking to use this class as a way to find packages from repositories
    /// passing minimumStability is all you need to worry about. The rest is for advanced pool creation including
    /// aliases, pinned references and other special cases.
    pub fn new(
        minimum_stability: &str,
        stability_flags: IndexMap<BString, i64>,
        root_aliases: Vec<RootAliasInput>,
        root_references: IndexMap<BString, BString>,
        mut root_requires: IndexMap<BString, AnyConstraint>,
        temporary_constraints: IndexMap<BString, AnyConstraint>,
    ) -> Self {
        let root_aliases = Self::get_root_aliases_per_package(root_aliases);

        let mut acceptable_stabilities: IndexMap<BString, i64> = IndexMap::new();
        // PHP: foreach (BasePackage::STABILITIES as $stability => $value)
        let stabilities = crate::package::STABILITIES.clone();
        let min_value = *stabilities.get(minimum_stability).unwrap_or(&0);
        for (stability, value) in stabilities.iter() {
            if *value <= min_value {
                acceptable_stabilities.insert(bstr::BString::from(stability.to_string()), *value);
            }
        }
        // PHP: foreach ($rootRequires as $name => $constraint) { if (...) unset(...); }
        let names: Vec<BString> = root_requires.keys().cloned().collect();
        for name in names {
            if PlatformRepository::is_platform_package(&name) {
                root_requires.shift_remove(&name);
            }
        }

        Self {
            root_aliases,
            root_references,
            repositories: vec![],
            acceptable_stabilities,
            stability_flags,
            root_requires,
            temporary_constraints,
            locked: false,
            allow_installed_repositories: false,
        }
    }

    pub fn allow_installed_repositories(&mut self, allow: bool) {
        self.allow_installed_repositories = allow;
    }

    pub fn get_root_requires(&self) -> &IndexMap<BString, AnyConstraint> {
        &self.root_requires
    }

    pub fn get_temporary_constraints(&self) -> &IndexMap<BString, AnyConstraint> {
        &self.temporary_constraints
    }

    /// Adds a repository to this repository set
    ///
    /// The first repos added have a higher priority. As soon as a package is found in any
    /// repository the search for that package ends, and following repos will not be consulted.
    pub fn add_repository(&mut self, repo: RepositoryInterfaceHandle) -> anyhow::Result<()> {
        if self.locked {
            return Err(RuntimeException::new("Pool has already been created from this repository set, it cannot be modified anymore.".to_string())
            .into());
        }

        let repos: Vec<RepositoryInterfaceHandle> = {
            let repo_ref = repo.borrow();
            if let Some(composite) = repo_ref.as_any().downcast_ref::<CompositeRepository>() {
                composite.get_repositories().clone()
            } else {
                drop(repo_ref);
                vec![repo]
            }
        };

        for repo in repos {
            self.repositories.push(repo);
        }

        Ok(())
    }

    /// Find packages providing or matching a name and optionally meeting a constraint in all repositories
    ///
    /// Returned in the order of repositories, matching priority
    pub fn find_packages(
        &self,
        name: &BStr,
        constraint: Option<AnyConstraint>,
        flags: i64,
    ) -> anyhow::Result<Vec<BasePackageHandle>> {
        let ignore_stability = (flags & Self::ALLOW_UNACCEPTABLE_STABILITIES) != 0;
        let load_from_all_repos = (flags & Self::ALLOW_SHADOWED_REPOSITORIES) != 0;

        let mut packages: Vec<Vec<BasePackageHandle>> = vec![];
        if load_from_all_repos {
            for repository in &self.repositories {
                // PHP: $repository->findPackages($name, $constraint) ?: []
                let constraint_clone = constraint
                    .as_ref()
                    .map(|c| FindPackageConstraint::Constraint(c.clone()));
                let found = repository.find_packages(name, constraint_clone)?;
                packages.push(found);
            }
        } else {
            'outer: for repository in &self.repositories {
                let mut name_map: IndexMap<BString, Option<AnyConstraint>> = IndexMap::new();
                name_map.insert(bstr::BString::from(name), constraint.clone());
                let acceptable = if ignore_stability {
                    // PHP: BasePackage::STABILITIES
                    crate::package::STABILITIES
                        .iter()
                        .map(|(k, v)| (BString::from(*k), *v))
                        .collect()
                } else {
                    self.acceptable_stabilities.clone()
                };
                let stability_flags = if ignore_stability {
                    IndexMap::new()
                } else {
                    self.stability_flags.clone()
                };
                let result = repository.load_packages(
                    name_map,
                    acceptable,
                    stability_flags,
                    IndexMap::new(),
                )?;

                packages.push(result.packages.into_values().collect());
                for name_found in result.names_found {
                    // avoid loading the same package again from other repositories once it has been found
                    if name == name_found {
                        break 'outer;
                    }
                }
            }
        }

        // PHP: $candidates = $packages ? array_merge(...$packages) : [];
        let candidates: Vec<BasePackageHandle> = if !packages.is_empty() {
            packages.into_iter().flatten().collect()
        } else {
            vec![]
        };

        // when using loadPackages above (!$loadFromAllRepos) the repos already filter for stability so no need to do it again
        if ignore_stability || !load_from_all_repos {
            return Ok(candidates);
        }

        let mut result: Vec<BasePackageHandle> = vec![];
        for candidate in candidates {
            if self.is_package_acceptable(
                &candidate.get_names(true),
                &legacy_lossy(&candidate.get_stability()),
            ) {
                result.push(candidate);
            }
        }

        Ok(result)
    }

    pub fn get_security_advisories(
        &self,
        package_names: Vec<BString>,
        allow_partial_advisories: bool,
        ignore_unreachable: bool,
    ) -> anyhow::Result<SecurityAdvisoriesResult> {
        let mut map: IndexMap<BString, AnyConstraint> = IndexMap::new();
        for name in &package_names {
            map.insert(name.clone(), MatchAllConstraint::new(None).into());
        }

        let mut unreachable_repos: Vec<BString> = vec![];
        let advisories = self.get_security_advisories_for_constraints(
            map,
            allow_partial_advisories,
            ignore_unreachable,
            &mut unreachable_repos,
        )?;

        Ok(SecurityAdvisoriesResult {
            advisories,
            unreachable_repos,
        })
    }

    #[tracing::instrument(skip_all)]
    pub fn get_matching_security_advisories(
        &self,
        packages: Vec<PackageInterfaceHandle>,
        allow_partial_advisories: bool,
        ignore_unreachable: bool,
    ) -> anyhow::Result<SecurityAdvisoriesResult> {
        let mut map: IndexMap<BString, AnyConstraint> = IndexMap::new();
        for package in packages {
            // ignore root alias versions as they are not actual package versions and should not matter when it comes to vulnerabilities
            if let Some(alias) = package.as_alias()
                && alias.is_root_package_alias()
            {
                continue;
            }
            let name = BString::from(&package.get_name()[..]);
            if map.contains_key(&name) {
                let existing = map.shift_remove(&name).unwrap();
                map.insert(
                    name,
                    MultiConstraint::new(
                        vec![
                            AnyConstraint::Simple(SimpleConstraint::new(
                                "=",
                                BString::from(&package.get_version()[..]),
                                None,
                            )),
                            existing,
                        ],
                        false,
                        None,
                    )
                    .into(),
                );
            } else {
                map.insert(
                    name,
                    SimpleConstraint::new("=", BString::from(&package.get_version()[..]), None)
                        .into(),
                );
            }
        }

        let mut unreachable_repos: Vec<BString> = vec![];
        let advisories = self.get_security_advisories_for_constraints(
            map,
            allow_partial_advisories,
            ignore_unreachable,
            &mut unreachable_repos,
        )?;

        Ok(SecurityAdvisoriesResult {
            advisories,
            unreachable_repos,
        })
    }

    fn get_security_advisories_for_constraints(
        &self,
        package_constraint_map: IndexMap<BString, AnyConstraint>,
        allow_partial_advisories: bool,
        ignore_unreachable: bool,
        unreachable_repos: &mut Vec<BString>,
    ) -> anyhow::Result<IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>>> {
        let mut repo_advisories: Vec<IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>>> =
            vec![];
        for repository in &self.repositories {
            let attempt: anyhow::Result<()> = (|| -> anyhow::Result<()> {
                let mut repo_ref = repository.borrow_mut();
                let Some(advisory_repo) = repo_ref.as_advisory_provider_mut() else {
                    return Ok(());
                };
                if !advisory_repo.has_security_advisories()? {
                    return Ok(());
                }

                let result = advisory_repo.get_security_advisories(
                    package_constraint_map.clone(),
                    allow_partial_advisories,
                )?;
                repo_advisories.push(result.advisories);
                Ok(())
            })();
            match attempt {
                Ok(_) => {}
                Err(e) => {
                    // PHP catches only \Composer\Downloader\TransportException; other
                    // exceptions propagate uncaught.
                    if !e.is_instanceof::<TransportException>() {
                        return Err(e);
                    }
                    if !ignore_unreachable {
                        return Err(e);
                    }
                    let message = e
                        .catch::<TransportException>()
                        .unwrap()
                        .get_message()
                        .to_string();
                    unreachable_repos.push(bstr::BString::from(message));
                }
            }
        }

        let mut advisories: IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>> =
            IndexMap::new();
        for repo in repo_advisories {
            for (name, list) in repo {
                advisories.entry(name).or_default().extend(list);
            }
        }
        ksort(&mut advisories);

        Ok(advisories)
    }

    pub fn get_providers(
        &self,
        package_name: &BStr,
    ) -> anyhow::Result<IndexMap<BString, crate::repository::ProviderInfo>> {
        let mut providers: IndexMap<BString, crate::repository::ProviderInfo> = IndexMap::new();
        for repository in &self.repositories {
            let repo_providers = repository.get_providers(bstr::BString::from(package_name))?;
            if !repo_providers.is_empty() {
                providers.extend(repo_providers);
            }
        }

        Ok(providers)
    }

    /// Check for each given package name whether it would be accepted by this RepositorySet in the given $stability
    pub fn is_package_acceptable(&self, names: &[BString], stability: &str) -> bool {
        StabilityFilter::is_package_acceptable(
            &self.acceptable_stabilities,
            &self.stability_flags,
            names,
            bstr::BStr::new(stability),
        )
    }

    /// Create a pool for dependency resolution from the packages in this repository set.
    #[allow(clippy::too_many_arguments, reason = "to keep PHP signature")]
    pub fn create_pool(
        &mut self,
        request: &mut Request,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        event_dispatcher: Option<std::rc::Rc<std::cell::RefCell<dyn EventDispatcherInterface>>>,
        pool_optimizer: Option<PoolOptimizer>,
        ignored_types: Vec<BString>,
        allowed_types: Option<Vec<BString>>,
        security_advisory_pool_filter: Option<SecurityAdvisoryPoolFilter>,
    ) -> anyhow::Result<Pool> {
        let root_aliases = self
            .root_aliases
            .iter()
            .map(|(name, versions)| {
                let versions = versions
                    .iter()
                    .map(|(version, entry)| {
                        let mut fields = IndexMap::new();
                        fields.insert(BString::from("alias"), entry.alias.clone());
                        fields.insert(
                            BString::from("alias_normalized"),
                            entry.alias_normalized.clone(),
                        );
                        (version.clone(), fields)
                    })
                    .collect();
                (name.clone(), versions)
            })
            .collect();
        let mut pool_builder = PoolBuilder::new(
            self.acceptable_stabilities.clone(),
            self.stability_flags.clone(),
            root_aliases,
            self.root_references.clone(),
            io,
            event_dispatcher,
            pool_optimizer,
            self.temporary_constraints.clone(),
            security_advisory_pool_filter,
        );
        pool_builder.set_ignored_types(
            ignored_types
                .to_vec()
                .iter()
                .map(|s| legacy_lossy(s).into_owned())
                .collect::<Vec<_>>()
                .into_iter()
                .map(bstr::BString::from)
                .collect::<Vec<_>>(),
        );
        pool_builder.set_allowed_types(allowed_types);

        for repo in &self.repositories {
            let is_installed = {
                let repo_ref = repo.borrow();
                repo_ref.as_installed_repository_interface().is_some()
                    || repo_ref.as_any().is::<InstalledRepository>()
            };
            if is_installed && !self.allow_installed_repositories {
                return Err(LogicException::new(
                    "The pool can not accept packages from an installed repository".to_string(),
                )
                .into());
            }
        }

        self.locked = true;

        pool_builder.build_pool(self.repositories.clone(), request)
    }

    /// Create a pool for dependency resolution from the packages in this repository set.
    pub fn create_pool_with_all_packages(&mut self) -> anyhow::Result<Pool> {
        for repo in &self.repositories {
            let is_installed = {
                let repo_ref = repo.borrow();
                repo_ref.as_installed_repository_interface().is_some()
                    || repo_ref.as_any().is::<InstalledRepository>()
            };
            if is_installed && !self.allow_installed_repositories {
                return Err(LogicException::new(
                    "The pool can not accept packages from an installed repository".to_string(),
                )
                .into());
            }
        }

        self.locked = true;

        let mut packages: Vec<BasePackageHandle> = vec![];
        for repository in &self.repositories {
            for mut package in repository.get_packages()? {
                let name = package.get_name();
                let version = package.get_version();
                packages.push(package.clone());

                if let Some(versions) = self.root_aliases.get(&name)
                    && let Some(alias) = versions.get(&version)
                {
                    while let Some(alias_pkg) = package.as_alias() {
                        package = alias_pkg.get_alias_of().into();
                    }
                    let alias_package: BasePackageHandle =
                        if let Some(complete) = package.as_complete_package() {
                            CompleteAliasPackageHandle::new(
                                complete,
                                alias.alias_normalized.clone(),
                                alias.alias.clone(),
                            )
                            .into()
                        } else {
                            AliasPackageHandle::new(
                                package.as_package().unwrap(),
                                alias.alias_normalized.clone(),
                                alias.alias.clone(),
                            )
                            .into()
                        };
                    if let Some(alias_handle) = alias_package.as_alias() {
                        alias_handle.set_root_package_alias(true);
                    }
                    packages.push(alias_package);
                }
            }
        }

        Ok(Pool::new(
            packages,
            vec![],
            IndexMap::new(),
            IndexMap::new(),
            IndexMap::new(),
            IndexMap::new(),
        ))
    }

    pub fn create_pool_for_package(
        &mut self,
        package_name: &BStr,
        locked_repo: Option<LockArrayRepositoryHandle>,
    ) -> anyhow::Result<Pool> {
        // TODO unify this with above in some simpler version without "request"?
        self.create_pool_for_packages(vec![bstr::BString::from(package_name)], locked_repo)
    }

    pub fn create_pool_for_packages(
        &mut self,
        package_names: Vec<BString>,
        locked_repo: Option<LockArrayRepositoryHandle>,
    ) -> anyhow::Result<Pool> {
        let mut request = Request::new(locked_repo);

        let mut allowed_packages: Vec<BString> = vec![];
        for package_name in &package_names {
            if PlatformRepository::is_platform_package(package_name) {
                return Err(LogicException::new("createPoolForPackage(s) can not be used for platform packages, as they are never loaded by the PoolBuilder which expects them to be fixed. Use createPoolWithAllPackages or pass in a proper request with the platform packages you need fixed in it.".to_string())
                .into());
            }

            request.require_name(bstr::BStr::new(&*legacy_lossy(package_name)), None)?;
            allowed_packages.push(BString::from(&strtolower(package_name)[..]));
        }

        if !allowed_packages.is_empty() {
            request.restrict_packages(
                allowed_packages
                    .to_vec()
                    .iter()
                    .map(|s| legacy_lossy(s).into_owned())
                    .collect::<Vec<_>>()
                    .into_iter()
                    .map(bstr::BString::from)
                    .collect::<Vec<_>>(),
            );
        }

        self.create_pool(
            &mut request,
            std::rc::Rc::new(std::cell::RefCell::new(NullIO::new())),
            None,
            None,
            vec![],
            None,
            None,
        )
    }

    fn get_root_aliases_per_package(
        aliases: Vec<RootAliasInput>,
    ) -> IndexMap<BString, IndexMap<BString, RootAliasEntry>> {
        let mut normalized_aliases: IndexMap<BString, IndexMap<BString, RootAliasEntry>> =
            IndexMap::new();

        for alias in aliases {
            normalized_aliases.entry(alias.package).or_default().insert(
                alias.version,
                RootAliasEntry {
                    alias: alias.alias,
                    alias_normalized: alias.alias_normalized,
                },
            );
        }

        normalized_aliases
    }
}

/// Seam over `RepositorySet` so consumers (e.g. `VersionSelector`) can receive a mockable
/// repository set in tests. The concrete `RepositorySet` implements it by delegating to its
/// inherent methods. New consumers may extend this trait additively.
pub trait RepositorySetInterface: std::fmt::Debug {
    fn find_packages(
        &self,
        name: &BStr,
        constraint: Option<AnyConstraint>,
        flags: i64,
    ) -> anyhow::Result<Vec<BasePackageHandle>>;
}

impl RepositorySetInterface for RepositorySet {
    fn find_packages(
        &self,
        name: &BStr,
        constraint: Option<AnyConstraint>,
        flags: i64,
    ) -> anyhow::Result<Vec<BasePackageHandle>> {
        RepositorySet::find_packages(self, name, constraint, flags)
    }
}

#[derive(Debug)]
pub struct SecurityAdvisoriesResult {
    pub advisories: IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>>,
    pub unreachable_repos: Vec<BString>,
}
