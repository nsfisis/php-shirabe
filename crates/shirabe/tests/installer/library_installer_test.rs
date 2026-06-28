//! ref: composer/tests/Composer/Test/Installer/LibraryInstallerTest.php

use crate::test_case::get_package;
use indexmap::IndexMap;
use shirabe::composer::{
    ComposerHandle, PartialComposerHandle, PartialComposerWeakHandle, PartialOrFullComposer,
};
use shirabe::config::Config;
use shirabe::downloader::{DownloadManagerInterface, DownloaderInterface};
use shirabe::installer::{BinaryInstallerInterface, InstallerInterface, LibraryInstaller};
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::package::PackageInterfaceHandle;
use shirabe::repository::InstalledArrayRepository;
use shirabe::repository::RepositoryInterface;
use shirabe::repository::WritableRepositoryInterface;
use shirabe::util::filesystem::Filesystem;
use shirabe_php_shim::PhpMixed;
use std::cell::RefCell;
use std::fs;
use std::rc::Rc;
use tempfile::TempDir;

// PHP mocks `Composer\Downloader\DownloadManager` with getMockBuilder and asserts its
// install/update/remove calls; here the equivalent mock is injected into the Composer via
// setDownloadManager.
mockall::mock! {
    #[derive(Debug)]
    pub DownloadManager {}
    #[async_trait::async_trait(?Send)]
    impl DownloadManagerInterface for DownloadManager {
        fn set_prefer_source(&mut self, prefer_source: bool);
        fn set_prefer_dist(&mut self, prefer_dist: bool);
        fn get_downloader_for_package(
            &self,
            package: PackageInterfaceHandle,
        ) -> anyhow::Result<Option<Rc<RefCell<dyn DownloaderInterface>>>>;
        async fn download(
            &self,
            package: PackageInterfaceHandle,
            target_dir: &str,
            prev_package: Option<PackageInterfaceHandle>,
        ) -> anyhow::Result<Option<PhpMixed>>;
        async fn prepare(
            &self,
            r#type: &str,
            package: PackageInterfaceHandle,
            target_dir: &str,
            prev_package: Option<PackageInterfaceHandle>,
        ) -> anyhow::Result<Option<PhpMixed>>;
        async fn install(
            &self,
            package: PackageInterfaceHandle,
            target_dir: &str,
        ) -> anyhow::Result<Option<PhpMixed>>;
        async fn update(
            &self,
            initial: PackageInterfaceHandle,
            target: PackageInterfaceHandle,
            target_dir: &str,
        ) -> anyhow::Result<Option<PhpMixed>>;
        async fn remove(
            &self,
            package: PackageInterfaceHandle,
            target_dir: &str,
        ) -> anyhow::Result<Option<PhpMixed>>;
        async fn cleanup(
            &self,
            r#type: &str,
            package: PackageInterfaceHandle,
            target_dir: &str,
            prev_package: Option<PackageInterfaceHandle>,
        ) -> anyhow::Result<Option<PhpMixed>>;
    }
}

// PHP mocks `Composer\Installer\BinaryInstaller`; the Rust seam is `BinaryInstallerInterface`.
mockall::mock! {
    #[derive(Debug)]
    pub BinaryInstaller {}
    impl BinaryInstallerInterface for BinaryInstaller {
        fn install_binaries(
            &mut self,
            package: PackageInterfaceHandle,
            install_path: &str,
            warn_on_overwrite: bool,
        );
        fn remove_binaries(&mut self, package: PackageInterfaceHandle);
    }
}

fn run<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(future)
}

/// Mirror of setUp(): builds the Composer/Config over temp root/vendor/bin dirs plus a NullIO and a
/// DownloadManager mock. `composer_full` keeps the inner Rc alive for the duration of the test since
/// LibraryInstaller only holds a weak handle, and lets tests swap in a configured DownloadManager
/// mock before constructing the installer.
struct SetUp {
    root: TempDir,
    vendor_dir: String,
    bin_dir: String,
    io: Rc<RefCell<dyn IOInterface>>,
    composer: PartialComposerWeakHandle,
    fs: Filesystem,
    composer_full: ComposerHandle,
}

/// Replaces the Composer's DownloadManager with the given mock. Must be called before
/// `LibraryInstaller::new`, which captures the manager at construction time.
fn set_download_manager(setup: &SetUp, dm: MockDownloadManager) {
    setup
        .composer_full
        .borrow_mut()
        .set_download_manager(Rc::new(RefCell::new(dm)));
}

fn set_up() -> SetUp {
    let fs = Filesystem::new(None);

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

    let composer_rc = Rc::new(RefCell::new(PartialOrFullComposer::new_full()));
    let composer = ComposerHandle::from_rc_unchecked(composer_rc.clone());
    composer.borrow_mut().set_config(config_rc);
    // Default unconfigured mock so LibraryInstaller::new can resolve a DownloadManager even in
    // tests that exercise no downloader call; tests that assert calls install their own via
    // set_download_manager.
    composer
        .borrow_mut()
        .set_download_manager(Rc::new(RefCell::new(MockDownloadManager::new())));

    let weak = PartialComposerHandle::from_rc(composer_rc).downgrade();

    SetUp {
        root,
        vendor_dir,
        bin_dir,
        io,
        composer: weak,
        fs,
        composer_full: composer,
    }
}

fn tear_down(setup: &mut SetUp) {
    let root = setup.root.path().to_path_buf();
    setup.fs.remove_directory(&root).ok();
}

#[test]
fn test_installer_creation_should_not_create_vendor_directory() {
    let mut setup = set_up();
    setup.fs.remove_directory(&setup.vendor_dir).unwrap();

    LibraryInstaller::new(setup.io.clone(), setup.composer.clone(), None, None, None);
    assert!(!std::path::Path::new(&setup.vendor_dir).exists());

    tear_down(&mut setup);
}

#[test]
fn test_installer_creation_should_not_create_bin_directory() {
    let mut setup = set_up();
    setup.fs.remove_directory(&setup.bin_dir).unwrap();

    LibraryInstaller::new(setup.io.clone(), setup.composer.clone(), None, None, None);
    assert!(!std::path::Path::new(&setup.bin_dir).exists());

    tear_down(&mut setup);
}

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
fn test_install() {
    let mut setup = set_up();
    let package = get_package("some/package", "1.0.0");

    // PHP asserts the DownloadManager mock's install() is called once with
    // ($package, vendorDir/some/package).
    let mut dm = MockDownloadManager::new();
    let expected_package = package.clone();
    let expected_path = format!("{}/some/package", setup.vendor_dir);
    dm.expect_install()
        .times(1)
        .withf_st(move |package, target_dir| {
            Rc::ptr_eq(package.as_rc(), expected_package.as_rc())
                && target_dir == expected_path.as_str()
        })
        .returning(|_, _| Ok(None));
    set_download_manager(&setup, dm);

    let mut library =
        LibraryInstaller::new(setup.io.clone(), setup.composer.clone(), None, None, None);

    let mut repository = InstalledArrayRepository::new().unwrap();

    run(library.install(&mut repository, package.clone())).unwrap();

    // PHP asserts repository->addPackage was called once with $package.
    assert!(repository.has_package(package));

    assert!(
        std::path::Path::new(&setup.vendor_dir).exists(),
        "Vendor dir should be created"
    );
    assert!(
        std::path::Path::new(&setup.bin_dir).exists(),
        "Bin dir should be created"
    );

    tear_down(&mut setup);
}

#[test]
fn test_update() {
    let mut setup = set_up();

    let initial = get_package("vendor/package1", "1.0.0");
    let target = get_package("vendor/package1", "2.0.0");

    initial.__set_target_dir(Some("oldtarget".to_string()));
    target.__set_target_dir(Some("newtarget".to_string()));

    // PHP mocks Filesystem::rename; here a real Filesystem renames the actual oldtarget
    // dir, so it must exist first. The install path embeds the pretty-name twice because
    // the package is named vendor/package1 and lives under vendorDir.
    let old_target_dir = format!("{}/vendor/package1/oldtarget", setup.vendor_dir);
    let new_target_dir = format!("{}/vendor/package1/newtarget", setup.vendor_dir);
    fs::create_dir_all(&old_target_dir).unwrap();

    // PHP asserts the DownloadManager mock's update() is called once with
    // ($initial, $target, vendorDir/vendor/package1/newtarget).
    let mut dm = MockDownloadManager::new();
    let expected_initial = initial.clone();
    let expected_target = target.clone();
    let expected_path = new_target_dir.clone();
    dm.expect_update()
        .times(1)
        .withf_st(move |initial, target, target_dir| {
            Rc::ptr_eq(initial.as_rc(), expected_initial.as_rc())
                && Rc::ptr_eq(target.as_rc(), expected_target.as_rc())
                && target_dir == expected_path.as_str()
        })
        .returning(|_, _, _| Ok(None));
    set_download_manager(&setup, dm);

    let mut repository = InstalledArrayRepository::new().unwrap();
    repository.add_package(initial.clone()).unwrap();

    // The default Filesystem is fine; the LibraryInstaller's own filesystem performs the rename.
    let mut library =
        LibraryInstaller::new(setup.io.clone(), setup.composer.clone(), None, None, None);

    run(library.update(&mut repository, initial.clone(), target.clone())).unwrap();

    assert!(
        std::path::Path::new(&new_target_dir).exists(),
        "oldtarget should have been renamed to newtarget"
    );
    assert!(!std::path::Path::new(&old_target_dir).exists());

    assert!(!repository.has_package(initial.clone()));
    assert!(repository.has_package(target.clone()));

    assert!(
        std::path::Path::new(&setup.vendor_dir).exists(),
        "Vendor dir should be created"
    );
    assert!(
        std::path::Path::new(&setup.bin_dir).exists(),
        "Bin dir should be created"
    );

    // Updating again, with the initial package no longer installed, fails.
    assert!(run(library.update(&mut repository, initial, target)).is_err());

    tear_down(&mut setup);
}

#[test]
fn test_uninstall() {
    let mut setup = set_up();
    let package = get_package("vendor/pkg", "1.0.0");

    // PHP asserts the DownloadManager mock's remove() is called once with
    // ($package, vendorDir/vendor/pkg).
    let mut dm = MockDownloadManager::new();
    let expected_package = package.clone();
    let expected_path = format!("{}/vendor/pkg", setup.vendor_dir);
    dm.expect_remove()
        .times(1)
        .withf_st(move |package, target_dir| {
            Rc::ptr_eq(package.as_rc(), expected_package.as_rc())
                && target_dir == expected_path.as_str()
        })
        .returning(|_, _| Ok(None));
    set_download_manager(&setup, dm);

    let mut library =
        LibraryInstaller::new(setup.io.clone(), setup.composer.clone(), None, None, None);

    // PHP mocks hasPackage to return (true, false) over two calls; a real repository
    // seeded with the package reproduces this naturally: present, then absent after
    // the first uninstall removes it.
    let mut repository = InstalledArrayRepository::new().unwrap();
    repository.add_package(package.clone()).unwrap();

    run(library.uninstall(&mut repository, package.clone())).unwrap();

    assert!(!repository.has_package(package.clone()));

    // Uninstalling again, with the package no longer installed, fails.
    assert!(run(library.uninstall(&mut repository, package)).is_err());

    tear_down(&mut setup);
}

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
fn test_get_install_path_with_target_dir() {
    let mut setup = set_up();
    let mut library =
        LibraryInstaller::new(setup.io.clone(), setup.composer.clone(), None, None, None);
    let package = get_package("Foo/Bar", "1.0.0");
    package.__set_target_dir(Some("Some/Namespace".to_string()));

    assert_eq!(
        format!(
            "{}/{}/Some/Namespace",
            setup.vendor_dir,
            package.get_pretty_name()
        ),
        library.get_install_path(package).unwrap()
    );

    tear_down(&mut setup);
}

#[test]
fn test_ensure_binaries_installed() {
    let mut setup = set_up();
    let mut library = LibraryInstaller::new(
        setup.io.clone(),
        setup.composer.clone(),
        Some("library".to_string()),
        None,
        None,
    );
    let package = get_package("foo/bar", "1.0.0");
    let expected_path = library.get_install_path(package.clone()).unwrap();

    let mut binary_installer = MockBinaryInstaller::new();
    // PHP asserts removeBinaries is never called.
    binary_installer.expect_remove_binaries().times(0);
    // PHP asserts installBinaries is called once with ($package, getInstallPath, false).
    let expected_package = package.clone();
    let expected_install_path = expected_path.clone();
    binary_installer
        .expect_install_binaries()
        .times(1)
        .withf_st(move |package, install_path, warn_on_overwrite| {
            Rc::ptr_eq(package.as_rc(), expected_package.as_rc())
                && install_path == expected_install_path.as_str()
                && !*warn_on_overwrite
        })
        .returning(|_, _, _| ());
    library.__set_binary_installer(Rc::new(RefCell::new(binary_installer)));

    library.ensure_binaries_presence(package.clone());

    tear_down(&mut setup);
}
