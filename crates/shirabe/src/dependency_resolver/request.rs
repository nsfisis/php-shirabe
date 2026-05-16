//! ref: composer/src/Composer/DependencyResolver/Request.php

use indexmap::IndexMap;
use shirabe_php_shim::{spl_object_hash, strtolower, LogicException};
use shirabe_semver::constraint::constraint_interface::ConstraintInterface;
use shirabe_semver::constraint::match_all_constraint::MatchAllConstraint;

use crate::package::base_package::BasePackage;
use crate::package::package_interface::PackageInterface;
use crate::repository::lock_array_repository::LockArrayRepository;

/// Identifies a partial update for listed packages only, all dependencies will remain at locked versions
pub const UPDATE_ONLY_LISTED: i64 = 0;

/// Identifies a partial update for listed packages and recursively all their dependencies, however
/// dependencies also directly required by the root composer.json and their dependencies will remain
/// at the locked version.
pub const UPDATE_LISTED_WITH_TRANSITIVE_DEPS_NO_ROOT_REQUIRE: i64 = 1;

/// Identifies a partial update for listed packages and recursively all their dependencies, even
/// dependencies also directly required by the root composer.json will be updated.
pub const UPDATE_LISTED_WITH_TRANSITIVE_DEPS: i64 = 2;

/// Represents the value of updateAllowTransitiveDependencies, which is false|UPDATE_* in PHP.
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateAllowTransitiveDeps {
    /// Corresponds to PHP false (initial value)
    False,
    UpdateOnlyListed,
    UpdateListedWithTransitiveDepsNoRootRequire,
    UpdateListedWithTransitiveDeps,
}

#[derive(Debug)]
pub struct Request {
    pub(crate) locked_repository: Option<LockArrayRepository>,
    pub(crate) requires: IndexMap<String, Box<dyn ConstraintInterface>>,
    pub(crate) fixed_packages: IndexMap<String, BasePackage>,
    pub(crate) locked_packages: IndexMap<String, BasePackage>,
    pub(crate) fixed_locked_packages: IndexMap<String, BasePackage>,
    pub(crate) update_allow_list: Vec<String>,
    pub(crate) update_allow_transitive_dependencies: UpdateAllowTransitiveDeps,
    restrict_packages: Option<Vec<String>>,
}

impl Request {
    pub fn new(locked_repository: Option<LockArrayRepository>) -> Self {
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
        constraint: Option<Box<dyn ConstraintInterface>>,
    ) -> anyhow::Result<()> {
        let package_name = strtolower(package_name);
        let constraint = constraint.unwrap_or_else(|| Box::new(MatchAllConstraint::new()));
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
    pub fn fix_package(&mut self, package: BasePackage) {
        let hash = spl_object_hash(&package);
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
    pub fn lock_package(&mut self, package: BasePackage) {
        let hash = spl_object_hash(&package);
        self.locked_packages.insert(hash, package);
    }

    /// Marks a locked package fixed. So it's treated irremovable like a platform package.
    ///
    /// This is necessary for the composer install step which verifies the lock file integrity and
    /// should not allow removal of any packages. At the same time lock packages there cannot simply
    /// be marked fixed, as error reporting would then report them as platform packages, so this
    /// still marks them as locked packages at the same time.
    pub fn fix_locked_package(&mut self, package: BasePackage) {
        let hash = spl_object_hash(&package);
        self.fixed_packages.insert(hash.clone(), package.clone());
        self.fixed_locked_packages.insert(hash, package);
    }

    pub fn unlock_package(&mut self, package: &BasePackage) {
        self.locked_packages.remove(&spl_object_hash(package));
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

    pub fn get_requires(&self) -> &IndexMap<String, Box<dyn ConstraintInterface>> {
        &self.requires
    }

    pub fn get_fixed_packages(&self) -> &IndexMap<String, BasePackage> {
        &self.fixed_packages
    }

    pub fn is_fixed_package(&self, package: &BasePackage) -> bool {
        self.fixed_packages.contains_key(&spl_object_hash(package))
    }

    pub fn get_locked_packages(&self) -> &IndexMap<String, BasePackage> {
        &self.locked_packages
    }

    pub fn is_locked_package(&self, package: &dyn PackageInterface) -> bool {
        let hash = spl_object_hash(package);
        self.locked_packages.contains_key(&hash) || self.fixed_locked_packages.contains_key(&hash)
    }

    pub fn get_fixed_or_locked_packages(&self) -> IndexMap<String, BasePackage> {
        let mut result = self.fixed_packages.clone();
        result.extend(self.locked_packages.clone());
        result
    }

    /// @TODO look into removing the packageIds option, the only place true is used
    ///       is for the installed map in the solver problems.
    ///       Some locked packages may not be in the pool,
    ///       so they have a package->id of -1
    pub fn get_present_map(&self, package_ids: bool) -> IndexMap<String, BasePackage> {
        let mut present_map: IndexMap<String, BasePackage> = IndexMap::new();

        if let Some(ref locked_repository) = self.locked_repository {
            for package in locked_repository.get_packages() {
                let key = if package_ids {
                    package.get_id().to_string()
                } else {
                    spl_object_hash(&package)
                };
                present_map.insert(key, package);
            }
        }

        for (_, package) in &self.fixed_packages {
            let key = if package_ids {
                package.get_id().to_string()
            } else {
                spl_object_hash(package)
            };
            present_map.insert(key, package.clone());
        }

        present_map
    }

    pub fn get_fixed_packages_map(&self) -> IndexMap<i64, BasePackage> {
        let mut fixed_packages_map: IndexMap<i64, BasePackage> = IndexMap::new();
        for (_, package) in &self.fixed_packages {
            fixed_packages_map.insert(package.get_id(), package.clone());
        }
        fixed_packages_map
    }

    pub fn get_locked_repository(&self) -> Option<&LockArrayRepository> {
        self.locked_repository.as_ref()
    }

    /// Restricts the pool builder from loading other packages than those listed here.
    pub fn restrict_packages(&mut self, names: Vec<String>) {
        self.restrict_packages = Some(names);
    }

    pub fn get_restricted_packages(&self) -> Option<&Vec<String>> {
        self.restrict_packages.as_ref()
    }
}
