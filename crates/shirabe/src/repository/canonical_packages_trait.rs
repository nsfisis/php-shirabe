//! ref: composer/src/Composer/Repository/CanonicalPackagesTrait.php

use crate::package::package_interface::PackageInterface;
use indexmap::IndexMap;

/// Provides get_canonical_packages() to various repository implementations.
pub trait CanonicalPackagesTrait {
    fn get_packages(&self) -> Vec<Box<dyn PackageInterface>>;

    /// Get unique packages (at most one package of each name), with aliases resolved and removed.
    fn get_canonical_packages(&self) -> Vec<Box<dyn PackageInterface>> {
        let packages = self.get_packages();

        // get at most one package of each name, preferring non-aliased ones
        let mut packages_by_name: IndexMap<String, Box<dyn PackageInterface>> = IndexMap::new();
        for package in packages {
            let name = package.get_name().to_string();
            let prefer_replace = packages_by_name
                .get(&name)
                .map(|existing| existing.as_alias_package().is_some())
                .unwrap_or(true);
            if prefer_replace {
                packages_by_name.insert(name, package);
            }
        }

        let mut canonical_packages = Vec::new();

        // unfold aliased packages
        for package in packages_by_name.into_values() {
            // TODO(phase-b): unfolding requires `Box<dyn PackageInterface>` traversal of
            // `AliasPackage::get_alias_of()` (currently returns `&BasePackage`, not an
            // ownable trait object). Push the alias as-is for now.
            canonical_packages.push(package);
        }

        canonical_packages
    }
}
