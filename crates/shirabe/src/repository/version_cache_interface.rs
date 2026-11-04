//! ref: composer/src/Composer/Repository/VersionCacheInterface.php sha1:53915c4de637de35eca5599eb19967bc6f01b66d

use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

/// Result of looking up a cached package version.
///
/// PHP's `getVersionPackage(...)` returns either an array (the package data),
/// `null` (cache miss), or `false` (cached absence). We model that as an enum.
#[derive(Debug)]
pub enum VersionCacheResult {
    /// Cache miss (PHP `null`).
    None,
    /// Cached absence (PHP `false`).
    Missing,
    /// Cached package data (PHP `array`).
    Package(IndexMap<BString, PhpMixed>),
}

pub trait VersionCacheInterface: std::fmt::Debug {
    fn get_version_package(&self, version: &BStr, identifier: &BStr) -> VersionCacheResult;
}
