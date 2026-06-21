//! ref: composer/tests/Composer/Test/Installer/InstallationManagerTest.php

/// Builds mocked Loop/repository/IO. The mocks are not available here, so this
/// remains a stub.
fn set_up() {
    todo!()
}

// These mock individual installers, the repository and IO to drive InstallationManager's
// add/execute/install/update/uninstall logic; mocking is not available here.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "mocks installers/repository/IO to drive InstallationManager; mocking is not available"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_add_get_installer);
stub!(test_add_remove_installer);
stub!(test_execute);
stub!(test_install);
stub!(test_update_with_equal_types);
stub!(test_update_with_not_equal_types);
stub!(test_uninstall);
stub!(test_install_binary);
