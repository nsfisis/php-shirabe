//! ref: composer/tests/Composer/Test/InstalledVersionsTest.php
//!
//! Every test here needs the lookup APIs of `InstalledVersions`, which the Rust port does not
//! have. The PHP setUp reflects into `ClassLoader::registeredLoaders` to make it seem like no
//! class loaders are registered, then loads the installed_relative.php fixture via `require`.

#[test]
#[ignore = "InstalledVersions::getInstalledPackages has no Rust counterpart"]
fn test_get_installed_packages() {
    // TODO(port): needs InstalledVersions::get_installed_packages.
    todo!()
}

#[test]
#[ignore = "InstalledVersions::isInstalled has no Rust counterpart"]
fn test_is_installed() {
    // TODO(port): needs InstalledVersions::is_installed.
    todo!()
}

#[test]
#[ignore = "InstalledVersions::satisfies has no Rust counterpart"]
fn test_satisfies() {
    // TODO(port): needs InstalledVersions::satisfies.
    todo!()
}

#[test]
#[ignore = "InstalledVersions::getVersionRanges has no Rust counterpart"]
fn test_get_version_ranges() {
    // TODO(port): needs InstalledVersions::get_version_ranges.
    todo!()
}

#[test]
#[ignore = "InstalledVersions::getVersion has no Rust counterpart"]
fn test_get_version() {
    // TODO(port): needs InstalledVersions::get_version.
    todo!()
}

#[test]
#[ignore = "InstalledVersions::getPrettyVersion has no Rust counterpart"]
fn test_get_pretty_version() {
    // TODO(port): needs InstalledVersions::get_pretty_version.
    todo!()
}

#[test]
#[ignore = "InstalledVersions::getVersion has no Rust counterpart"]
fn test_get_version_out_of_bounds() {
    // TODO(port): needs InstalledVersions::get_version.
    todo!()
}

#[test]
#[ignore = "InstalledVersions::getRootPackage has no Rust counterpart"]
fn test_get_root_package() {
    // TODO(port): needs InstalledVersions::get_root_package.
    todo!()
}

#[test]
#[ignore = "InstalledVersions::getRawData has no Rust counterpart"]
fn test_get_raw_data() {
    // TODO(port): needs InstalledVersions::get_raw_data.
    todo!()
}

#[test]
#[ignore = "InstalledVersions::getReference has no Rust counterpart"]
fn test_get_reference() {
    // TODO(port): needs InstalledVersions::get_reference.
    todo!()
}

#[test]
#[ignore = "InstalledVersions::getInstalledPackagesByType has no Rust counterpart"]
fn test_get_installed_packages_by_type() {
    // TODO(port): needs InstalledVersions::get_installed_packages_by_type.
    todo!()
}

#[test]
#[ignore = "InstalledVersions::getInstallPath has no Rust counterpart"]
fn test_get_install_path() {
    // TODO(port): needs InstalledVersions::get_install_path.
    todo!()
}

#[test]
#[ignore = "InstalledVersions::isInstalled and getRootPackage have no Rust counterpart"]
fn test_with_class_loader_loaded() {
    // TODO(port): needs InstalledVersions::is_installed and
    // InstalledVersions::get_root_package.
    todo!()
}
