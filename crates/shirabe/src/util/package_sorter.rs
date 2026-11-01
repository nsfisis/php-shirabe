//! ref: composer/src/Composer/Util/PackageSorter.php

use crate::package::{Link, PackageInterfaceHandle};
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_shim::{CmpOp, strnatcasecmp, version_compare};

pub struct PackageSorter;

impl PackageSorter {
    pub fn get_most_current_version(
        packages: Vec<PackageInterfaceHandle>,
    ) -> Option<PackageInterfaceHandle> {
        if packages.is_empty() {
            return None;
        }

        let mut iter = packages.into_iter();
        let mut highest = iter.next().unwrap();
        for candidate in iter {
            if candidate.is_default_branch() {
                return Some(candidate);
            }
            if version_compare(highest.get_version(), candidate.get_version(), CmpOp::Lt) {
                highest = candidate;
            }
        }

        Some(highest)
    }

    pub fn sort_packages_alphabetically(
        mut packages: Vec<PackageInterfaceHandle>,
    ) -> Vec<PackageInterfaceHandle> {
        packages.sort_by_key(|p| p.get_name());
        packages
    }

    pub fn sort_packages(
        packages: Vec<PackageInterfaceHandle>,
        weights: IndexMap<BString, i64>,
    ) -> Vec<PackageInterfaceHandle> {
        let mut usage_list: IndexMap<BString, Vec<BString>> = IndexMap::new();

        for package in &packages {
            let mut links: IndexMap<BString, Link> = (*package.get_requires()).clone();
            if let Some(root_package) = package.as_root() {
                links.extend(
                    root_package
                        .get_dev_requires()
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone())),
                );
            }
            for link in links.values() {
                usage_list
                    .entry(BString::from(link.get_target()))
                    .or_default()
                    .push(package.get_name());
            }
        }

        let mut helper = ComputeImportanceHelper {
            computing: IndexMap::new(),
            computed: IndexMap::new(),
            usage_list: &usage_list,
            weights: &weights,
        };

        let mut weighted_packages: Vec<(BString, i64, usize)> = Vec::new();
        for (index, package) in packages.iter().enumerate() {
            let name = package.get_name();
            let weight = helper.compute(name.as_ref());
            weighted_packages.push((name, weight, index));
        }

        weighted_packages.sort_by(|a, b| {
            if a.1 != b.1 {
                a.1.cmp(&b.1)
            } else {
                strnatcasecmp(&a.0, &b.0).cmp(&0)
            }
        });

        let mut packages: Vec<Option<PackageInterfaceHandle>> =
            packages.into_iter().map(Some).collect();
        weighted_packages
            .into_iter()
            .map(|(_, _, index)| packages[index].take().unwrap())
            .collect()
    }
}

struct ComputeImportanceHelper<'a> {
    computing: IndexMap<BString, bool>,
    computed: IndexMap<BString, i64>,
    usage_list: &'a IndexMap<BString, Vec<BString>>,
    weights: &'a IndexMap<BString, i64>,
}

impl ComputeImportanceHelper<'_> {
    fn compute(&mut self, name: &BStr) -> i64 {
        if let Some(&w) = self.computed.get(name) {
            return w;
        }
        if self.computing.contains_key(name) {
            return 0;
        }
        self.computing.insert(BString::from(name), true);
        let mut weight = *self.weights.get(name).unwrap_or(&0);
        if let Some(users) = self.usage_list.get(name) {
            let users = users.clone();
            for user in &users {
                weight -= 1 - self.compute(user.as_ref());
            }
        }
        self.computing.shift_remove(name);
        self.computed.insert(BString::from(name), weight);
        weight
    }
}
