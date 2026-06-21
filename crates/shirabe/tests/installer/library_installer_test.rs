//! ref: composer/tests/Composer/Test/Installer/LibraryInstallerTest.php

/// Sets up a Composer/Config over root/vendor/bin temp dirs plus mocked
/// DownloadManager/repository/IO. The temp-dir helpers and the mocks are not
/// available here, so this remains a stub.
fn set_up() {
    todo!()
}

/// Removes the root dir created by `set_up`, which is itself a stub.
fn tear_down() {
    todo!()
}

struct TearDown;

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down();
    }
}

// These construct a LibraryInstaller over a temp dir with a mocked IO/Filesystem/repository
// and mocked packages to drive install/update/uninstall and path resolution.
#[test]
#[ignore = "mocks IO/Filesystem/repository and packages to drive LibraryInstaller; mocking is not available"]
fn test_installer_creation_should_not_create_vendor_directory() {
    todo!()
}

#[test]
#[ignore = "mocks IO/Filesystem/repository and packages to drive LibraryInstaller; mocking is not available"]
fn test_installer_creation_should_not_create_bin_directory() {
    todo!()
}

#[test]
#[ignore = "mocks IO/Filesystem/repository and packages to drive LibraryInstaller; mocking is not available"]
fn test_is_installed() {
    todo!()
}

#[test]
#[ignore = "mocks IO/Filesystem/repository and packages to drive LibraryInstaller; mocking is not available"]
fn test_install() {
    todo!()
}

#[test]
#[ignore = "mocks IO/Filesystem/repository and packages to drive LibraryInstaller; mocking is not available"]
fn test_update() {
    todo!()
}

#[test]
#[ignore = "mocks IO/Filesystem/repository and packages to drive LibraryInstaller; mocking is not available"]
fn test_uninstall() {
    todo!()
}

#[test]
#[ignore = "mocks IO/Filesystem/repository and packages to drive LibraryInstaller; mocking is not available"]
fn test_get_install_path_without_target_dir() {
    todo!()
}

#[test]
#[ignore = "mocks IO/Filesystem/repository and packages to drive LibraryInstaller; mocking is not available"]
fn test_get_install_path_with_target_dir() {
    todo!()
}

#[test]
#[ignore = "mocks IO/Filesystem/repository and packages to drive LibraryInstaller; mocking is not available"]
fn test_ensure_binaries_installed() {
    todo!()
}
