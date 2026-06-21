//! ref: composer/tests/Composer/Test/Installer/InstallationManagerTest.php

/// Builds mocked Loop/repository/IO. The mocks are not available here, so this
/// remains a stub.
fn set_up() {
    todo!()
}

// These mock individual installers, the repository and IO to drive InstallationManager's
// add/execute/install/update/uninstall logic; mocking is not available here.
#[ignore = "requires PHPUnit mocks of InstallerInterface/IOInterface/Loop with expects() call-count assertions"]
#[test]
fn test_add_get_installer() {
    todo!()
}

#[ignore = "requires PHPUnit mocks of InstallerInterface/IOInterface/Loop with expects() call-count assertions"]
#[test]
fn test_add_remove_installer() {
    todo!()
}

#[ignore = "requires partial PHPUnit mock of InstallationManager (onlyMethods install/update/uninstall) and PackageInterface mock"]
#[test]
fn test_execute() {
    todo!()
}

#[ignore = "requires PHPUnit mocks of InstallerInterface/PackageInterface with expects() call-count assertions"]
#[test]
fn test_install() {
    todo!()
}

#[ignore = "requires PHPUnit mocks of InstallerInterface/PackageInterface with expects() call-count assertions"]
#[test]
fn test_update_with_equal_types() {
    todo!()
}

#[ignore = "requires PHPUnit mocks of InstallerInterface/PackageInterface with expects() call-count assertions"]
#[test]
fn test_update_with_not_equal_types() {
    todo!()
}

#[ignore = "requires PHPUnit mocks of InstallerInterface/PackageInterface with expects() call-count assertions"]
#[test]
fn test_uninstall() {
    todo!()
}

#[ignore = "requires PHPUnit mock of LibraryInstaller and PackageInterface with expects() call-count assertions"]
#[test]
fn test_install_binary() {
    todo!()
}
