//! ref: composer/src/Composer/DependencyResolver/Pool.php

use std::fmt;

use indexmap::IndexMap;
use shirabe_php_shim::{abs, spl_object_hash, str_pad, Countable, STR_PAD_LEFT};
use shirabe_semver::compiling_matcher::CompilingMatcher;
use shirabe_semver::constraint::constraint::Constraint;
use shirabe_semver::constraint::constraint_interface::ConstraintInterface;

use crate::advisory::partial_security_advisory::PartialSecurityAdvisory;
use crate::package::base_package::BasePackage;
use crate::package::version::version_parser::VersionParser;

/// A package pool contains all packages for dependency resolution
#[derive(Debug)]
pub struct Pool {
    /// @var BasePackage[]
    pub(crate) packages: Vec<Box<BasePackage>>,
    /// @var array<string, BasePackage[]>
    pub(crate) package_by_name: IndexMap<String, Vec<Box<BasePackage>>>,
    /// @var VersionParser
    pub(crate) version_parser: VersionParser,
    /// @var array<string, array<string, BasePackage[]>>
    pub(crate) provider_cache: IndexMap<String, IndexMap<String, Vec<Box<BasePackage>>>>,
    /// @var BasePackage[]
    pub(crate) unacceptable_fixed_or_locked_packages: Vec<Box<BasePackage>>,
    /// @var array<string, array<string, string>> Map of package name => normalized version => pretty version
    pub(crate) removed_versions: IndexMap<String, IndexMap<String, String>>,
    /// @var array<string, array<string, string>> Map of package object hash => removed normalized versions => removed pretty version
    pub(crate) removed_versions_by_package: IndexMap<String, IndexMap<String, String>>,
    /// @var array<string, array<string, array<SecurityAdvisory|PartialSecurityAdvisory>>> Map of package name => normalized version => security advisories
    // TODO(phase-b): SecurityAdvisory|PartialSecurityAdvisory union — stored as PartialSecurityAdvisory base
    security_removed_versions: IndexMap<String, IndexMap<String, Vec<PartialSecurityAdvisory>>>,
    /// @var array<string, array<string, string>> Map of package name => normalized version => pretty version
    abandoned_removed_versions: IndexMap<String, IndexMap<String, String>>,
}

impl Pool {
    /// @param BasePackage[] $packages
    /// @param BasePackage[] $unacceptableFixedOrLockedPackages
    /// @param array<string, array<string, string>> $removedVersions
    /// @param array<string, array<string, string>> $removedVersionsByPackage
    /// @param array<string, array<string, array<SecurityAdvisory|PartialSecurityAdvisory>>> $securityRemovedVersions
    /// @param array<string, array<string, string>> $abandonedRemovedVersions
    pub fn new(
        packages: Vec<Box<BasePackage>>,
        unacceptable_fixed_or_locked_packages: Vec<Box<BasePackage>>,
        removed_versions: IndexMap<String, IndexMap<String, String>>,
        removed_versions_by_package: IndexMap<String, IndexMap<String, String>>,
        security_removed_versions: IndexMap<
            String,
            IndexMap<String, Vec<PartialSecurityAdvisory>>,
        >,
        abandoned_removed_versions: IndexMap<String, IndexMap<String, String>>,
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

    /// @return array<string, string>
    pub fn get_removed_versions(
        &self,
        name: &str,
        constraint: &dyn ConstraintInterface,
    ) -> IndexMap<String, String> {
        let Some(versions) = self.removed_versions.get(name) else {
            return IndexMap::new();
        };

        let mut result: IndexMap<String, String> = IndexMap::new();
        for (version, pretty_version) in versions {
            if constraint.matches(&Constraint::new("==", version)) {
                result.insert(version.clone(), pretty_version.clone());
            }
        }

        result
    }

    /// @return array<string, array<string, string>>
    pub fn get_all_removed_versions(&self) -> &IndexMap<String, IndexMap<String, String>> {
        &self.removed_versions
    }

    /// @return array<string, string>
    pub fn get_removed_versions_by_package(&self, object_hash: &str) -> IndexMap<String, String> {
        let Some(versions) = self.removed_versions_by_package.get(object_hash) else {
            return IndexMap::new();
        };

        versions.clone()
    }

    /// @return array<string, array<string, string>>
    pub fn get_all_removed_versions_by_package(
        &self,
    ) -> &IndexMap<String, IndexMap<String, String>> {
        &self.removed_versions_by_package
    }

    pub fn is_security_removed_package_version(
        &self,
        package_name: &str,
        constraint: Option<&dyn ConstraintInterface>,
    ) -> bool {
        let empty = IndexMap::new();
        let versions = self.security_removed_versions.get(package_name).unwrap_or(&empty);
        for (version, _package_with_security_advisories) in versions {
            if let Some(c) = constraint {
                if c.matches(&Constraint::new("==", version)) {
                    return true;
                }
            }
        }

        false
    }

    /// @return string[]
    pub fn get_security_advisory_identifiers_for_package_version(
        &self,
        package_name: &str,
        constraint: Option<&dyn ConstraintInterface>,
    ) -> Vec<String> {
        let empty = IndexMap::new();
        let versions = self.security_removed_versions.get(package_name).unwrap_or(&empty);
        for (version, package_with_security_advisories) in versions {
            if let Some(c) = constraint {
                if c.matches(&Constraint::new("==", version)) {
                    return package_with_security_advisories
                        .iter()
                        .map(|advisory| advisory.advisory_id.clone())
                        .collect();
                }
            }
        }

        vec![]
    }

    pub fn is_abandoned_removed_package_version(
        &self,
        package_name: &str,
        constraint: Option<&dyn ConstraintInterface>,
    ) -> bool {
        let empty = IndexMap::new();
        let versions = self.abandoned_removed_versions.get(package_name).unwrap_or(&empty);
        for (version, _pretty_version) in versions {
            if let Some(c) = constraint {
                if c.matches(&Constraint::new("==", version)) {
                    return true;
                }
            }
        }

        false
    }

    /// @return array<string, array<string, array<SecurityAdvisory|PartialSecurityAdvisory>>>
    pub fn get_all_security_removed_package_versions(
        &self,
    ) -> &IndexMap<String, IndexMap<String, Vec<PartialSecurityAdvisory>>> {
        &self.security_removed_versions
    }

    /// @return array<string, array<string, string>>
    pub fn get_all_abandoned_removed_package_versions(
        &self,
    ) -> &IndexMap<String, IndexMap<String, String>> {
        &self.abandoned_removed_versions
    }

    /// @param BasePackage[] $packages
    fn set_packages(&mut self, packages: Vec<Box<BasePackage>>) {
        let mut id: i64 = 1;

        for mut package in packages {
            package.id = id;
            id += 1;

            for provided in package.get_names(true) {
                self.package_by_name
                    .entry(provided)
                    .or_insert_with(Vec::new)
                    .push(package.clone_box());
            }

            self.packages.push(package);
        }
    }

    /// @return BasePackage[]
    pub fn get_packages(&self) -> &Vec<Box<BasePackage>> {
        &self.packages
    }

    /// Retrieves the package object for a given package id.
    pub fn package_by_id(&self, id: i64) -> &BasePackage {
        &self.packages[(id - 1) as usize]
    }

    /// Searches all packages providing the given package name and match the constraint
    ///
    /// @param string $name The package name to be searched for
    /// @param ?ConstraintInterface $constraint A constraint that all returned
    ///                                         packages must match or null to return all
    /// @return BasePackage[] A set of packages
    pub fn what_provides(
        &mut self,
        name: &str,
        constraint: Option<&dyn ConstraintInterface>,
    ) -> Vec<Box<BasePackage>> {
        // PHP: $key = (string) $constraint;
        let key = match constraint {
            Some(c) => c.to_string(),
            None => String::new(),
        };
        if let Some(by_key) = self.provider_cache.get(name) {
            if let Some(cached) = by_key.get(&key) {
                return cached.iter().map(|p| p.clone_box()).collect();
            }
        }

        let computed = self.compute_what_provides(name, constraint);
        self.provider_cache
            .entry(name.to_string())
            .or_insert_with(IndexMap::new)
            .insert(key, computed.iter().map(|p| p.clone_box()).collect());
        computed
    }

    /// @param  string               $name       The package name to be searched for
    /// @param  ?ConstraintInterface $constraint A constraint that all returned
    ///                                          packages must match or null to return all
    /// @return BasePackage[]
    fn compute_what_provides(
        &self,
        name: &str,
        constraint: Option<&dyn ConstraintInterface>,
    ) -> Vec<Box<BasePackage>> {
        let Some(candidates) = self.package_by_name.get(name) else {
            return vec![];
        };

        let mut matches: Vec<Box<BasePackage>> = vec![];

        for candidate in candidates {
            if self.r#match(candidate, name, constraint) {
                matches.push(candidate.clone_box());
            }
        }

        matches
    }

    pub fn literal_to_package(&self, literal: i64) -> &BasePackage {
        let package_id = abs(literal);

        self.package_by_id(package_id)
    }

    /// @param array<int, BasePackage> $installedMap
    pub fn literal_to_pretty_string(
        &self,
        literal: i64,
        installed_map: &IndexMap<i64, Box<BasePackage>>,
    ) -> String {
        let package = self.literal_to_package(literal);

        let prefix = if installed_map.contains_key(&package.id) {
            if literal > 0 {
                "keep"
            } else {
                "remove"
            }
        } else {
            if literal > 0 {
                "install"
            } else {
                "don't install"
            }
        };

        format!("{} {}", prefix, package.get_pretty_string())
    }

    /// Checks if the package matches the given constraint directly or through
    /// provided or replaced packages
    ///
    /// @param  string              $name       Name of the package to be matched
    pub fn r#match(
        &self,
        candidate: &BasePackage,
        name: &str,
        constraint: Option<&dyn ConstraintInterface>,
    ) -> bool {
        let candidate_name = candidate.get_name();
        let candidate_version = candidate.get_version();

        if candidate_name == name {
            return constraint.is_none()
                || CompilingMatcher::r#match(
                    constraint.unwrap(),
                    Constraint::OP_EQ,
                    candidate_version,
                );
        }

        let provides = candidate.get_provides();
        let replaces = candidate.get_replaces();

        // aliases create multiple replaces/provides for one target so they can not use the shortcut below
        // TODO(phase-b): PHP uses isset($replaces[0])/isset($provides[0]) to detect numeric-indexed
        // lists; the Rust IndexMap is string-keyed, so this branch checks the "0" key. Confirm
        // semantics during Phase B review.
        if replaces.contains_key("0") || provides.contains_key("0") {
            for link in provides.values() {
                if link.get_target() == name
                    && (constraint.is_none()
                        || constraint.unwrap().matches(link.get_constraint()))
                {
                    return true;
                }
            }

            for link in replaces.values() {
                if link.get_target() == name
                    && (constraint.is_none()
                        || constraint.unwrap().matches(link.get_constraint()))
                {
                    return true;
                }
            }

            return false;
        }

        if let Some(provide) = provides.get(name) {
            if constraint.is_none() || constraint.unwrap().matches(provide.get_constraint()) {
                return true;
            }
        }

        if let Some(replace) = replaces.get(name) {
            if constraint.is_none() || constraint.unwrap().matches(replace.get_constraint()) {
                return true;
            }
        }

        false
    }

    pub fn is_unacceptable_fixed_or_locked_package(&self, package: &BasePackage) -> bool {
        // PHP: \in_array($package, $this->unacceptableFixedOrLockedPackages, true)
        // strict comparison checks reference identity for objects
        let target_hash = spl_object_hash(package);
        self.unacceptable_fixed_or_locked_packages
            .iter()
            .any(|p| spl_object_hash(p.as_ref()) == target_hash)
    }

    /// @return BasePackage[]
    pub fn get_unacceptable_fixed_or_locked_packages(&self) -> &Vec<Box<BasePackage>> {
        &self.unacceptable_fixed_or_locked_packages
    }
}

impl Countable for Pool {
    /// Returns how many packages have been loaded into the pool
    fn count(&self) -> i64 {
        self.packages.len() as i64
    }
}

impl fmt::Display for Pool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut str = String::from("Pool:\n");

        for package in &self.packages {
            str.push_str(&format!(
                "- {}: {}\n",
                str_pad(&package.id.to_string(), 6, " ", STR_PAD_LEFT),
                package.get_name()
            ));
        }

        write!(f, "{}", str)
    }
}
