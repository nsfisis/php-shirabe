//! ref: composer/tests/Composer/Test/Installer/InstallationManagerTest.php

use crate::test_case::get_package;
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
use std::cell::RefCell;
use std::rc::Rc;

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

// Equivalent to `getMockBuilder(InstallerInterface::class)->getMock()`. mockall cannot generate an
// `#[async_trait]` impl for the async methods that take `&mut dyn InstalledRepositoryInterface`
// (the object lifetime async_trait inserts clashes with mockall's generated lifetimes), so the
// expectations live on inherent methods and a thin hand-written InstallerInterface impl forwards to
// them, dropping the unused `repo` argument exactly as the PHPUnit mock ignores it. The methods not
// configured by any test (is_installed/download/prepare/cleanup/get_install_path, and the defaulted
// as_binary_presence_interface/as_plugin_installer_mut) return the same defaults as an unconfigured
// PHPUnit mock.
mockall::mock! {
    #[derive(Debug)]
    pub Installer {
        fn supports(&self, package_type: &str) -> bool;
        fn install(
            &mut self,
            package: PackageInterfaceHandle,
        ) -> anyhow::Result<Option<PhpMixed>>;
        fn update(
            &mut self,
            initial: PackageInterfaceHandle,
            target: PackageInterfaceHandle,
        ) -> anyhow::Result<Option<PhpMixed>>;
        fn uninstall(
            &mut self,
            package: PackageInterfaceHandle,
        ) -> anyhow::Result<Option<PhpMixed>>;
    }
}

#[async_trait::async_trait(?Send)]
impl InstallerInterface for MockInstaller {
    fn supports(&self, package_type: &str) -> bool {
        MockInstaller::supports(self, package_type)
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
        MockInstaller::install(self, package)
    }

    async fn update(
        &mut self,
        _repo: &mut dyn InstalledRepositoryInterface,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        MockInstaller::update(self, initial, target)
    }

    async fn uninstall(
        &mut self,
        _repo: &mut dyn InstalledRepositoryInterface,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        MockInstaller::uninstall(self, package)
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
    let mut installer = MockInstaller::new();
    // PHP expects supports() to be called exactly twice (once for the cached 'vendor' lookup,
    // once for the failing 'unregistered' lookup), returning true only for 'vendor'.
    installer
        .expect_supports()
        .times(2)
        .returning(|arg| arg == "vendor");

    let mut manager =
        shirabe::installer::InstallationManager::new(set_up.loop_.clone(), set_up.io.clone(), None);

    manager.add_installer(Box::new(installer));
    assert!(manager.get_installer("vendor").is_ok());

    assert!(manager.get_installer("unregistered").is_err());
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
    let package = get_package("test/pkg", "1.0.0");

    let mut installer = MockInstaller::new();
    installer
        .expect_supports()
        .times(1)
        .withf(|package_type| package_type == "library")
        .returning(|_| true);
    let expected = package.clone();
    installer
        .expect_install()
        .times(1)
        .withf_st(move |package| same_handle(package, &expected))
        .returning(|_| Ok(None));

    let mut manager =
        shirabe::installer::InstallationManager::new(set_up.loop_.clone(), set_up.io.clone(), None);
    manager.add_installer(Box::new(installer));

    let operation = InstallOperation::new(package.clone());

    let mut repository = InstalledArrayRepository::new().unwrap();
    run(manager.install(&mut repository, &operation));
}

#[test]
fn test_update_with_equal_types() {
    let set_up = set_up();
    let initial = get_package("test/initial", "1.0.0");
    let target = get_package("test/target", "1.0.1");

    let mut installer = MockInstaller::new();
    installer
        .expect_supports()
        .times(1)
        .withf(|package_type| package_type == "library")
        .returning(|_| true);
    let expected_initial = initial.clone();
    let expected_target = target.clone();
    installer
        .expect_update()
        .times(1)
        .withf_st(move |initial, target| {
            same_handle(initial, &expected_initial) && same_handle(target, &expected_target)
        })
        .returning(|_, _| Ok(None));

    let mut manager =
        shirabe::installer::InstallationManager::new(set_up.loop_.clone(), set_up.io.clone(), None);
    manager.add_installer(Box::new(installer));

    let operation = UpdateOperation::new(initial.clone(), target.clone());

    let mut repository = InstalledArrayRepository::new().unwrap();
    run(manager.update(&mut repository, &operation));
}

#[test]
fn test_update_with_not_equal_types() {
    let set_up = set_up();
    let initial = typed_package("test/initial", "1.0.0", "library");
    let target = typed_package("test/target", "1.0.1", "bundles");

    let mut lib_installer = MockInstaller::new();
    lib_installer
        .expect_supports()
        .times(1)
        .withf(|package_type| package_type == "library")
        .returning(|_| true);
    let expected_initial = initial.clone();
    lib_installer
        .expect_uninstall()
        .times(1)
        .withf_st(move |package| same_handle(package, &expected_initial))
        .returning(|_| Ok(None));

    let mut bundle_installer = MockInstaller::new();
    bundle_installer
        .expect_supports()
        .times(2)
        .returning(|arg| arg == "bundles");
    let expected_target = target.clone();
    bundle_installer
        .expect_install()
        .times(1)
        .withf_st(move |package| same_handle(package, &expected_target))
        .returning(|_| Ok(None));

    let mut manager =
        shirabe::installer::InstallationManager::new(set_up.loop_.clone(), set_up.io.clone(), None);
    manager.add_installer(Box::new(lib_installer));
    manager.add_installer(Box::new(bundle_installer));

    let operation = UpdateOperation::new(initial.clone(), target.clone());

    let mut repository = InstalledArrayRepository::new().unwrap();
    run(manager.update(&mut repository, &operation));
}

#[test]
fn test_uninstall() {
    let set_up = set_up();
    let package = get_package("test/pkg", "1.0.0");

    let mut installer = MockInstaller::new();
    installer
        .expect_supports()
        .times(1)
        .withf(|package_type| package_type == "library")
        .returning(|_| true);
    let expected = package.clone();
    installer
        .expect_uninstall()
        .times(1)
        .withf_st(move |package| same_handle(package, &expected))
        .returning(|_| Ok(None));

    let mut manager =
        shirabe::installer::InstallationManager::new(set_up.loop_.clone(), set_up.io.clone(), None);
    manager.add_installer(Box::new(installer));

    let operation = UninstallOperation::new(package.clone());

    let mut repository = InstalledArrayRepository::new().unwrap();
    run(manager.uninstall(&mut repository, &operation));
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
