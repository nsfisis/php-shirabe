//! ref: composer/src/Composer/Util/PackageSorter.php

use std::any::Any;

use indexmap::IndexMap;
use shirabe_php_shim::{strnatcasecmp, version_compare};

use crate::package::link::Link;
use crate::package::package_interface::PackageInterface;
use crate::package::root_package::RootPackage;

pub struct PackageSorter;

impl PackageSorter {
    pub fn get_most_current_version(packages: Vec<Box<dyn PackageInterface>>) -> Option<Box<dyn PackageInterface>> {
        if packages.is_empty() {
            return None;
        }

        let mut iter = packages.into_iter();
        let mut highest = iter.next().unwrap();
        for candidate in iter {
            if candidate.is_default_branch() {
                return Some(candidate);
            }
            if version_compare(highest.get_version(), candidate.get_version(), "<") {
                highest = candidate;
            }
        }

        Some(highest)
    }

    pub fn sort_packages_alphabetically(mut packages: Vec<Box<dyn PackageInterface>>) -> Vec<Box<dyn PackageInterface>> {
        packages.sort_by_key(|p| p.get_name());
        packages
    }

    pub fn sort_packages(packages: Vec<Box<dyn PackageInterface>>, weights: IndexMap<String, i64>) -> Vec<Box<dyn PackageInterface>> {
        let mut usage_list: IndexMap<String, Vec<String>> = IndexMap::new();

        for package in &packages {
            let mut links: IndexMap<String, Link> = package.get_requires();
            // TODO: check for RootAliasPackage as well
            if let Some(root_package) = (package.as_any() as &dyn Any).downcast_ref::<RootPackage>() {
                links.extend(root_package.get_dev_requires());
            }
            for link in links.values() {
                let target = link.get_target().to_string();
                usage_list.entry(target).or_default().push(package.get_name().to_string());
            }
        }

        let mut helper = ComputeImportanceHelper {
            computing: IndexMap::new(),
            computed: IndexMap::new(),
            usage_list: &usage_list,
            weights: &weights,
        };

        let mut weighted_packages: Vec<(String, i64, usize)> = Vec::new();
        for (index, package) in packages.iter().enumerate() {
            let name = package.get_name().to_string();
            let weight = helper.compute(&name);
            weighted_packages.push((name, weight, index));
        }

        weighted_packages.sort_by(|a, b| {
            if a.1 != b.1 {
                a.1.cmp(&b.1)
            } else {
                strnatcasecmp(&a.0, &b.0).cmp(&0)
            }
        });

        let mut packages: Vec<Option<Box<dyn PackageInterface>>> = packages.into_iter().map(Some).collect();
        weighted_packages
            .into_iter()
            .map(|(_, _, index)| packages[index].take().unwrap())
            .collect()
    }
}

struct ComputeImportanceHelper<'a> {
    computing: IndexMap<String, bool>,
    computed: IndexMap<String, i64>,
    usage_list: &'a IndexMap<String, Vec<String>>,
    weights: &'a IndexMap<String, i64>,
}

impl ComputeImportanceHelper<'_> {
    fn compute(&mut self, name: &str) -> i64 {
        if let Some(&w) = self.computed.get(name) {
            return w;
        }
        if self.computing.contains_key(name) {
            return 0;
        }
        self.computing.insert(name.to_string(), true);
        let mut weight = *self.weights.get(name).unwrap_or(&0);
        if let Some(users) = self.usage_list.get(name) {
            let users = users.clone();
            for user in &users {
                weight -= 1 - self.compute(user);
            }
        }
        self.computing.remove(name);
        self.computed.insert(name.to_string(), weight);
        weight
    }
}
