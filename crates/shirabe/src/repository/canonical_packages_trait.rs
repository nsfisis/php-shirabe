//! ref: composer/src/Composer/Repository/CanonicalPackagesTrait.php

use crate::package::PackageInterfaceHandle;
use crate::repository::RepositoryInterface;
use indexmap::IndexMap;

/// Provides get_canonical_packages() to various repository implementations.
pub trait CanonicalPackagesTrait: RepositoryInterface {
    /// Get unique packages (at most one package of each name), with aliases resolved and removed.
    fn get_canonical_packages(&mut self) -> anyhow::Result<Vec<PackageInterfaceHandle>> {
        let packages = self.get_packages()?;

        // get at most one package of each name, preferring non-aliased ones
        let mut packages_by_name: IndexMap<String, PackageInterfaceHandle> = IndexMap::new();
        for package in packages {
            let name = package.get_name();
            let prefer_replace = packages_by_name
                .get(&name)
                .map(|existing| existing.as_alias().is_some())
                .unwrap_or(true);
            if prefer_replace {
                packages_by_name.insert(name, package);
            }
        }

        let mut canonical_packages = Vec::new();

        // unfold aliased packages
        for mut package in packages_by_name.into_values() {
            while let Some(alias) = package.as_alias() {
                package = alias.get_alias_of().into();
            }
            canonical_packages.push(package);
        }

        Ok(canonical_packages)
    }
}
