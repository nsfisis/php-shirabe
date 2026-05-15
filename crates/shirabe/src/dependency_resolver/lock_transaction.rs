//! ref: composer/src/Composer/DependencyResolver/LockTransaction.php

use std::any::Any;

use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;

use crate::dependency_resolver::decisions::Decisions;
use crate::dependency_resolver::pool::Pool;
use crate::dependency_resolver::transaction::Transaction;
use crate::package::alias_package::AliasPackage;
use crate::package::package::Package;
use crate::package::package_interface::PackageInterface;

#[derive(Debug)]
pub struct LockTransaction {
    inner: Transaction,
    /// packages in current lock file, platform repo or otherwise present
    /// Indexed by spl_object_hash
    pub(crate) present_map: IndexMap<String, Box<dyn PackageInterface>>,
    /// Packages which cannot be mapped, platform repo, root package, other fixed repos
    /// Indexed by package id
    pub(crate) unlockable_map: IndexMap<i64, Box<dyn PackageInterface>>,
    pub(crate) result_packages: IndexMap<String, Vec<Box<dyn PackageInterface>>>,
}

impl LockTransaction {
    pub fn new(
        pool: &Pool,
        present_map: IndexMap<String, Box<dyn PackageInterface>>,
        unlockable_map: IndexMap<i64, Box<dyn PackageInterface>>,
        decisions: &Decisions,
    ) -> Self {
        let mut this = Self {
            inner: Transaction::default(),
            present_map,
            unlockable_map,
            result_packages: IndexMap::new(),
        };
        this.set_result_packages(pool, decisions);
        let all = this.result_packages.get("all").cloned().unwrap_or_default();
        let present: Vec<Box<dyn PackageInterface>> = this.present_map.values()
            .map(|p| p.clone_box())
            .collect();
        this.inner = Transaction::new(present, all);
        this
    }

    pub fn set_result_packages(&mut self, pool: &Pool, decisions: &Decisions) {
        let mut result_packages: IndexMap<String, Vec<Box<dyn PackageInterface>>> = IndexMap::new();
        result_packages.insert("all".to_string(), vec![]);
        result_packages.insert("non-dev".to_string(), vec![]);
        result_packages.insert("dev".to_string(), vec![]);

        for decision in decisions.iter() {
            let literal = decision[Decisions::DECISION_LITERAL];

            if literal > 0 {
                let package = pool.literal_to_package(literal);

                result_packages.get_mut("all").unwrap().push(package.clone_box());
                if !self.unlockable_map.contains_key(&package.get_id()) {
                    result_packages.get_mut("non-dev").unwrap().push(package.clone_box());
                }
            }
        }

        self.result_packages = result_packages;
    }

    pub fn set_non_dev_packages(&mut self, extraction_result: &LockTransaction) {
        let packages = extraction_result.get_new_lock_packages(false, false);

        let non_dev = self.result_packages.remove("non-dev").unwrap_or_default();
        self.result_packages.insert("dev".to_string(), non_dev);
        self.result_packages.insert("non-dev".to_string(), vec![]);

        let mut remaining_dev = self.result_packages.remove("dev").unwrap_or_default();
        for package in &packages {
            let mut i = 0;
            while i < remaining_dev.len() {
                if package.get_name() == remaining_dev[i].get_name() {
                    let result_package = remaining_dev.remove(i);
                    self.result_packages.get_mut("non-dev").unwrap().push(result_package);
                } else {
                    i += 1;
                }
            }
        }
        self.result_packages.insert("dev".to_string(), remaining_dev);
    }

    pub fn get_new_lock_packages(&self, dev_mode: bool, update_mirrors: bool) -> Vec<Box<dyn PackageInterface>> {
        let key = if dev_mode { "dev" } else { "non-dev" };
        let mut packages = vec![];

        let source = self.result_packages.get(key).map(|v| v.as_slice()).unwrap_or_default();
        for package in source {
            if (package.as_any() as &dyn Any).downcast_ref::<AliasPackage>().is_some() {
                continue;
            }

            if update_mirrors && !self.present_map.contains_key(&package.get_object_hash()) {
                let updated = self.update_mirror_and_urls(package.as_ref());
                packages.push(updated);
            } else {
                packages.push(package.clone_box());
            }
        }

        packages
    }

    fn update_mirror_and_urls(&self, package: &dyn PackageInterface) -> Box<dyn PackageInterface> {
        for present_package in self.present_map.values() {
            if package.get_name() != present_package.get_name() {
                continue;
            }

            if package.get_version() != present_package.get_version() {
                continue;
            }

            if present_package.get_source_reference().is_none() {
                continue;
            }

            if present_package.get_source_type() != package.get_source_type() {
                continue;
            }

            if let Some(concrete_pkg) = (present_package.as_any() as &dyn Any).downcast_ref::<Package>() {
                concrete_pkg.set_source_url(package.get_source_url());
                concrete_pkg.set_source_mirrors(package.get_source_mirrors());
            }

            if present_package.get_dist_type() != package.get_dist_type() {
                return present_package.clone_box();
            }

            if package.get_dist_url().is_some()
                && present_package.get_dist_reference().is_some()
                && Preg::is_match(r"{^https?://(?:(?:www\.)?bitbucket\.org|(api\.)?github\.com|(?:www\.)?gitlab\.com)/}i", package.get_dist_url().unwrap()).unwrap_or(false)
            {
                let new_dist_url = Preg::replace(
                    r"{(?<=/|sha=)[a-f0-9]{40}(?=/|$)}i",
                    present_package.get_dist_reference().unwrap(),
                    package.get_dist_url().unwrap(),
                    -1,
                ).unwrap_or_else(|_| package.get_dist_url().unwrap().to_string());
                present_package.set_dist_url(&new_dist_url);
            }
            present_package.set_dist_mirrors(package.get_dist_mirrors());

            return present_package.clone_box();
        }

        package.clone_box()
    }

    pub fn get_aliases(
        &self,
        aliases: Vec<IndexMap<String, String>>,
    ) -> Vec<IndexMap<String, String>> {
        let mut used_aliases = vec![];
        let mut remaining_aliases = aliases;

        if let Some(all_packages) = self.result_packages.get("all") {
            for package in all_packages {
                if (package.as_any() as &dyn Any).downcast_ref::<AliasPackage>().is_some() {
                    let mut i = 0;
                    while i < remaining_aliases.len() {
                        if remaining_aliases[i].get("package").map(|s| s.as_str()) == Some(package.get_name()) {
                            used_aliases.push(remaining_aliases.remove(i));
                        } else {
                            i += 1;
                        }
                    }
                }
            }
        }

        used_aliases.sort_by(|a, b| {
            let a_pkg = a.get("package").map(|s| s.as_str()).unwrap_or("");
            let b_pkg = b.get("package").map(|s| s.as_str()).unwrap_or("");
            a_pkg.cmp(b_pkg)
        });

        used_aliases
    }

    pub fn get_operations(&self) -> &Vec<Box<dyn crate::dependency_resolver::operation::operation_interface::OperationInterface>> {
        self.inner.get_operations()
    }
}
