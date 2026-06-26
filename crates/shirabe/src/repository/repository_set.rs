//! ref: composer/src/Composer/Repository/RepositorySet.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_php_shim::{LogicException, RuntimeException, ksort, strtolower};
use shirabe_semver::constraint::AnyConstraint;
use shirabe_semver::constraint::MatchAllConstraint;
use shirabe_semver::constraint::MultiConstraint;
use shirabe_semver::constraint::SimpleConstraint;

use crate::advisory::AnySecurityAdvisory;
use crate::dependency_resolver::Pool;
use crate::dependency_resolver::PoolBuilder;
use crate::dependency_resolver::PoolOptimizer;
use crate::dependency_resolver::Request;
use crate::dependency_resolver::SecurityAdvisoryPoolFilter;
use crate::downloader::TransportException;
use crate::event_dispatcher::EventDispatcher;
use crate::io::IOInterface;
use crate::io::NullIO;
use crate::package::AliasPackageHandle;
use crate::package::BasePackageHandle;
use crate::package::CompleteAliasPackageHandle;
use crate::package::PackageInterfaceHandle;
use crate::package::version::StabilityFilter;
use crate::repository::CompositeRepository;
use crate::repository::InstalledRepository;
use crate::repository::LockArrayRepositoryHandle;
use crate::repository::PlatformRepository;
use crate::repository::{FindPackageConstraint, RepositoryInterfaceHandle};

#[derive(Debug, Clone)]
pub struct RootAliasEntry {
    pub alias: String,
    pub alias_normalized: String,
}

#[derive(Debug, Clone)]
pub struct RootAliasInput {
    pub package: String,
    pub version: String,
    pub alias: String,
    pub alias_normalized: String,
}

/// @see RepositoryUtils for ways to work with single repos
#[derive(Debug)]
pub struct RepositorySet {
    /// Packages are returned even though their stability does not match the required stability
    /// PHP: ALLOW_UNACCEPTABLE_STABILITIES = 1

    /// @var array[]
    /// @phpstan-var array<string, array<string, array{alias: string, alias_normalized: string}>>
    pub(crate) root_aliases: IndexMap<String, IndexMap<String, RootAliasEntry>>,

    /// @var string[]
    /// @phpstan-var array<string, string>
    pub(crate) root_references: IndexMap<String, String>,

    /// @var RepositoryInterface[]
    pub(crate) repositories: Vec<RepositoryInterfaceHandle>,

    /// @var int[] array of stability => BasePackage::STABILITY_* value
    /// @phpstan-var array<key-of<BasePackage::STABILITIES>, BasePackage::STABILITY_*>
    pub(crate) acceptable_stabilities: IndexMap<String, i64>,

    /// @var int[] array of package name => BasePackage::STABILITY_* value
    /// @phpstan-var array<string, BasePackage::STABILITY_*>
    pub(crate) stability_flags: IndexMap<String, i64>,

    /// @var ConstraintInterface[]
    /// @phpstan-var array<string, ConstraintInterface>
    pub(crate) root_requires: IndexMap<String, AnyConstraint>,

    /// @var array<string, ConstraintInterface>
    pub(crate) temporary_constraints: IndexMap<String, AnyConstraint>,

    /// @var bool
    locked: bool,
    /// @var bool
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
    ///
    /// @param key-of<BasePackage::STABILITIES> $minimumStability
    /// @param int[]  $stabilityFlags   an array of package name => BasePackage::STABILITY_* value
    /// @phpstan-param array<string, BasePackage::STABILITY_*> $stabilityFlags
    /// @param array[] $rootAliases
    /// @phpstan-param list<array{package: string, version: string, alias: string, alias_normalized: string}> $rootAliases
    /// @param string[] $rootReferences an array of package name => source reference
    /// @phpstan-param array<string, string> $rootReferences
    /// @param ConstraintInterface[] $rootRequires an array of package name => constraint from the root package
    /// @phpstan-param array<string, ConstraintInterface> $rootRequires
    /// @param array<string, ConstraintInterface> $temporaryConstraints Runtime temporary constraints that will be used to filter packages
    pub fn new(
        minimum_stability: &str,
        stability_flags: IndexMap<String, i64>,
        root_aliases: Vec<RootAliasInput>,
        root_references: IndexMap<String, String>,
        mut root_requires: IndexMap<String, AnyConstraint>,
        temporary_constraints: IndexMap<String, AnyConstraint>,
    ) -> Self {
        let root_aliases = Self::get_root_aliases_per_package(root_aliases);

        let mut acceptable_stabilities: IndexMap<String, i64> = IndexMap::new();
        // PHP: foreach (BasePackage::STABILITIES as $stability => $value)
        let stabilities = crate::package::STABILITIES.clone();
        let min_value = *stabilities.get(minimum_stability).unwrap_or(&0);
        for (stability, value) in stabilities.iter() {
            if *value <= min_value {
                acceptable_stabilities.insert(stability.to_string(), *value);
            }
        }
        // PHP: foreach ($rootRequires as $name => $constraint) { if (...) unset(...); }
        let names: Vec<String> = root_requires.keys().cloned().collect();
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

    /// @return ConstraintInterface[] an array of package name => constraint from the root package, platform requirements excluded
    /// @phpstan-return array<string, ConstraintInterface>
    pub fn get_root_requires(&self) -> &IndexMap<String, AnyConstraint> {
        &self.root_requires
    }

    /// @return array<string, ConstraintInterface> Runtime temporary constraints that will be used to filter packages
    pub fn get_temporary_constraints(&self) -> &IndexMap<String, AnyConstraint> {
        &self.temporary_constraints
    }

    /// Adds a repository to this repository set
    ///
    /// The first repos added have a higher priority. As soon as a package is found in any
    /// repository the search for that package ends, and following repos will not be consulted.
    ///
    /// @param RepositoryInterface $repo A package repository
    pub fn add_repository(&mut self, repo: RepositoryInterfaceHandle) -> Result<()> {
        if self.locked {
            return Err(RuntimeException {
                message: "Pool has already been created from this repository set, it cannot be modified anymore.".to_string(),
                code: 0,
            }
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
    ///
    /// @param  int                      $flags      any of the ALLOW_* constants from this class to tweak what is returned
    /// @return BasePackage[]
    pub fn find_packages(
        &self,
        name: &str,
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
                let mut name_map: IndexMap<String, Option<AnyConstraint>> = IndexMap::new();
                name_map.insert(name.to_string(), constraint.clone());
                let acceptable = if ignore_stability {
                    // PHP: BasePackage::STABILITIES
                    crate::package::STABILITIES
                        .iter()
                        .map(|(k, v)| (k.to_string(), *v))
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
            if self.is_package_acceptable(&candidate.get_names(true), &candidate.get_stability()) {
                result.push(candidate);
            }
        }

        Ok(result)
    }

    /// @param string[] $packageNames
    /// @return ($allowPartialAdvisories is true ? array{advisories: array<string, array<PartialSecurityAdvisory|SecurityAdvisory>>, unreachableRepos: array<string>} : array{advisories: array<string, array<SecurityAdvisory>>, unreachableRepos: array<string>})
    pub fn get_security_advisories(
        &self,
        package_names: Vec<String>,
        allow_partial_advisories: bool,
        ignore_unreachable: bool,
    ) -> Result<SecurityAdvisoriesResult> {
        let mut map: IndexMap<String, AnyConstraint> = IndexMap::new();
        for name in &package_names {
            map.insert(name.clone(), MatchAllConstraint::new(None).into());
        }

        let mut unreachable_repos: Vec<String> = vec![];
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

    /// @param PackageInterface[] $packages
    /// @return ($allowPartialAdvisories is true ? array{advisories: array<string, array<PartialSecurityAdvisory|SecurityAdvisory>>, unreachableRepos: array<string>} : array{advisories: array<string, array<SecurityAdvisory>>, unreachableRepos: array<string>})
    pub fn get_matching_security_advisories(
        &self,
        packages: Vec<PackageInterfaceHandle>,
        allow_partial_advisories: bool,
        ignore_unreachable: bool,
    ) -> Result<SecurityAdvisoriesResult> {
        let mut map: IndexMap<String, AnyConstraint> = IndexMap::new();
        for package in packages {
            // ignore root alias versions as they are not actual package versions and should not matter when it comes to vulnerabilities
            if let Some(alias) = package.as_alias()
                && alias.is_root_package_alias()
            {
                continue;
            }
            let name = package.get_name().to_string();
            if map.contains_key(&name) {
                let existing = map.shift_remove(&name).unwrap();
                map.insert(
                    name,
                    MultiConstraint::new(
                        vec![
                            AnyConstraint::Simple(SimpleConstraint::new(
                                "=".to_string(),
                                package.get_version().to_string(),
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
                    SimpleConstraint::new("=".to_string(), package.get_version().to_string(), None)
                        .into(),
                );
            }
        }

        let mut unreachable_repos: Vec<String> = vec![];
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

    /// @param array<string, ConstraintInterface> $packageConstraintMap
    /// @param array<string> &$unreachableRepos Array to store messages about unreachable repositories
    /// @return ($allowPartialAdvisories is true ? array<string, array<PartialSecurityAdvisory|SecurityAdvisory>> : array<string, array<SecurityAdvisory>>)
    fn get_security_advisories_for_constraints(
        &self,
        package_constraint_map: IndexMap<String, AnyConstraint>,
        allow_partial_advisories: bool,
        ignore_unreachable: bool,
        unreachable_repos: &mut Vec<String>,
    ) -> Result<IndexMap<String, Vec<AnySecurityAdvisory>>> {
        let mut repo_advisories: Vec<IndexMap<String, Vec<AnySecurityAdvisory>>> = vec![];
        for repository in &self.repositories {
            let attempt: Result<()> = (|| -> Result<()> {
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
                    if e.downcast_ref::<TransportException>().is_none() {
                        return Err(e);
                    }
                    if !ignore_unreachable {
                        return Err(e);
                    }
                    let message = e
                        .downcast_ref::<TransportException>()
                        .unwrap()
                        .message
                        .clone();
                    unreachable_repos.push(message);
                }
            }
        }

        let mut advisories: IndexMap<String, Vec<AnySecurityAdvisory>> = IndexMap::new();
        for repo in repo_advisories {
            for (name, list) in repo {
                advisories.entry(name).or_default().extend(list);
            }
        }
        ksort(&mut advisories);

        Ok(advisories)
    }

    /// @return array[] an array with the provider name as key and value of array('name' => '...', 'description' => '...', 'type' => '...')
    /// @phpstan-return array<string, array{name: string, description: string|null, type: string}>
    pub fn get_providers(
        &self,
        package_name: &str,
    ) -> anyhow::Result<IndexMap<String, crate::repository::ProviderInfo>> {
        let mut providers: IndexMap<String, crate::repository::ProviderInfo> = IndexMap::new();
        for repository in &self.repositories {
            let repo_providers = repository.get_providers(package_name.to_string())?;
            if !repo_providers.is_empty() {
                providers.extend(repo_providers);
            }
        }

        Ok(providers)
    }

    /// Check for each given package name whether it would be accepted by this RepositorySet in the given $stability
    ///
    /// @param string[] $names
    /// @param key-of<BasePackage::STABILITIES> $stability one of 'stable', 'RC', 'beta', 'alpha' or 'dev'
    pub fn is_package_acceptable(&self, names: &[String], stability: &str) -> bool {
        StabilityFilter::is_package_acceptable(
            &self.acceptable_stabilities,
            &self.stability_flags,
            names,
            stability,
        )
    }

    /// Create a pool for dependency resolution from the packages in this repository set.
    #[allow(clippy::too_many_arguments, reason = "to keep PHP signature")]
    pub fn create_pool(
        &mut self,
        request: &mut Request,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        event_dispatcher: Option<std::rc::Rc<std::cell::RefCell<EventDispatcher>>>,
        pool_optimizer: Option<PoolOptimizer>,
        ignored_types: Vec<String>,
        allowed_types: Option<Vec<String>>,
        security_advisory_pool_filter: Option<SecurityAdvisoryPoolFilter>,
    ) -> Result<Pool> {
        let root_aliases = self
            .root_aliases
            .iter()
            .map(|(name, versions)| {
                let versions = versions
                    .iter()
                    .map(|(version, entry)| {
                        let mut fields = IndexMap::new();
                        fields.insert("alias".to_string(), entry.alias.clone());
                        fields.insert(
                            "alias_normalized".to_string(),
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
        pool_builder.set_ignored_types(ignored_types);
        pool_builder.set_allowed_types(allowed_types);

        for repo in &self.repositories {
            let is_installed = {
                let repo_ref = repo.borrow();
                repo_ref.as_installed_repository_interface().is_some()
                    || repo_ref.as_any().is::<InstalledRepository>()
            };
            if is_installed && !self.allow_installed_repositories {
                return Err(LogicException {
                    message: "The pool can not accept packages from an installed repository"
                        .to_string(),
                    code: 0,
                }
                .into());
            }
        }

        self.locked = true;

        pool_builder.build_pool(self.repositories.clone(), request)
    }

    /// Create a pool for dependency resolution from the packages in this repository set.
    pub fn create_pool_with_all_packages(&mut self) -> Result<Pool> {
        for repo in &self.repositories {
            let is_installed = {
                let repo_ref = repo.borrow();
                repo_ref.as_installed_repository_interface().is_some()
                    || repo_ref.as_any().is::<InstalledRepository>()
            };
            if is_installed && !self.allow_installed_repositories {
                return Err(LogicException {
                    message: "The pool can not accept packages from an installed repository"
                        .to_string(),
                    code: 0,
                }
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
        package_name: &str,
        locked_repo: Option<LockArrayRepositoryHandle>,
    ) -> Result<Pool> {
        // TODO unify this with above in some simpler version without "request"?
        self.create_pool_for_packages(vec![package_name.to_string()], locked_repo)
    }

    /// @param string[] $packageNames
    pub fn create_pool_for_packages(
        &mut self,
        package_names: Vec<String>,
        locked_repo: Option<LockArrayRepositoryHandle>,
    ) -> Result<Pool> {
        let mut request = Request::new(locked_repo);

        let mut allowed_packages: Vec<String> = vec![];
        for package_name in &package_names {
            if PlatformRepository::is_platform_package(package_name) {
                return Err(LogicException {
                    message: "createPoolForPackage(s) can not be used for platform packages, as they are never loaded by the PoolBuilder which expects them to be fixed. Use createPoolWithAllPackages or pass in a proper request with the platform packages you need fixed in it.".to_string(),
                    code: 0,
                }
                .into());
            }

            request.require_name(package_name, None)?;
            allowed_packages.push(strtolower(package_name));
        }

        if !allowed_packages.is_empty() {
            request.restrict_packages(allowed_packages);
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

    /// @param array[] $aliases
    /// @phpstan-param list<array{package: string, version: string, alias: string, alias_normalized: string}> $aliases
    ///
    /// @return array<string, array<string, array{alias: string, alias_normalized: string}>>
    fn get_root_aliases_per_package(
        aliases: Vec<RootAliasInput>,
    ) -> IndexMap<String, IndexMap<String, RootAliasEntry>> {
        let mut normalized_aliases: IndexMap<String, IndexMap<String, RootAliasEntry>> =
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

#[derive(Debug)]
pub struct SecurityAdvisoriesResult {
    pub advisories: IndexMap<String, Vec<AnySecurityAdvisory>>,
    pub unreachable_repos: Vec<String>,
}
