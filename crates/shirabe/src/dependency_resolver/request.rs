//! ref: composer/src/Composer/DependencyResolver/Request.php

use crate::package::BasePackageHandle;
use crate::repository::LockArrayRepositoryHandle;
use crate::repository::RepositoryInterface;
use indexmap::IndexMap;
use shirabe_php_shim::{LogicException, strtolower};
use shirabe_semver::constraint::AnyConstraint;
use shirabe_semver::constraint::MatchAllConstraint;

/// Identifies a partial update for listed packages only, all dependencies will remain at locked versions
pub const UPDATE_ONLY_LISTED: i64 = 0;

/// Identifies a partial update for listed packages and recursively all their dependencies, however
/// dependencies also directly required by the root composer.json and their dependencies will remain
/// at the locked version.
pub const UPDATE_LISTED_WITH_TRANSITIVE_DEPS_NO_ROOT_REQUIRE: i64 = 1;

/// Identifies a partial update for listed packages and recursively all their dependencies, even
/// dependencies also directly required by the root composer.json will be updated.
pub const UPDATE_LISTED_WITH_TRANSITIVE_DEPS: i64 = 2;

impl Request {
    pub const UPDATE_ONLY_LISTED: i64 = UPDATE_ONLY_LISTED;
    pub const UPDATE_LISTED_WITH_TRANSITIVE_DEPS_NO_ROOT_REQUIRE: i64 =
        UPDATE_LISTED_WITH_TRANSITIVE_DEPS_NO_ROOT_REQUIRE;
    pub const UPDATE_LISTED_WITH_TRANSITIVE_DEPS: i64 = UPDATE_LISTED_WITH_TRANSITIVE_DEPS;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateAllowTransitiveDeps {
    /// Corresponds to PHP false.
    False,
    /// \Composer\DependencyResolver\Request::UPDATE_ONLY_LISTED
    UpdateOnlyListed,
    /// \Composer\DependencyResolver\Request::UPDATE_LISTED_WITH_TRANSITIVE_DEPS_NO_ROOT_REQUIRE
    UpdateListedWithTransitiveDepsNoRootRequire,
    /// \Composer\DependencyResolver\Request::UPDATE_LISTED_WITH_TRANSITIVE_DEPS
    UpdateListedWithTransitiveDeps,
}

#[derive(Debug)]
pub struct Request {
    pub(crate) locked_repository: Option<LockArrayRepositoryHandle>,
    pub(crate) requires: IndexMap<String, AnyConstraint>,
    pub(crate) fixed_packages: IndexMap<String, BasePackageHandle>,
    pub(crate) locked_packages: IndexMap<String, BasePackageHandle>,
    pub(crate) fixed_locked_packages: IndexMap<String, BasePackageHandle>,
    pub(crate) update_allow_list: Vec<String>,
    pub(crate) update_allow_transitive_dependencies: UpdateAllowTransitiveDeps,
    restrict_packages: Option<Vec<String>>,
}

impl Request {
    pub fn new(locked_repository: Option<LockArrayRepositoryHandle>) -> Self {
        Self {
            locked_repository,
            requires: IndexMap::new(),
            fixed_packages: IndexMap::new(),
            locked_packages: IndexMap::new(),
            fixed_locked_packages: IndexMap::new(),
            update_allow_list: vec![],
            update_allow_transitive_dependencies: UpdateAllowTransitiveDeps::False,
            restrict_packages: None,
        }
    }

    pub fn require_name(
        &mut self,
        package_name: &str,
        constraint: Option<AnyConstraint>,
    ) -> anyhow::Result<()> {
        let package_name = strtolower(package_name);
        let constraint = constraint.unwrap_or_else(|| MatchAllConstraint::new(None).into());
        if self.requires.contains_key(&package_name) {
            return Err(LogicException {
                message: format!(
                    "Overwriting requires seems like a bug ({} {} => {}, check why it is happening, might be a root alias",
                    package_name,
                    self.requires[&package_name].get_pretty_string(),
                    constraint.get_pretty_string()
                ),
                code: 0,
            }
            .into());
        }
        self.requires.insert(package_name, constraint);
        Ok(())
    }

    /// Mark a package as currently present and having to remain installed.
    ///
    /// This is used for platform packages which cannot be modified by Composer. A rule enforcing
    /// their installation is generated for dependency resolution. Partial updates with dependencies
    /// cannot in any way modify these packages.
    pub fn fix_package(&mut self, package: BasePackageHandle) {
        let hash = package.ptr_id().to_string();
        self.fixed_packages.insert(hash, package);
    }

    /// Mark a package as locked to a specific version but removable.
    ///
    /// This is used for lock file packages which need to be treated similar to fixed packages by
    /// the pool builder in that by default they should really only have the currently present
    /// version loaded and no remote alternatives.
    ///
    /// However unlike fixed packages there will not be a special rule enforcing their installation
    /// for the solver, so if nothing requires these packages they will be removed. Additionally in
    /// a partial update these packages can be unlocked, meaning other versions can be installed if
    /// explicitly requested as part of the update.
    pub fn lock_package(&mut self, package: BasePackageHandle) {
        let hash = package.ptr_id().to_string();
        self.locked_packages.insert(hash, package);
    }

    /// Marks a locked package fixed. So it's treated irremovable like a platform package.
    ///
    /// This is necessary for the composer install step which verifies the lock file integrity and
    /// should not allow removal of any packages. At the same time lock packages there cannot simply
    /// be marked fixed, as error reporting would then report them as platform packages, so this
    /// still marks them as locked packages at the same time.
    pub fn fix_locked_package(&mut self, package: BasePackageHandle) {
        let hash = package.ptr_id().to_string();
        self.fixed_packages.insert(hash.clone(), package.clone());
        self.fixed_locked_packages.insert(hash, package);
    }

    pub fn unlock_package(&mut self, package: BasePackageHandle) {
        self.locked_packages
            .shift_remove(&package.ptr_id().to_string());
    }

    pub fn set_update_allow_list(
        &mut self,
        update_allow_list: Vec<String>,
        update_allow_transitive_dependencies: UpdateAllowTransitiveDeps,
    ) {
        self.update_allow_list = update_allow_list;
        self.update_allow_transitive_dependencies = update_allow_transitive_dependencies;
    }

    pub fn get_update_allow_list(&self) -> &Vec<String> {
        &self.update_allow_list
    }

    pub fn get_update_allow_transitive_dependencies(&self) -> bool {
        // PHP: $this->updateAllowTransitiveDependencies !== self::UPDATE_ONLY_LISTED
        // false !== 0 is true in PHP (strict inequality, different types)
        self.update_allow_transitive_dependencies != UpdateAllowTransitiveDeps::UpdateOnlyListed
    }

    pub fn get_update_allow_transitive_root_dependencies(&self) -> bool {
        self.update_allow_transitive_dependencies
            == UpdateAllowTransitiveDeps::UpdateListedWithTransitiveDeps
    }

    pub fn get_requires(&self) -> &IndexMap<String, AnyConstraint> {
        &self.requires
    }

    pub fn get_fixed_packages(&self) -> &IndexMap<String, BasePackageHandle> {
        &self.fixed_packages
    }

    pub fn is_fixed_package(&self, package: BasePackageHandle) -> bool {
        self.fixed_packages
            .contains_key(&package.ptr_id().to_string())
    }

    pub fn get_locked_packages(&self) -> &IndexMap<String, BasePackageHandle> {
        &self.locked_packages
    }

    pub fn is_locked_package(&self, package: BasePackageHandle) -> bool {
        let hash = package.ptr_id().to_string();
        self.locked_packages.contains_key(&hash) || self.fixed_locked_packages.contains_key(&hash)
    }

    pub fn get_fixed_or_locked_packages(&self) -> IndexMap<String, BasePackageHandle> {
        let mut result: IndexMap<String, BasePackageHandle> = self
            .fixed_packages
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        result.extend(
            self.locked_packages
                .iter()
                .map(|(k, v)| (k.clone(), v.clone())),
        );
        result
    }

    /// @TODO look into removing the packageIds option, the only place true is used
    ///       is for the installed map in the solver problems.
    ///       Some locked packages may not be in the pool,
    ///       so they have a package->id of -1
    pub fn get_present_map(
        &self,
        package_ids: bool,
    ) -> anyhow::Result<IndexMap<String, crate::package::BasePackageHandle>> {
        let mut present_map: IndexMap<String, crate::package::BasePackageHandle> = IndexMap::new();

        if let Some(ref locked_repository) = self.locked_repository {
            for package in RepositoryInterface::get_packages(&mut *locked_repository.borrow_mut())?
            {
                let key = if package_ids {
                    package.get_id().to_string()
                } else {
                    package.ptr_id().to_string()
                };
                present_map.insert(key, package);
            }
        }

        for (_, package) in &self.fixed_packages {
            let key = if package_ids {
                package.get_id().to_string()
            } else {
                package.ptr_id().to_string()
            };
            present_map.insert(key, package.clone());
        }

        Ok(present_map)
    }

    pub fn get_fixed_packages_map(&self) -> IndexMap<i64, BasePackageHandle> {
        let mut fixed_packages_map: IndexMap<i64, BasePackageHandle> = IndexMap::new();
        for (_, package) in &self.fixed_packages {
            fixed_packages_map.insert(package.get_id(), package.clone());
        }
        fixed_packages_map
    }

    pub fn get_locked_repository(&self) -> Option<LockArrayRepositoryHandle> {
        self.locked_repository.clone()
    }

    /// Restricts the pool builder from loading other packages than those listed here.
    pub fn restrict_packages(&mut self, names: Vec<String>) {
        self.restrict_packages = Some(names);
    }

    pub fn get_restricted_packages(&self) -> Option<&Vec<String>> {
        self.restrict_packages.as_ref()
    }
}
