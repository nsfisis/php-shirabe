//! ref: composer/tests/Composer/Test/Installer/LibraryInstallerTest.php

use std::cell::RefCell;
use std::fs;
use std::rc::Rc;

use indexmap::IndexMap;
use tempfile::TempDir;

use shirabe::composer::{
    ComposerHandle, PartialComposerHandle, PartialComposerWeakHandle, PartialOrFullComposer,
};
use shirabe::config::Config;
use shirabe::downloader::DownloadManager;
use shirabe::installer::{InstallerInterface, LibraryInstaller};
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::InstalledArrayRepository;
use shirabe::repository::WritableRepositoryInterface;
use shirabe::util::filesystem::Filesystem;
use shirabe_php_shim::PhpMixed;

use crate::test_case::get_package;

/// Mirror of setUp(): builds the Composer/Config over temp root/vendor/bin dirs
/// plus a real DownloadManager and a NullIO. The `_composer_rc` keeps the inner
/// Rc alive for the duration of the test since LibraryInstaller only holds a weak
/// handle.
struct SetUp {
    root: TempDir,
    vendor_dir: String,
    bin_dir: String,
    io: Rc<RefCell<dyn IOInterface>>,
    composer: PartialComposerWeakHandle,
    fs: Filesystem,
    _composer_rc: ComposerHandle,
}

fn set_up() -> SetUp {
    let mut fs = Filesystem::new(None);

    let root = TempDir::new().unwrap();
    let root_dir = fs::canonicalize(root.path())
        .unwrap()
        .to_string_lossy()
        .into_owned();

    let vendor_dir = format!("{}/vendor", root_dir);
    fs::create_dir_all(&vendor_dir).unwrap();

    let bin_dir = format!("{}/bin", root_dir);
    fs::create_dir_all(&bin_dir).unwrap();

    let mut config = Config::new(false, None);
    let mut config_section: IndexMap<String, PhpMixed> = IndexMap::new();
    config_section.insert(
        "vendor-dir".to_string(),
        PhpMixed::String(vendor_dir.clone()),
    );
    config_section.insert("bin-dir".to_string(), PhpMixed::String(bin_dir.clone()));
    let mut merged: IndexMap<String, PhpMixed> = IndexMap::new();
    merged.insert("config".to_string(), PhpMixed::Array(config_section));
    config.merge(&merged, Config::SOURCE_UNKNOWN);
    let config_rc = Rc::new(RefCell::new(config));

    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));

    let dm = DownloadManager::new(io.clone(), false, None);
    let dm_rc = Rc::new(RefCell::new(dm));

    let composer_rc = Rc::new(RefCell::new(PartialOrFullComposer::new_full()));
    let composer = ComposerHandle::from_rc_unchecked(composer_rc.clone());
    composer.borrow_mut().set_config(config_rc);
    composer.borrow_mut().set_download_manager(dm_rc);

    let weak = PartialComposerHandle::from_rc(composer_rc).downgrade();

    SetUp {
        root,
        vendor_dir,
        bin_dir,
        io,
        composer: weak,
        fs,
        _composer_rc: composer,
    }
}

fn tear_down(setup: &mut SetUp) {
    let root = setup.root.path().to_path_buf();
    setup.fs.remove_directory(&root).ok();
}

#[ignore]
#[test]
fn test_installer_creation_should_not_create_vendor_directory() {
    let mut setup = set_up();
    setup.fs.remove_directory(&setup.vendor_dir).unwrap();

    LibraryInstaller::new(setup.io.clone(), setup.composer.clone(), None, None, None);
    assert!(!std::path::Path::new(&setup.vendor_dir).exists());

    tear_down(&mut setup);
}

#[ignore]
#[test]
fn test_installer_creation_should_not_create_bin_directory() {
    let mut setup = set_up();
    setup.fs.remove_directory(&setup.bin_dir).unwrap();

    LibraryInstaller::new(setup.io.clone(), setup.composer.clone(), None, None, None);
    assert!(!std::path::Path::new(&setup.bin_dir).exists());

    tear_down(&mut setup);
}

#[ignore]
#[test]
fn test_is_installed() {
    let mut setup = set_up();
    let mut library =
        LibraryInstaller::new(setup.io.clone(), setup.composer.clone(), None, None, None);
    let package = get_package("test/pkg", "1.0.0");

    let mut repository = InstalledArrayRepository::new().unwrap();
    assert!(!library.is_installed(&repository, package.clone()));

    // package being in repo is not enough to be installed
    repository.add_package(package.clone()).unwrap();
    assert!(!library.is_installed(&repository, package.clone()));

    // package being in repo and vendor/pkg/foo dir present means it is seen as installed
    let pkg_dir = format!("{}/{}", setup.vendor_dir, package.get_pretty_name());
    fs::create_dir_all(&pkg_dir).unwrap();
    assert!(library.is_installed(&repository, package.clone()));

    repository.remove_package(package.clone()).unwrap();
    assert!(!library.is_installed(&repository, package));

    tear_down(&mut setup);
}

#[test]
#[ignore = "requires PHPUnit mock of DownloadManager (expects(once)->method('install')->will(resolve(null))) and InstalledRepositoryInterface (expects(once)->addPackage); a real DownloadManager would attempt a download"]
fn test_install() {
    todo!()
}

#[test]
#[ignore = "requires PHPUnit mocks (Filesystem::rename, DownloadManager::update, repository hasPackage/add/remove with onConsecutiveCalls) and Package::setTargetDir, which is not exposed on PackageInterfaceHandle"]
fn test_update() {
    todo!()
}

#[test]
#[ignore = "requires PHPUnit mock of DownloadManager (expects(once)->method('remove')->will(resolve(null))) and InstalledRepositoryInterface (hasPackage onConsecutiveCalls, removePackage)"]
fn test_uninstall() {
    todo!()
}

#[ignore]
#[test]
fn test_get_install_path_without_target_dir() {
    let mut setup = set_up();
    let mut library =
        LibraryInstaller::new(setup.io.clone(), setup.composer.clone(), None, None, None);
    let package = get_package("Vendor/Pkg", "1.0.0");

    assert_eq!(
        format!("{}/{}", setup.vendor_dir, package.get_pretty_name()),
        library.get_install_path(package).unwrap()
    );

    tear_down(&mut setup);
}

#[test]
#[ignore = "Package::setTargetDir is not exposed on PackageInterfaceHandle, so the target-dir cannot be set on the test package"]
fn test_get_install_path_with_target_dir() {
    todo!()
}

#[test]
#[ignore = "requires PHPUnit mock of BinaryInstaller (expects(never)->removeBinaries, expects(once)->installBinaries) injected via LibraryInstaller's binaryInstaller argument"]
fn test_ensure_binaries_installed() {
    todo!()
}
