//! ref: composer/src/Composer/DependencyResolver/DefaultPolicy.php

use std::any::Any;
use std::cell::RefCell;

use indexmap::IndexMap;
use shirabe_semver::compiling_matcher::CompilingMatcher;
use shirabe_semver::constraint::constraint::Constraint;

use crate::dependency_resolver::policy_interface::PolicyInterface;
use crate::dependency_resolver::pool::Pool;
use crate::package::alias_package::AliasPackage;
use crate::package::base_package::{BasePackage, STABILITIES};
use crate::package::package_interface::PackageInterface;
use crate::util::platform::Platform;

#[derive(Debug)]
pub struct DefaultPolicy {
    prefer_stable: bool,
    prefer_lowest: bool,
    prefer_dev_over_prerelease: bool,
    preferred_versions: Option<IndexMap<String, String>>,
    preferred_package_result_cache_per_pool: RefCell<IndexMap<i64, IndexMap<String, Vec<i64>>>>,
    sorting_cache_per_pool: RefCell<IndexMap<i64, IndexMap<String, i64>>>,
}

impl DefaultPolicy {
    pub fn new(
        prefer_stable: bool,
        prefer_lowest: bool,
        preferred_versions: Option<IndexMap<String, String>>,
    ) -> Self {
        Self {
            prefer_stable,
            prefer_lowest,
            preferred_versions,
            prefer_dev_over_prerelease: Platform::get_env("COMPOSER_PREFER_DEV_OVER_PRERELEASE")
                .map(|v| !v.is_empty())
                .unwrap_or(false),
            preferred_package_result_cache_per_pool: RefCell::new(IndexMap::new()),
            sorting_cache_per_pool: RefCell::new(IndexMap::new()),
        }
    }

    pub fn compare_by_priority(
        &self,
        pool: &Pool,
        a: &BasePackage,
        b: &BasePackage,
        required_package: Option<String>,
        ignore_replace: bool,
    ) -> i64 {
        if a.get_name() == b.get_name() {
            let a_aliased = (a.as_any() as &dyn Any).downcast_ref::<AliasPackage>().is_some();
            let b_aliased = (b.as_any() as &dyn Any).downcast_ref::<AliasPackage>().is_some();
            if a_aliased && !b_aliased {
                return -1;
            }
            if !a_aliased && b_aliased {
                return 1;
            }
        }

        if !ignore_replace {
            if self.replaces(a, b) {
                return 1;
            }
            if self.replaces(b, a) {
                return -1;
            }

            if let Some(ref required_package) = required_package {
                if let Some(pos) = required_package.find('/') {
                    let required_vendor = &required_package[..pos];
                    let a_is_same_vendor = a.get_name().starts_with(required_vendor);
                    let b_is_same_vendor = b.get_name().starts_with(required_vendor);
                    if b_is_same_vendor != a_is_same_vendor {
                        return if a_is_same_vendor { -1 } else { 1 };
                    }
                }
            }
        }

        if a.id == b.id {
            return 0;
        }

        if a.id < b.id { -1 } else { 1 }
    }

    pub(crate) fn group_literals_by_name(
        &self,
        pool: &Pool,
        literals: &[i64],
    ) -> IndexMap<String, Vec<i64>> {
        let mut packages: IndexMap<String, Vec<i64>> = IndexMap::new();
        for &literal in literals {
            let package_name = pool.literal_to_package(literal).get_name().to_string();
            packages.entry(package_name).or_default().push(literal);
        }
        packages
    }

    pub(crate) fn prune_to_best_version(&self, pool: &Pool, literals: Vec<i64>) -> Vec<i64> {
        if let Some(ref preferred_versions) = self.preferred_versions {
            let name = pool.literal_to_package(literals[0]).get_name().to_string();
            if let Some(preferred_version) = preferred_versions.get(&name) {
                let best_literals: Vec<i64> = literals
                    .iter()
                    .copied()
                    .filter(|&literal| pool.literal_to_package(literal).get_version() == preferred_version)
                    .collect();
                if !best_literals.is_empty() {
                    return best_literals;
                }
            }
        }

        let operator = if self.prefer_lowest { "<" } else { ">" };
        let mut best_literals = vec![literals[0]];
        let mut best_package = pool.literal_to_package(literals[0]);
        for (i, &literal) in literals.iter().enumerate() {
            if i == 0 {
                continue;
            }
            let package = pool.literal_to_package(literal);
            if self.version_compare(package, best_package, operator) {
                best_package = package;
                best_literals = vec![literal];
            } else if self.version_compare(package, best_package, "==") {
                best_literals.push(literal);
            }
        }
        best_literals
    }

    pub(crate) fn prune_remote_aliases(&self, pool: &Pool, literals: Vec<i64>) -> Vec<i64> {
        let mut has_local_alias = false;

        for &literal in &literals {
            let package = pool.literal_to_package(literal);
            if let Some(alias_pkg) = (package.as_any() as &dyn Any).downcast_ref::<AliasPackage>() {
                if alias_pkg.is_root_package_alias() {
                    has_local_alias = true;
                    break;
                }
            }
        }

        if !has_local_alias {
            return literals;
        }

        let mut selected = vec![];
        for &literal in &literals {
            let package = pool.literal_to_package(literal);
            if let Some(alias_pkg) = (package.as_any() as &dyn Any).downcast_ref::<AliasPackage>() {
                if alias_pkg.is_root_package_alias() {
                    selected.push(literal);
                }
            }
        }
        selected
    }

    pub(crate) fn replaces(&self, source: &BasePackage, target: &BasePackage) -> bool {
        for link in source.get_replaces().values() {
            if link.get_target() == target.get_name() {
                return true;
            }
        }
        false
    }
}

impl PolicyInterface for DefaultPolicy {
    fn version_compare(&self, a: &dyn PackageInterface, b: &dyn PackageInterface, operator: &str) -> bool {
        if self.prefer_stable {
            let stab_a = a.get_stability().to_string();
            let stab_b = b.get_stability().to_string();
            if stab_a != stab_b {
                let (mut stab_a, mut stab_b) = (stab_a, stab_b);
                if self.prefer_lowest && self.prefer_dev_over_prerelease && "stable" != stab_a && "stable" != stab_b {
                    if stab_a == "dev" { stab_a = "stable".to_string(); }
                    if stab_b == "dev" { stab_b = "stable".to_string(); }
                }
                return STABILITIES.get(stab_a.as_str()).copied().unwrap_or(0)
                    < STABILITIES.get(stab_b.as_str()).copied().unwrap_or(0);
            }
        }

        if (a.is_dev() && a.get_version().starts_with("dev-"))
            || (b.is_dev() && b.get_version().starts_with("dev-"))
        {
            let constraint = Constraint::new(operator, b.get_version());
            let version = Constraint::new("==", a.get_version());
            return constraint.match_specific(&version, true);
        }

        CompilingMatcher::r#match(
            &Constraint::new(operator, b.get_version()),
            Constraint::OP_EQ,
            a.get_version(),
        )
    }

    fn select_preferred_packages(
        &self,
        pool: &Pool,
        mut literals: Vec<i64>,
        required_package: Option<String>,
    ) -> Vec<i64> {
        literals.sort();
        let result_cache_key = format!(
            "{}{}",
            literals.iter().map(|l| l.to_string()).collect::<Vec<_>>().join(","),
            required_package.as_deref().unwrap_or("")
        );
        let pool_id = pool as *const Pool as i64;

        {
            let cache = self.preferred_package_result_cache_per_pool.borrow();
            if let Some(pool_cache) = cache.get(&pool_id) {
                if let Some(cached) = pool_cache.get(&result_cache_key) {
                    return cached.clone();
                }
            }
        }

        let mut packages = self.group_literals_by_name(pool, &literals);

        for name_literals in packages.values_mut() {
            name_literals.sort_by(|&a, &b| {
                let cache_key = format!(
                    "i{}.{}{}",
                    a,
                    b,
                    required_package.as_deref().unwrap_or("")
                );
                {
                    let cache = self.sorting_cache_per_pool.borrow();
                    if let Some(pool_cache) = cache.get(&pool_id) {
                        if let Some(&result) = pool_cache.get(&cache_key) {
                            return result.cmp(&0);
                        }
                    }
                }
                let result = self.compare_by_priority(
                    pool,
                    pool.literal_to_package(a),
                    pool.literal_to_package(b),
                    required_package.clone(),
                    true,
                );
                self.sorting_cache_per_pool
                    .borrow_mut()
                    .entry(pool_id)
                    .or_default()
                    .insert(cache_key, result);
                result.cmp(&0)
            });
        }

        for sorted_literals in packages.values_mut() {
            *sorted_literals = self.prune_to_best_version(pool, sorted_literals.clone());
            *sorted_literals = self.prune_remote_aliases(pool, sorted_literals.clone());
        }

        let mut selected: Vec<i64> = packages.into_values().flatten().collect();

        selected.sort_by(|&a, &b| {
            let cache_key = format!(
                "{}.{}{}",
                a,
                b,
                required_package.as_deref().unwrap_or("")
            );
            {
                let cache = self.sorting_cache_per_pool.borrow();
                if let Some(pool_cache) = cache.get(&pool_id) {
                    if let Some(&result) = pool_cache.get(&cache_key) {
                        return result.cmp(&0);
                    }
                }
            }
            let result = self.compare_by_priority(
                pool,
                pool.literal_to_package(a),
                pool.literal_to_package(b),
                required_package.clone(),
                false,
            );
            self.sorting_cache_per_pool
                .borrow_mut()
                .entry(pool_id)
                .or_default()
                .insert(cache_key, result);
            result.cmp(&0)
        });

        self.preferred_package_result_cache_per_pool
            .borrow_mut()
            .entry(pool_id)
            .or_default()
            .insert(result_cache_key, selected.clone());
        selected
    }
}
