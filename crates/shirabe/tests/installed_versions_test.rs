//! ref: composer/tests/Composer/Test/InstalledVersionsTest.php

// setUpBeforeClass reflects into ClassLoader::registeredLoaders and the cases load
// installed.php fixtures via InstalledVersions::reload; the reflection and fixture loading
// are not ported.

use tempfile::TempDir;

fn set_up() -> TempDir {
    let root = TempDir::new().unwrap();

    // Loading the installed_relative.php fixture and InstalledVersions::reload are not ported.
    todo!();

    #[allow(unreachable_code)]
    root
}

#[test]
#[ignore = "needs reflection into ClassLoader::registeredLoaders and installed.php fixtures (not ported)"]
fn test_get_installed_packages() {
    let _root = set_up();
    todo!()
}

#[test]
#[ignore = "needs reflection into ClassLoader::registeredLoaders and installed.php fixtures (not ported)"]
fn test_is_installed() {
    let _root = set_up();
    todo!()
}

#[test]
#[ignore = "needs reflection into ClassLoader::registeredLoaders and installed.php fixtures (not ported)"]
fn test_satisfies() {
    let _root = set_up();
    todo!()
}

#[test]
#[ignore = "needs reflection into ClassLoader::registeredLoaders and installed.php fixtures (not ported)"]
fn test_get_version_ranges() {
    let _root = set_up();
    todo!()
}

#[test]
#[ignore = "needs reflection into ClassLoader::registeredLoaders and installed.php fixtures (not ported)"]
fn test_get_version() {
    let _root = set_up();
    todo!()
}

#[test]
#[ignore = "needs reflection into ClassLoader::registeredLoaders and installed.php fixtures (not ported)"]
fn test_get_pretty_version() {
    let _root = set_up();
    todo!()
}

#[test]
#[ignore = "needs reflection into ClassLoader::registeredLoaders and installed.php fixtures (not ported)"]
fn test_get_version_out_of_bounds() {
    let _root = set_up();
    todo!()
}

#[test]
#[ignore = "needs reflection into ClassLoader::registeredLoaders and installed.php fixtures (not ported)"]
fn test_get_root_package() {
    let _root = set_up();
    todo!()
}

#[test]
#[ignore = "needs reflection into ClassLoader::registeredLoaders and installed.php fixtures (not ported)"]
fn test_get_raw_data() {
    let _root = set_up();
    todo!()
}

#[test]
#[ignore = "needs reflection into ClassLoader::registeredLoaders and installed.php fixtures (not ported)"]
fn test_get_reference() {
    let _root = set_up();
    todo!()
}

#[test]
#[ignore = "needs reflection into ClassLoader::registeredLoaders and installed.php fixtures (not ported)"]
fn test_get_installed_packages_by_type() {
    let _root = set_up();
    todo!()
}

#[test]
#[ignore = "needs reflection into ClassLoader::registeredLoaders and installed.php fixtures (not ported)"]
fn test_get_install_path() {
    let _root = set_up();
    todo!()
}

#[test]
#[ignore = "needs reflection into ClassLoader::registeredLoaders and installed.php fixtures (not ported)"]
fn test_with_class_loader_loaded() {
    let _root = set_up();
    todo!()
}
