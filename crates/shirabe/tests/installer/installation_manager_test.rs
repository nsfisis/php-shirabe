//! ref: composer/tests/Composer/Test/Installer/InstallationManagerTest.php

use std::cell::RefCell;
use std::rc::Rc;

use shirabe::dependency_resolver::operation::{
    InstallOperation, UninstallOperation, UpdateOperation,
};
use shirabe::installer::{BinaryPresenceInterface, InstallerInterface};
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::package::PackageInterfaceHandle;
use shirabe::package::handle::CompletePackageHandle;
use shirabe::repository::{InstalledArrayRepository, InstalledRepositoryInterface};
use shirabe::util::http_downloader::HttpDownloader;
use shirabe::util::r#loop::Loop;
use shirabe_php_shim::PhpMixed;
use shirabe_semver::VersionParser;

use crate::test_case::get_package;

fn run<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(future)
}

/// ref: setUp(): the PHP loop/io mocks are never exercised by these tests (the loop has its
/// constructor disabled), so a real Loop over a real HttpDownloader and a NullIO stand in.
struct SetUp {
    loop_: Rc<RefCell<Loop>>,
    io: Rc<RefCell<dyn IOInterface>>,
}

fn set_up() -> SetUp {
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let config = Rc::new(RefCell::new(shirabe::config::Config::new(false, None)));
    // The PHP loop mock has its constructor disabled and is never exercised by these tests, so a
    // mock HttpDownloader (no real curl backend) stands in.
    let http_downloader = Rc::new(RefCell::new(HttpDownloader::__new_mock(io.clone(), config)));
    let loop_ = Rc::new(RefCell::new(Loop::new(http_downloader, None)));
    SetUp { loop_, io }
}

/// Records of the calls a `CountingInstaller` received, shared so the test can inspect them
/// after the installer has been moved into the InstallationManager. This reproduces PHPUnit's
/// `expects($this->exactly(n))->method(...)->with(...)` assertions with explicit counters.
#[derive(Debug, Default)]
struct InstallerCalls {
    supports_args: Vec<String>,
    install: Vec<PackageInterfaceHandle>,
    uninstall: Vec<PackageInterfaceHandle>,
    update: Vec<(PackageInterfaceHandle, PackageInterfaceHandle)>,
}

/// Configurable `InstallerInterface` stub, equivalent to
/// `getMockBuilder(InstallerInterface::class)->getMock()`. `supports` returns true only when the
/// requested type equals `supported_type` (PHP uses a returnCallback comparing `$arg === 'vendor'`),
/// and every method records its arguments into the shared `calls`.
#[derive(Debug)]
struct CountingInstaller {
    supported_type: String,
    calls: Rc<RefCell<InstallerCalls>>,
}

impl CountingInstaller {
    fn new(supported_type: &str) -> (Self, Rc<RefCell<InstallerCalls>>) {
        let calls = Rc::new(RefCell::new(InstallerCalls::default()));
        (
            Self {
                supported_type: supported_type.to_string(),
                calls: calls.clone(),
            },
            calls,
        )
    }
}

#[async_trait::async_trait(?Send)]
impl InstallerInterface for CountingInstaller {
    fn supports(&self, package_type: &str) -> bool {
        self.calls
            .borrow_mut()
            .supports_args
            .push(package_type.to_string());
        package_type == self.supported_type
    }

    fn is_installed(
        &mut self,
        _repo: &dyn InstalledRepositoryInterface,
        _package: PackageInterfaceHandle,
    ) -> bool {
        false
    }

    async fn download(
        &mut self,
        _package: PackageInterfaceHandle,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }

    async fn prepare(
        &mut self,
        _type: &str,
        _package: PackageInterfaceHandle,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }

    async fn install(
        &mut self,
        _repo: &mut dyn InstalledRepositoryInterface,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.calls.borrow_mut().install.push(package);
        Ok(None)
    }

    async fn update(
        &mut self,
        _repo: &mut dyn InstalledRepositoryInterface,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.calls.borrow_mut().update.push((initial, target));
        Ok(None)
    }

    async fn uninstall(
        &mut self,
        _repo: &mut dyn InstalledRepositoryInterface,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.calls.borrow_mut().uninstall.push(package);
        Ok(None)
    }

    async fn cleanup(
        &mut self,
        _type: &str,
        _package: PackageInterfaceHandle,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }

    fn get_install_path(&mut self, _package: PackageInterfaceHandle) -> Option<String> {
        None
    }
}

/// Records the calls a `BinaryInstaller` stub received. Standing in for the partial mock of
/// `LibraryInstaller` that PHP's `testInstallBinary` builds (only `supports`/`ensureBinariesPresence`
/// are exercised).
#[derive(Debug, Default)]
struct BinaryInstallerCalls {
    supports_args: Vec<String>,
    ensure_binaries_presence: Vec<PackageInterfaceHandle>,
}

#[derive(Debug)]
struct BinaryInstaller {
    calls: Rc<RefCell<BinaryInstallerCalls>>,
}

impl BinaryInstaller {
    fn new() -> (Self, Rc<RefCell<BinaryInstallerCalls>>) {
        let calls = Rc::new(RefCell::new(BinaryInstallerCalls::default()));
        (
            Self {
                calls: calls.clone(),
            },
            calls,
        )
    }
}

#[async_trait::async_trait(?Send)]
impl InstallerInterface for BinaryInstaller {
    fn supports(&self, package_type: &str) -> bool {
        self.calls
            .borrow_mut()
            .supports_args
            .push(package_type.to_string());
        package_type == "library"
    }

    fn is_installed(
        &mut self,
        _repo: &dyn InstalledRepositoryInterface,
        _package: PackageInterfaceHandle,
    ) -> bool {
        false
    }

    async fn download(
        &mut self,
        _package: PackageInterfaceHandle,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }

    async fn prepare(
        &mut self,
        _type: &str,
        _package: PackageInterfaceHandle,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }

    async fn install(
        &mut self,
        _repo: &mut dyn InstalledRepositoryInterface,
        _package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }

    async fn update(
        &mut self,
        _repo: &mut dyn InstalledRepositoryInterface,
        _initial: PackageInterfaceHandle,
        _target: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }

    async fn uninstall(
        &mut self,
        _repo: &mut dyn InstalledRepositoryInterface,
        _package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }

    async fn cleanup(
        &mut self,
        _type: &str,
        _package: PackageInterfaceHandle,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }

    fn get_install_path(&mut self, _package: PackageInterfaceHandle) -> Option<String> {
        None
    }

    fn as_binary_presence_interface(&mut self) -> Option<&mut dyn BinaryPresenceInterface> {
        Some(self)
    }
}

impl BinaryPresenceInterface for BinaryInstaller {
    fn ensure_binaries_presence(&mut self, package: PackageInterfaceHandle) {
        self.calls
            .borrow_mut()
            .ensure_binaries_presence
            .push(package);
    }
}

/// Builds a `CompletePackage` of the given type, mirroring TestCase::getPackage but with a custom
/// type set via `__set_type` (PackageInterfaceHandle does not expose the setter).
fn typed_package(name: &str, version: &str, r#type: &str) -> PackageInterfaceHandle {
    let norm_version = VersionParser.normalize(version, None).unwrap();
    let handle = CompletePackageHandle::new(name.to_string(), norm_version, version.to_string());
    handle.__set_type(r#type.to_string());
    handle.into()
}

fn same_handle(a: &PackageInterfaceHandle, b: &PackageInterfaceHandle) -> bool {
    Rc::ptr_eq(a.as_rc(), b.as_rc())
}

#[test]
fn test_add_get_installer() {
    let set_up = set_up();
    let (installer, calls) = CountingInstaller::new("vendor");

    let mut manager =
        shirabe::installer::InstallationManager::new(set_up.loop_.clone(), set_up.io.clone(), None);

    manager.add_installer(Box::new(installer));
    assert!(manager.get_installer("vendor").is_ok());

    assert!(manager.get_installer("unregistered").is_err());

    // PHP expects supports() to be called exactly twice (once for the cached 'vendor' lookup,
    // once for the failing 'unregistered' lookup).
    assert_eq!(calls.borrow().supports_args.len(), 2);
}

#[ignore = "removeInstaller compares installers by object identity, but add_installer moves the Box<dyn InstallerInterface> into the manager, leaving no &dyn reference to pass back to remove_installer; faithful reproduction needs a shared-ownership installer registry"]
#[test]
fn test_add_remove_installer() {
    todo!()
}

#[ignore = "partial mock of InstallationManager (onlyMethods install/update/uninstall) with expects(once)->with(...) is not reproducible without method-overriding mocks; execute() also takes the batched download path"]
#[test]
fn test_execute() {
    todo!()
}

#[test]
fn test_install() {
    let set_up = set_up();
    let (installer, calls) = CountingInstaller::new("library");
    let mut manager =
        shirabe::installer::InstallationManager::new(set_up.loop_.clone(), set_up.io.clone(), None);
    manager.add_installer(Box::new(installer));

    let package = get_package("test/pkg", "1.0.0");
    let operation = InstallOperation::new(package.clone());

    let mut repository = InstalledArrayRepository::new().unwrap();
    // install() returns the (empty) installer promise; the call is observed via the recorded args.
    run(manager.install(&mut repository, &operation));

    assert_eq!(calls.borrow().supports_args, vec!["library".to_string()]);
    assert_eq!(calls.borrow().install.len(), 1);
    assert!(same_handle(&calls.borrow().install[0], &package));
}

#[test]
fn test_update_with_equal_types() {
    let set_up = set_up();
    let (installer, calls) = CountingInstaller::new("library");
    let mut manager =
        shirabe::installer::InstallationManager::new(set_up.loop_.clone(), set_up.io.clone(), None);
    manager.add_installer(Box::new(installer));

    let initial = get_package("test/initial", "1.0.0");
    let target = get_package("test/target", "1.0.1");
    let operation = UpdateOperation::new(initial.clone(), target.clone());

    let mut repository = InstalledArrayRepository::new().unwrap();
    run(manager.update(&mut repository, &operation));

    assert_eq!(calls.borrow().supports_args, vec!["library".to_string()]);
    assert_eq!(calls.borrow().update.len(), 1);
    assert!(same_handle(&calls.borrow().update[0].0, &initial));
    assert!(same_handle(&calls.borrow().update[0].1, &target));
}

#[test]
fn test_update_with_not_equal_types() {
    let set_up = set_up();
    let (lib_installer, lib_calls) = CountingInstaller::new("library");
    let (bundle_installer, bundle_calls) = CountingInstaller::new("bundles");
    let mut manager =
        shirabe::installer::InstallationManager::new(set_up.loop_.clone(), set_up.io.clone(), None);
    manager.add_installer(Box::new(lib_installer));
    manager.add_installer(Box::new(bundle_installer));

    let initial = typed_package("test/initial", "1.0.0", "library");
    let target = typed_package("test/target", "1.0.1", "bundles");
    let operation = UpdateOperation::new(initial.clone(), target.clone());

    let mut repository = InstalledArrayRepository::new().unwrap();
    run(manager.update(&mut repository, &operation));

    // The lib installer uninstalls the initial package once.
    assert_eq!(lib_calls.borrow().uninstall.len(), 1);
    assert!(same_handle(&lib_calls.borrow().uninstall[0], &initial));

    // The bundle installer installs the target package once.
    assert_eq!(bundle_calls.borrow().install.len(), 1);
    assert!(same_handle(&bundle_calls.borrow().install[0], &target));
}

#[test]
fn test_uninstall() {
    let set_up = set_up();
    let (installer, calls) = CountingInstaller::new("library");
    let mut manager =
        shirabe::installer::InstallationManager::new(set_up.loop_.clone(), set_up.io.clone(), None);
    manager.add_installer(Box::new(installer));

    let package = get_package("test/pkg", "1.0.0");
    let operation = UninstallOperation::new(package.clone());

    let mut repository = InstalledArrayRepository::new().unwrap();
    run(manager.uninstall(&mut repository, &operation));

    assert_eq!(calls.borrow().supports_args, vec!["library".to_string()]);
    assert_eq!(calls.borrow().uninstall.len(), 1);
    assert!(same_handle(&calls.borrow().uninstall[0], &package));
}

#[test]
fn test_install_binary() {
    let set_up = set_up();
    let (installer, calls) = BinaryInstaller::new();
    let mut manager =
        shirabe::installer::InstallationManager::new(set_up.loop_.clone(), set_up.io.clone(), None);
    manager.add_installer(Box::new(installer));

    let package = get_package("test/pkg", "1.0.0");
    manager.ensure_binaries_presence(package.clone());

    assert_eq!(calls.borrow().supports_args, vec!["library".to_string()]);
    assert_eq!(calls.borrow().ensure_binaries_presence.len(), 1);
    assert!(same_handle(
        &calls.borrow().ensure_binaries_presence[0],
        &package
    ));
}
