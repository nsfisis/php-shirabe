//! ref: composer/src/Composer/DependencyResolver/Pool.php

use crate::advisory::AnySecurityAdvisory;
use crate::package::BasePackageHandle;
use crate::package::version::VersionParser;
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_shim::{CmpOp, STR_PAD_LEFT, bconcat, legacy_lossy, str_pad};
use shirabe_semver::CompilingMatcher;
use shirabe_semver::constraint::{AnyConstraint, SimpleConstraint};
use std::fmt;

/// A package pool contains all packages for dependency resolution
#[derive(Debug)]
pub struct Pool {
    packages: Vec<BasePackageHandle>,
    package_by_name: IndexMap<BString, Vec<BasePackageHandle>>,
    version_parser: VersionParser,
    provider_cache: IndexMap<BString, IndexMap<BString, Vec<BasePackageHandle>>>,
    unacceptable_fixed_or_locked_packages: Vec<BasePackageHandle>,
    removed_versions: IndexMap<BString, IndexMap<BString, BString>>,
    removed_versions_by_package: IndexMap<BString, IndexMap<BString, BString>>,
    security_removed_versions:
        IndexMap<BString, IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>>>,
    abandoned_removed_versions: IndexMap<BString, IndexMap<BString, BString>>,
}

impl Pool {
    pub fn new(
        packages: Vec<BasePackageHandle>,
        unacceptable_fixed_or_locked_packages: Vec<BasePackageHandle>,
        removed_versions: IndexMap<BString, IndexMap<BString, BString>>,
        removed_versions_by_package: IndexMap<BString, IndexMap<BString, BString>>,
        security_removed_versions: IndexMap<
            BString,
            IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>>,
        >,
        abandoned_removed_versions: IndexMap<BString, IndexMap<BString, BString>>,
    ) -> Self {
        let mut this = Self {
            packages: vec![],
            package_by_name: IndexMap::new(),
            version_parser: VersionParser::new(),
            provider_cache: IndexMap::new(),
            unacceptable_fixed_or_locked_packages,
            removed_versions,
            removed_versions_by_package,
            security_removed_versions,
            abandoned_removed_versions,
        };
        this.set_packages(packages);
        this
    }

    pub fn get_removed_versions(
        &self,
        name: &BStr,
        constraint: &AnyConstraint,
    ) -> IndexMap<BString, BString> {
        let Some(versions) = self.removed_versions.get(name) else {
            return IndexMap::new();
        };

        let mut result: IndexMap<BString, BString> = IndexMap::new();
        for (version, pretty_version) in versions {
            if constraint.matches(
                &SimpleConstraint::new("==", legacy_lossy(version).into_owned(), None).into(),
            ) {
                result.insert(version.clone(), pretty_version.clone());
            }
        }

        result
    }

    pub fn get_all_removed_versions(&self) -> &IndexMap<BString, IndexMap<BString, BString>> {
        &self.removed_versions
    }

    pub fn get_removed_versions_by_package(&self, object_hash: &str) -> IndexMap<BString, BString> {
        let Some(versions) = self.removed_versions_by_package.get(bstr::B(object_hash)) else {
            return IndexMap::new();
        };

        versions.clone()
    }

    pub fn get_all_removed_versions_by_package(
        &self,
    ) -> &IndexMap<BString, IndexMap<BString, BString>> {
        &self.removed_versions_by_package
    }

    pub fn is_security_removed_package_version(
        &self,
        package_name: &BStr,
        constraint: Option<&AnyConstraint>,
    ) -> bool {
        let empty = IndexMap::new();
        let versions = self
            .security_removed_versions
            .get(package_name)
            .unwrap_or(&empty);
        for (version, _package_with_security_advisories) in versions {
            if let Some(c) = constraint
                && c.matches(
                    &SimpleConstraint::new("==", legacy_lossy(version).into_owned(), None).into(),
                )
            {
                return true;
            }
        }

        false
    }

    pub fn get_security_advisory_identifiers_for_package_version(
        &self,
        package_name: &BStr,
        constraint: Option<&AnyConstraint>,
    ) -> Vec<BString> {
        let empty = IndexMap::new();
        let versions = self
            .security_removed_versions
            .get(package_name)
            .unwrap_or(&empty);
        for (version, package_with_security_advisories) in versions {
            if let Some(c) = constraint
                && c.matches(
                    &SimpleConstraint::new("==", legacy_lossy(version).into_owned(), None).into(),
                )
            {
                return package_with_security_advisories
                    .iter()
                    .map(|advisory| BString::from(advisory.advisory_id()))
                    .collect();
            }
        }

        vec![]
    }

    pub fn is_abandoned_removed_package_version(
        &self,
        package_name: &BStr,
        constraint: Option<&AnyConstraint>,
    ) -> bool {
        let empty = IndexMap::new();
        let versions = self
            .abandoned_removed_versions
            .get(package_name)
            .unwrap_or(&empty);
        for (version, _pretty_version) in versions {
            if let Some(c) = constraint
                && c.matches(
                    &SimpleConstraint::new("==", legacy_lossy(version).into_owned(), None).into(),
                )
            {
                return true;
            }
        }

        false
    }

    pub fn get_all_security_removed_package_versions(
        &self,
    ) -> &IndexMap<BString, IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>>> {
        &self.security_removed_versions
    }

    pub fn get_all_abandoned_removed_package_versions(
        &self,
    ) -> &IndexMap<BString, IndexMap<BString, BString>> {
        &self.abandoned_removed_versions
    }

    fn set_packages(&mut self, packages: Vec<BasePackageHandle>) {
        let mut id: i64 = 1;

        for package in packages {
            package.set_id(id);
            id += 1;

            for provided in package.get_names(true) {
                self.package_by_name
                    .entry(BString::from(&provided[..]))
                    .or_default()
                    .push(package.clone());
            }

            self.packages.push(package);
        }
    }

    pub fn get_packages(&self) -> &Vec<BasePackageHandle> {
        &self.packages
    }

    /// Retrieves the package object for a given package id.
    pub fn package_by_id(&self, id: i64) -> BasePackageHandle {
        self.packages[(id - 1) as usize].clone()
    }

    /// Searches all packages providing the given package name and match the constraint
    pub fn what_provides(
        &mut self,
        name: &BStr,
        constraint: Option<&AnyConstraint>,
    ) -> Vec<BasePackageHandle> {
        // PHP: $key = (string) $constraint;
        let key = match constraint {
            Some(c) => BString::from(&c.to_php_string()[..]),
            None => BString::default(),
        };
        if let Some(by_key) = self.provider_cache.get(name)
            && let Some(cached) = by_key.get(&key)
        {
            return cached.clone();
        }

        let computed = self.compute_what_provides(name, constraint);
        self.provider_cache
            .entry(bstr::BString::from(name))
            .or_default()
            .insert(key, computed.clone());
        computed
    }

    pub(crate) fn compute_what_provides(
        &self,
        name: &BStr,
        constraint: Option<&AnyConstraint>,
    ) -> Vec<BasePackageHandle> {
        let Some(candidates) = self.package_by_name.get(name) else {
            return vec![];
        };

        let mut matches: Vec<BasePackageHandle> = vec![];

        for candidate in candidates {
            if self.r#match(candidate.clone(), name, constraint) {
                matches.push(candidate.clone());
            }
        }

        matches
    }

    pub fn literal_to_package(&self, literal: i64) -> BasePackageHandle {
        let package_id = literal.abs();

        self.package_by_id(package_id)
    }

    pub fn literal_to_pretty_string(
        &self,
        literal: i64,
        installed_map: &IndexMap<BString, BasePackageHandle>,
    ) -> BString {
        let package = self.literal_to_package(literal);

        let prefix = if installed_map.contains_key(bstr::B(&package.id().to_string())) {
            if literal > 0 { "keep" } else { "remove" }
        } else {
            if literal > 0 {
                "install"
            } else {
                "don't install"
            }
        };

        bconcat!(prefix, " ", package.get_pretty_string())
    }

    /// Checks if the package matches the given constraint directly or through
    /// provided or replaced packages
    pub fn r#match(
        &self,
        candidate: BasePackageHandle,
        name: &BStr,
        constraint: Option<&AnyConstraint>,
    ) -> bool {
        let candidate_name = candidate.get_name();
        let candidate_version = candidate.get_version();

        if candidate_name == name {
            return constraint.is_none()
                || CompilingMatcher::r#match(constraint.unwrap(), CmpOp::Eq, &candidate_version);
        }

        let provides = candidate.get_provides();
        let replaces = candidate.get_replaces();

        // aliases create multiple replaces/provides for one target so they can not use the shortcut below
        if replaces.contains_key(b"0".as_slice()) || provides.contains_key(b"0".as_slice()) {
            for link in provides.values() {
                if link.get_target() == name
                    && (constraint.is_none() || constraint.unwrap().matches(link.get_constraint()))
                {
                    return true;
                }
            }

            for link in replaces.values() {
                if link.get_target() == name
                    && (constraint.is_none() || constraint.unwrap().matches(link.get_constraint()))
                {
                    return true;
                }
            }

            return false;
        }

        if let Some(provide) = provides.get(name)
            && (constraint.is_none() || constraint.unwrap().matches(provide.get_constraint()))
        {
            return true;
        }

        if let Some(replace) = replaces.get(name)
            && (constraint.is_none() || constraint.unwrap().matches(replace.get_constraint()))
        {
            return true;
        }

        false
    }

    pub fn is_unacceptable_fixed_or_locked_package(&self, package: BasePackageHandle) -> bool {
        // PHP: \in_array($package, $this->unacceptableFixedOrLockedPackages, true)
        // strict comparison checks reference identity for objects
        self.unacceptable_fixed_or_locked_packages
            .iter()
            .any(|p| p.ptr_eq(&package))
    }

    pub fn get_unacceptable_fixed_or_locked_packages(&self) -> &Vec<BasePackageHandle> {
        &self.unacceptable_fixed_or_locked_packages
    }

    /// For testing only: mirrors PHP `Pool implements \Countable` (`count($pool)`),
    /// returning how many packages have been loaded into the pool.
    pub fn __count(&self) -> i64 {
        self.packages.len() as i64
    }
}

impl fmt::Display for Pool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut str = String::from("Pool:\n");

        for package in &self.packages {
            str.push_str(&format!(
                "- {}: {}\n",
                legacy_lossy(&str_pad(package.id().to_string(), 6, " ", STR_PAD_LEFT)),
                legacy_lossy(&package.get_name())
            ));
        }

        write!(f, "{}", str)
    }
}
