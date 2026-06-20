//! ref: composer/tests/Composer/Test/InstalledVersionsTest.php

// setUpBeforeClass reflects into ClassLoader::registeredLoaders and the cases load
// installed.php fixtures via InstalledVersions::reload; the reflection and fixture loading
// are not ported.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "needs reflection into ClassLoader::registeredLoaders and installed.php fixtures (not ported)"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_get_installed_packages);
stub!(test_is_installed);
stub!(test_satisfies);
stub!(test_get_version_ranges);
stub!(test_get_version);
stub!(test_get_pretty_version);
stub!(test_get_version_out_of_bounds);
stub!(test_get_root_package);
stub!(test_get_raw_data);
stub!(test_get_reference);
stub!(test_get_installed_packages_by_type);
stub!(test_get_install_path);
stub!(test_with_class_loader_loaded);
