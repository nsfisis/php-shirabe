//! ref: composer/src/Composer/Repository/RepositorySet.php

use std::any::Any;

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_php_shim::{
    LogicException, PhpMixed, RuntimeException, array_merge, array_merge_recursive, ksort,
    strtolower,
};
use shirabe_semver::constraint::constraint::Constraint;
use shirabe_semver::constraint::constraint_interface::ConstraintInterface;
use shirabe_semver::constraint::match_all_constraint::MatchAllConstraint;
use shirabe_semver::constraint::multi_constraint::MultiConstraint;

use crate::advisory::partial_security_advisory::PartialSecurityAdvisory;
use crate::advisory::security_advisory::SecurityAdvisory;
use crate::dependency_resolver::pool::Pool;
use crate::dependency_resolver::pool_builder::PoolBuilder;
use crate::dependency_resolver::pool_optimizer::PoolOptimizer;
use crate::dependency_resolver::request::Request;
use crate::dependency_resolver::security_advisory_pool_filter::SecurityAdvisoryPoolFilter;
use crate::downloader::transport_exception::TransportException;
use crate::event_dispatcher::event_dispatcher::EventDispatcher;
use crate::io::io_interface::IOInterface;
use crate::io::null_io::NullIO;
use crate::package::alias_package::AliasPackage;
use crate::package::base_package::BasePackage;
use crate::package::complete_alias_package::CompleteAliasPackage;
use crate::package::complete_package::CompletePackage;
use crate::package::package_interface::PackageInterface;
use crate::package::version::stability_filter::StabilityFilter;
use crate::repository::advisory_provider_interface::AdvisoryProviderInterface;
use crate::repository::composite_repository::CompositeRepository;
use crate::repository::installed_repository::InstalledRepository;
use crate::repository::installed_repository_interface::InstalledRepositoryInterface;
use crate::repository::lock_array_repository::LockArrayRepository;
use crate::repository::platform_repository::PlatformRepository;
use crate::repository::repository_interface::{FindPackageConstraint, RepositoryInterface};

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
    pub(crate) repositories: Vec<Box<dyn RepositoryInterface>>,

    /// @var int[] array of stability => BasePackage::STABILITY_* value
    /// @phpstan-var array<key-of<BasePackage::STABILITIES>, BasePackage::STABILITY_*>
    pub(crate) acceptable_stabilities: IndexMap<String, i64>,

    /// @var int[] array of package name => BasePackage::STABILITY_* value
    /// @phpstan-var array<string, BasePackage::STABILITY_*>
    pub(crate) stability_flags: IndexMap<String, i64>,

    /// @var ConstraintInterface[]
    /// @phpstan-var array<string, ConstraintInterface>
    pub(crate) root_requires: IndexMap<String, Box<dyn ConstraintInterface>>,

    /// @var array<string, ConstraintInterface>
    pub(crate) temporary_constraints: IndexMap<String, Box<dyn ConstraintInterface>>,

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
        mut root_requires: IndexMap<String, Box<dyn ConstraintInterface>>,
        temporary_constraints: IndexMap<String, Box<dyn ConstraintInterface>>,
    ) -> Self {
        let root_aliases = Self::get_root_aliases_per_package(root_aliases);

        let mut acceptable_stabilities: IndexMap<String, i64> = IndexMap::new();
        // PHP: foreach (BasePackage::STABILITIES as $stability => $value)
        let stabilities = crate::package::base_package::STABILITIES.clone();
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
    pub fn get_root_requires(&self) -> &IndexMap<String, Box<dyn ConstraintInterface>> {
        &self.root_requires
    }

    /// @return array<string, ConstraintInterface> Runtime temporary constraints that will be used to filter packages
    pub fn get_temporary_constraints(&self) -> &IndexMap<String, Box<dyn ConstraintInterface>> {
        &self.temporary_constraints
    }

    /// Adds a repository to this repository set
    ///
    /// The first repos added have a higher priority. As soon as a package is found in any
    /// repository the search for that package ends, and following repos will not be consulted.
    ///
    /// @param RepositoryInterface $repo A package repository
    pub fn add_repository(&mut self, repo: Box<dyn RepositoryInterface>) -> Result<()> {
        if self.locked {
            return Err(RuntimeException {
                message: "Pool has already been created from this repository set, it cannot be modified anymore.".to_string(),
                code: 0,
            }
            .into());
        }

        let repos: Vec<Box<dyn RepositoryInterface>> = if let Some(composite) =
            (repo.as_any() as &dyn Any).downcast_ref::<CompositeRepository>()
        {
            // TODO(phase-b): clone composite.get_repositories() — Box<dyn RepositoryInterface> cloning
            composite
                .get_repositories()
                .iter()
                .map(|r| r.clone_box())
                .collect()
        } else {
            vec![repo]
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
        constraint: Option<Box<dyn ConstraintInterface>>,
        flags: i64,
    ) -> Vec<Box<BasePackage>> {
        let ignore_stability = (flags & Self::ALLOW_UNACCEPTABLE_STABILITIES) != 0;
        let load_from_all_repos = (flags & Self::ALLOW_SHADOWED_REPOSITORIES) != 0;

        let mut packages: Vec<Vec<Box<BasePackage>>> = vec![];
        if load_from_all_repos {
            for repository in &self.repositories {
                // PHP: $repository->findPackages($name, $constraint) ?: []
                let constraint_clone = constraint
                    .as_ref()
                    .map(|c| FindPackageConstraint::Constraint(c.clone_box()));
                let found = repository.find_packages(name.to_string(), constraint_clone);
                packages.push(found);
            }
        } else {
            'outer: for repository in &self.repositories {
                let mut name_map: IndexMap<String, Option<Box<dyn ConstraintInterface>>> =
                    IndexMap::new();
                name_map.insert(name.to_string(), constraint.as_ref().map(|c| c.clone_box()));
                let acceptable = if ignore_stability {
                    // PHP: BasePackage::STABILITIES
                    crate::package::base_package::STABILITIES
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
                );

                packages.push(result.packages);
                for name_found in result.names_found {
                    // avoid loading the same package again from other repositories once it has been found
                    if name == name_found {
                        break 'outer;
                    }
                }
            }
        }

        // PHP: $candidates = $packages ? array_merge(...$packages) : [];
        let candidates: Vec<Box<BasePackage>> = if !packages.is_empty() {
            packages.into_iter().flatten().collect()
        } else {
            vec![]
        };

        // when using loadPackages above (!$loadFromAllRepos) the repos already filter for stability so no need to do it again
        if ignore_stability || !load_from_all_repos {
            return candidates;
        }

        let mut result: Vec<Box<BasePackage>> = vec![];
        for candidate in candidates {
            if self.is_package_acceptable(&candidate.get_names(true), candidate.get_stability()) {
                result.push(candidate);
            }
        }

        result
    }

    /// @param string[] $packageNames
    /// @return ($allowPartialAdvisories is true ? array{advisories: array<string, array<PartialSecurityAdvisory|SecurityAdvisory>>, unreachableRepos: array<string>} : array{advisories: array<string, array<SecurityAdvisory>>, unreachableRepos: array<string>})
    pub fn get_security_advisories(
        &self,
        package_names: Vec<String>,
        allow_partial_advisories: bool,
        ignore_unreachable: bool,
    ) -> Result<SecurityAdvisoriesResult> {
        let mut map: IndexMap<String, Box<dyn ConstraintInterface>> = IndexMap::new();
        for name in &package_names {
            map.insert(name.clone(), Box::new(MatchAllConstraint::new()));
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
        packages: Vec<Box<dyn PackageInterface>>,
        allow_partial_advisories: bool,
        ignore_unreachable: bool,
    ) -> Result<SecurityAdvisoriesResult> {
        let mut map: IndexMap<String, Box<dyn ConstraintInterface>> = IndexMap::new();
        for package in packages {
            // ignore root alias versions as they are not actual package versions and should not matter when it comes to vulnerabilities
            if let Some(alias) = (package.as_any() as &dyn Any).downcast_ref::<AliasPackage>() {
                if alias.is_root_package_alias() {
                    continue;
                }
            }
            let name = package.get_name().to_string();
            if map.contains_key(&name) {
                // TODO(phase-b): MultiConstraint::new signature
                let existing = map.shift_remove(&name).unwrap();
                map.insert(
                    name,
                    Box::new(MultiConstraint::new(
                        vec![
                            Box::new(Constraint::new("=", package.get_version())),
                            existing,
                        ],
                        false,
                    )),
                );
            } else {
                map.insert(name, Box::new(Constraint::new("=", package.get_version())));
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
        package_constraint_map: IndexMap<String, Box<dyn ConstraintInterface>>,
        allow_partial_advisories: bool,
        ignore_unreachable: bool,
        unreachable_repos: &mut Vec<String>,
    ) -> Result<IndexMap<String, Vec<PartialSecurityAdvisory>>> {
        let mut repo_advisories: Vec<IndexMap<String, Vec<PartialSecurityAdvisory>>> = vec![];
        for repository in &self.repositories {
            // TODO(phase-b): use anyhow::Result<Result<T, E>> to model PHP try/catch
            let attempt: Result<()> = (|| -> Result<()> {
                let Some(advisory_repo) = repository.as_advisory_provider() else {
                    return Ok(());
                };
                if !advisory_repo.has_security_advisories() {
                    return Ok(());
                }

                let result = advisory_repo.get_security_advisories(
                    // TODO(phase-b): clone package_constraint_map values
                    todo!("clone package_constraint_map"),
                    allow_partial_advisories,
                )?;
                repo_advisories.push(result.advisories);
                Ok(())
            })();
            match attempt {
                Ok(_) => {}
                Err(e) => {
                    // TODO(phase-b): downcast e to \Composer\Downloader\TransportException
                    let _te: &TransportException = todo!("downcast e to TransportException");
                    if !ignore_unreachable {
                        return Err(e);
                    }
                    unreachable_repos.push(e.to_string());
                }
            }
        }

        let mut advisories = if !repo_advisories.is_empty() {
            // PHP: array_merge_recursive([], ...$repoAdvisories)
            // TODO(phase-b): array_merge_recursive signature expects PhpMixed arguments
            todo!("array_merge_recursive across repo_advisories")
        } else {
            IndexMap::new()
        };
        ksort(&mut advisories);

        Ok(advisories)
    }

    /// @return array[] an array with the provider name as key and value of array('name' => '...', 'description' => '...', 'type' => '...')
    /// @phpstan-return array<string, array{name: string, description: string|null, type: string}>
    pub fn get_providers(
        &self,
        package_name: &str,
    ) -> IndexMap<String, crate::repository::repository_interface::ProviderInfo> {
        let mut providers: IndexMap<String, crate::repository::repository_interface::ProviderInfo> =
            IndexMap::new();
        for repository in &self.repositories {
            let repo_providers = repository.get_providers(package_name.to_string());
            if !repo_providers.is_empty() {
                providers.extend(repo_providers);
            }
        }

        providers
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
    ///
    /// @param list<string>      $ignoredTypes Packages of those types are ignored
    /// @param list<string>|null $allowedTypes Only packages of those types are allowed if set to non-null
    pub fn create_pool(
        &mut self,
        request: Request,
        io: Box<dyn IOInterface>,
        event_dispatcher: Option<EventDispatcher>,
        pool_optimizer: Option<PoolOptimizer>,
        ignored_types: Vec<String>,
        allowed_types: Option<Vec<String>>,
        security_advisory_pool_filter: Option<SecurityAdvisoryPoolFilter>,
    ) -> Result<Pool> {
        let mut pool_builder = PoolBuilder::new(
            self.acceptable_stabilities.clone(),
            self.stability_flags.clone(),
            // TODO(phase-b): clone root_aliases into PoolBuilder's expected type
            todo!("self.root_aliases.clone()"),
            self.root_references.clone(),
            io,
            event_dispatcher,
            pool_optimizer,
            // TODO(phase-b): clone temporary_constraints
            todo!("self.temporary_constraints.clone()"),
            security_advisory_pool_filter,
        );
        pool_builder.set_ignored_types(ignored_types);
        pool_builder.set_allowed_types(allowed_types);

        for repo in &self.repositories {
            let is_installed = (repo.as_any() as &dyn Any)
                .downcast_ref::<dyn InstalledRepositoryInterface>()
                .is_some()
                || (repo.as_any() as &dyn Any)
                    .downcast_ref::<InstalledRepository>()
                    .is_some();
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

        // TODO(phase-b): pass repositories by reference; pool_builder.build_pool expects &Vec<Box<dyn RepositoryInterface>>
        pool_builder.build_pool(&self.repositories, &request)
    }

    /// Create a pool for dependency resolution from the packages in this repository set.
    pub fn create_pool_with_all_packages(&mut self) -> Result<Pool> {
        for repo in &self.repositories {
            let is_installed = (repo.as_any() as &dyn Any)
                .downcast_ref::<dyn InstalledRepositoryInterface>()
                .is_some()
                || (repo.as_any() as &dyn Any)
                    .downcast_ref::<InstalledRepository>()
                    .is_some();
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

        let mut packages: Vec<Box<BasePackage>> = vec![];
        for repository in &self.repositories {
            for mut package in repository.get_packages() {
                let name = package.get_name().to_string();
                let version = package.get_version().to_string();
                packages.push(package.clone_box());

                if let Some(versions) = self.root_aliases.get(&name) {
                    if let Some(alias) = versions.get(&version) {
                        while let Some(alias_pkg) =
                            (package.as_any() as &dyn Any).downcast_ref::<AliasPackage>()
                        {
                            package = alias_pkg.get_alias_of().clone_box();
                        }
                        let alias_package: Box<BasePackage> = if (package.as_any() as &dyn Any)
                            .downcast_ref::<CompletePackage>()
                            .is_some()
                        {
                            // TODO(phase-b): construct CompleteAliasPackage and box as BasePackage
                            todo!(
                                "new CompleteAliasPackage(package, alias.alias_normalized, alias.alias)"
                            )
                        } else {
                            // TODO(phase-b): construct AliasPackage and box as BasePackage
                            todo!("new AliasPackage(package, alias.alias_normalized, alias.alias)")
                        };
                        // TODO(phase-b): set_root_package_alias on the wrapper
                        todo!("alias_package.set_root_package_alias(true)");
                        #[allow(unreachable_code)]
                        packages.push(alias_package);
                    }
                }
            }
        }

        // TODO(phase-b): Pool::new signature
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
        locked_repo: Option<LockArrayRepository>,
    ) -> Result<Pool> {
        // TODO unify this with above in some simpler version without "request"?
        self.create_pool_for_packages(vec![package_name.to_string()], locked_repo)
    }

    /// @param string[] $packageNames
    pub fn create_pool_for_packages(
        &mut self,
        package_names: Vec<String>,
        locked_repo: Option<LockArrayRepository>,
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
            // TODO(phase-b): Request::restrict_packages signature
            request.restrict_packages(allowed_packages);
        }

        self.create_pool(
            request,
            Box::new(NullIO::new()),
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
            normalized_aliases
                .entry(alias.package)
                .or_insert_with(IndexMap::new)
                .insert(
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
    pub advisories: IndexMap<String, Vec<PartialSecurityAdvisory>>,
    pub unreachable_repos: Vec<String>,
}
