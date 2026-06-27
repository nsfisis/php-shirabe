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
use shirabe::repository::RepositoryInterface;
use shirabe::repository::WritableRepositoryInterface;
use shirabe::util::filesystem::Filesystem;
use shirabe_php_shim::PhpMixed;

use crate::downloader_stub::{DownloaderCall, DownloaderStub};
use crate::test_case::get_package;

fn run<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(future)
}

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
    /// Recorded downloader operations forwarded by the LibraryInstaller, mirroring the
    /// PHP DownloadManager mock's expectations on install/update/remove.
    downloader_calls: Rc<RefCell<Vec<DownloaderCall>>>,
    _composer_rc: ComposerHandle,
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

    let mut dm = DownloadManager::new(io.clone(), false, None);
    let downloader = DownloaderStub::new();
    let downloader_calls = downloader.calls();
    dm.set_downloader("fake", Rc::new(RefCell::new(downloader)));
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
        downloader_calls,
        _composer_rc: composer,
    }
}

/// Builds a `dist`-installable test package backed by the `fake` downloader stub, so the
/// real DownloadManager dispatches install/update/remove to the recording stub instead of
/// erroring on a package with no installation source.
fn get_installable_package(name: &str, version: &str) -> shirabe::package::PackageInterfaceHandle {
    let package = get_package(name, version);
    package.set_installation_source(Some("dist".to_string()));
    package.set_dist_type(Some("fake".to_string()));
    package
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
    let mut library =
        LibraryInstaller::new(setup.io.clone(), setup.composer.clone(), None, None, None);
    let package = get_installable_package("some/package", "1.0.0");

    // PHP asserts the DownloadManager mock's install() is called once with
    // ($package, vendorDir/some/package); here the recording downloader stub
    // captures the forwarded call instead.
    let mut repository = InstalledArrayRepository::new().unwrap();

    run(library.install(&mut repository, package.clone())).unwrap();

    assert_eq!(
        vec![DownloaderCall::Install {
            package: "some/package".to_string(),
            path: format!("{}/some/package", setup.vendor_dir),
        }],
        *setup.downloader_calls.borrow()
    );
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

    let initial = get_installable_package("vendor/package1", "1.0.0");
    let target = get_installable_package("vendor/package1", "2.0.0");

    initial.__set_target_dir(Some("oldtarget".to_string()));
    target.__set_target_dir(Some("newtarget".to_string()));

    // PHP mocks Filesystem::rename; here a real Filesystem renames the actual oldtarget
    // dir, so it must exist first. The install path embeds the pretty-name twice because
    // the package is named vendor/package1 and lives under vendorDir.
    let old_target_dir = format!("{}/vendor/package1/oldtarget", setup.vendor_dir);
    let new_target_dir = format!("{}/vendor/package1/newtarget", setup.vendor_dir);
    fs::create_dir_all(&old_target_dir).unwrap();

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

    assert_eq!(
        vec![DownloaderCall::Update {
            initial: "vendor/package1".to_string(),
            target: "vendor/package1".to_string(),
            path: new_target_dir.clone(),
        }],
        *setup.downloader_calls.borrow()
    );
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
    let mut library =
        LibraryInstaller::new(setup.io.clone(), setup.composer.clone(), None, None, None);
    let package = get_installable_package("vendor/pkg", "1.0.0");

    // PHP mocks hasPackage to return (true, false) over two calls; a real repository
    // seeded with the package reproduces this naturally: present, then absent after
    // the first uninstall removes it.
    let mut repository = InstalledArrayRepository::new().unwrap();
    repository.add_package(package.clone()).unwrap();

    run(library.uninstall(&mut repository, package.clone())).unwrap();

    assert_eq!(
        vec![DownloaderCall::Remove {
            package: "vendor/pkg".to_string(),
            path: format!("{}/vendor/pkg", setup.vendor_dir),
        }],
        *setup.downloader_calls.borrow()
    );
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

/// Records the calls a `BinaryInstaller` double receives, standing in for the PHPUnit mock that
/// asserts `removeBinaries` is never called and `installBinaries` is called once.
#[derive(Debug, Default)]
struct BinaryInstallerCalls {
    install_binaries: Vec<(shirabe::package::PackageInterfaceHandle, String, bool)>,
    remove_binaries: Vec<shirabe::package::PackageInterfaceHandle>,
}

#[derive(Debug)]
struct RecordingBinaryInstaller {
    calls: Rc<RefCell<BinaryInstallerCalls>>,
}

impl shirabe::installer::BinaryInstallerInterface for RecordingBinaryInstaller {
    fn install_binaries(
        &mut self,
        package: shirabe::package::PackageInterfaceHandle,
        install_path: &str,
        warn_on_overwrite: bool,
    ) {
        self.calls.borrow_mut().install_binaries.push((
            package,
            install_path.to_string(),
            warn_on_overwrite,
        ));
    }

    fn remove_binaries(&mut self, package: shirabe::package::PackageInterfaceHandle) {
        self.calls.borrow_mut().remove_binaries.push(package);
    }
}

#[test]
fn test_ensure_binaries_installed() {
    let mut setup = set_up();
    let calls = Rc::new(RefCell::new(BinaryInstallerCalls::default()));
    let mut library = LibraryInstaller::new(
        setup.io.clone(),
        setup.composer.clone(),
        Some("library".to_string()),
        None,
        None,
    );
    library.__set_binary_installer(std::rc::Rc::new(std::cell::RefCell::new(
        RecordingBinaryInstaller {
            calls: calls.clone(),
        },
    )));
    let package = get_package("foo/bar", "1.0.0");
    let expected_path = library.get_install_path(package.clone()).unwrap();

    library.ensure_binaries_presence(package.clone());

    let recorded = calls.borrow();
    // PHP asserts removeBinaries is never called.
    assert!(recorded.remove_binaries.is_empty());
    // PHP asserts installBinaries is called once with ($package, getInstallPath, false).
    assert_eq!(recorded.install_binaries.len(), 1);
    let (recorded_package, recorded_path, warn_on_overwrite) = &recorded.install_binaries[0];
    assert!(Rc::ptr_eq(recorded_package.as_rc(), package.as_rc()));
    assert_eq!(recorded_path, &expected_path);
    assert!(!warn_on_overwrite);
    drop(recorded);

    tear_down(&mut setup);
}
