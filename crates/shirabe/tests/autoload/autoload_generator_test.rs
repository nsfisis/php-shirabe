//! ref: composer/tests/Composer/Test/Autoload/AutoloadGeneratorTest.php sha1:49ef7fbbc645f03edff5d75bd89005271cbfffd2

use crate::config_stub::ConfigStubBuilder;
use bstr::{B, BStr, BString, ByteSlice};
use indexmap::IndexMap;
use serial_test::serial;
use shirabe::autoload::AutoloadGenerator;
use shirabe::composer::{ComposerHandle, PartialOrFullComposer};
use shirabe::config::Config;
use shirabe::event_dispatcher::EventDispatcher;
use shirabe::filter::platform_requirement_filter::PlatformRequirementFilterFactory;
use shirabe::installer::{InstallationManager, InstallerInterface};
use shirabe::io::{BufferIO, IOInterface};
use shirabe::package::handle::{
    AliasPackageHandle, CompletePackageHandle, PackageHandle, RootPackageHandle,
};
use shirabe::package::{Link, PackageInterfaceHandle, RootPackageInterfaceHandle};
use shirabe::repository::{InstalledArrayRepository, InstalledRepositoryInterfaceHandle};
use shirabe::script::ScriptEvents;
use shirabe::util::http_downloader::HttpDownloader;
use shirabe::util::r#loop::Loop;
use shirabe_php_shim::{
    PhpMixed, bconcat, dirname, error_message, path_from_bytes, path_to_bytes, preg_quote,
    realpath, strtr, symlink,
};
use shirabe_semver::VersionParser;
use shirabe_semver::constraint::{AnyConstraint, MatchAllConstraint, SimpleConstraint};
use shirabe_symfony_console::output::output_interface;
use tempfile::TempDir;

/// The mock `InstallationManager::getInstallPath` used throughout the test: metapackages return
/// null, every other package returns `vendorDir/<name>(/<targetDir>)`. Registered as an installer
/// that supports every type so `InstallationManager::getInstallPath` routes to it.
#[derive(Debug)]
struct InstallPathStubInstaller {
    vendor_dir: std::path::PathBuf,
}

#[async_trait::async_trait(?Send)]
impl InstallerInterface for InstallPathStubInstaller {
    fn supports(&self, _package_type: &bstr::BStr) -> anyhow::Result<bool> {
        Ok(true)
    }

    fn is_installed(
        &self,
        _repo: &InstalledRepositoryInterfaceHandle,
        _package: PackageInterfaceHandle,
    ) -> anyhow::Result<bool> {
        Ok(true)
    }

    async fn download(
        &self,
        _package: PackageInterfaceHandle,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }

    async fn prepare(
        &self,
        _type: &bstr::BStr,
        _package: PackageInterfaceHandle,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }

    async fn install(
        &self,
        _repo: &InstalledRepositoryInterfaceHandle,
        _package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }

    async fn update(
        &self,
        _repo: &InstalledRepositoryInterfaceHandle,
        _initial: PackageInterfaceHandle,
        _target: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }

    async fn uninstall(
        &self,
        _repo: &InstalledRepositoryInterfaceHandle,
        _package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }

    async fn cleanup(
        &self,
        _type: &bstr::BStr,
        _package: PackageInterfaceHandle,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }

    fn get_install_path(&self, package: PackageInterfaceHandle) -> Option<std::path::PathBuf> {
        if package.get_type() == "metapackage" {
            return None;
        }

        let mut path = self.vendor_dir.join(path_from_bytes(&package.get_name()));
        if let Some(dir) = package.get_target_dir()
            && !dir.is_empty()
        {
            path = path.join(path_from_bytes(&dir));
        }

        Some(path.to_path_buf())
    }
}

/// Mirrors the PHP `setUp`/`tearDown` lifecycle: a fresh temp working dir, a `composer-test-autoload`
/// vendor dir inside it, `chdir`ed into the working dir, plus the mocked Config/InstallationManager/
/// repository/EventDispatcher and BufferIO. The temp tree is removed and the cwd restored on drop.
struct SetUp {
    _temp_dir: TempDir,
    prev_cwd: std::path::PathBuf,
    working_dir: std::path::PathBuf,
    vendor_dir: std::path::PathBuf,
    repository: shirabe::repository::RepositoryInterfaceHandle,
    /// ref: `$this->configValueMap['use-include-path']`, which testUseGlobalIncludePath mutates.
    use_include_path: bool,
    im: std::rc::Rc<std::cell::RefCell<dyn shirabe::installer::InstallationManagerInterface>>,
    io: std::rc::Rc<std::cell::RefCell<BufferIO>>,
    generator: AutoloadGenerator,
    event_dispatcher: std::rc::Rc<std::cell::RefCell<EventDispatcher>>,
}

impl Drop for SetUp {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.prev_cwd);
    }
}

fn null_path(s: &BStr) -> BString {
    bconcat!(s)
}

/// Builds an InstallationManager whose single installer routes `getInstallPath` through
/// `InstallPathStubInstaller`. The PHP loop mock has its constructor disabled and is never
/// exercised here, so a mock HttpDownloader (no real curl backend) stands in for the loop.
fn make_installation_manager(
    vendor_dir: &std::path::Path,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
) -> InstallationManager {
    let config_for_downloader = std::rc::Rc::new(std::cell::RefCell::new(Config::new(false, None)));
    let http_downloader = std::rc::Rc::new(std::cell::RefCell::new(HttpDownloader::__new_mock(
        io.clone(),
        config_for_downloader,
    )));
    let loop_ = std::rc::Rc::new(std::cell::RefCell::new(Loop::new(http_downloader, None)));

    let im = InstallationManager::new(loop_, io, None);
    im.add_installer(std::rc::Rc::new(InstallPathStubInstaller {
        vendor_dir: vendor_dir.to_path_buf(),
    }));
    im
}

fn set_up() -> SetUp {
    let temp_dir = TempDir::new().unwrap();
    let working_dir = temp_dir.path().to_path_buf();
    let vendor_dir = working_dir.join("composer-test-autoload");
    std::fs::create_dir_all(&vendor_dir).unwrap();

    let prev_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(&working_dir).unwrap();

    let io = std::rc::Rc::new(std::cell::RefCell::new(
        BufferIO::new(BString::default(), output_interface::VERBOSITY_NORMAL, None).unwrap(),
    ));

    let dispatcher_io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io.clone();
    let im: std::rc::Rc<std::cell::RefCell<dyn shirabe::installer::InstallationManagerInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(make_installation_manager(
            &vendor_dir,
            dispatcher_io.clone(),
        )));

    let repository = shirabe::repository::RepositoryInterfaceHandle::new(
        InstalledArrayRepository::new().unwrap(),
    );

    // EventDispatcher constructor is disabled in PHP and dispatch is never called when run-scripts
    // is off (the default), so a real dispatcher over an empty Composer is a faithful no-op stand-in.
    let composer = ComposerHandle::from_rc_unchecked(std::rc::Rc::new(std::cell::RefCell::new(
        PartialOrFullComposer::new_full(),
    )));
    let event_dispatcher = std::rc::Rc::new(std::cell::RefCell::new(EventDispatcher::new(
        composer.upcast().downgrade(),
        dispatcher_io.clone(),
        None,
    )));

    let generator = AutoloadGenerator::new(event_dispatcher.clone(), Some(dispatcher_io));

    SetUp {
        _temp_dir: temp_dir,
        prev_cwd,
        working_dir,
        vendor_dir,
        repository,
        use_include_path: false,
        im,
        io,
        generator,
        event_dispatcher,
    }
}

impl SetUp {
    /// Builds the mocked Config returning `vendor-dir`/`platform-check`/`use-include-path`, mirroring
    /// the PHP `configValueMap`. Rebuilt per call because tests mutate `vendor_dir`.
    fn config(&self) -> Config {
        ConfigStubBuilder::new()
            .with(
                "vendor-dir",
                PhpMixed::string(path_to_bytes(&self.vendor_dir)),
            )
            .with("platform-check", PhpMixed::Bool(true))
            .with("use-include-path", PhpMixed::Bool(self.use_include_path))
            .build()
    }

    fn ensure_dir(&self, path: impl AsRef<std::path::Path>) {
        std::fs::create_dir_all(path).unwrap();
    }

    fn put(&self, path: impl AsRef<std::path::Path>, contents: &BStr) {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, contents).unwrap();
    }

    fn set_canonical_packages(&mut self, packages: Vec<PackageInterfaceHandle>) {
        for p in packages {
            self.repository
                .borrow_mut()
                .as_writable_repository_interface_mut()
                .expect("the test repository is writable")
                .add_package(p)
                .unwrap();
        }
    }
}

fn fixtures_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../composer/tests/Composer/Test/Autoload/Fixtures")
        .canonicalize()
        .unwrap()
}

/// ref: AutoloadGeneratorTest::assertAutoloadFiles
#[track_caller]
fn assert_autoload_files(name: &BStr, dir: impl AsRef<std::path::Path>, r#type: &BStr) {
    let a = fixtures_dir().join(path_from_bytes(&bconcat!("autoload_", name, ".php")));
    let b = dir
        .as_ref()
        .join(path_from_bytes(&bconcat!("autoload_", r#type, ".php")));
    assert_file_content_equals(a, b);
}

/// ref: AutoloadGeneratorTest::assertFileContentEquals
#[track_caller]
fn assert_file_content_equals(
    expected: impl AsRef<std::path::Path>,
    actual: impl AsRef<std::path::Path>,
) {
    let (expected, actual) = (expected.as_ref(), actual.as_ref());
    let exp = bstr::BString::from(
        std::fs::read(expected)
            .unwrap_or_else(|e| panic!("read {}: {}", expected.display(), e))
            .replace("\r", ""),
    );
    let act = bstr::BString::from(
        std::fs::read(actual)
            .unwrap_or_else(|e| panic!("read {}: {}", actual.display(), e))
            .replace("\r", ""),
    );
    assert_eq!(
        exp,
        act,
        "{} equals {}",
        expected.display(),
        actual.display()
    );
}

/// ref: PHPUnit `assertStringEqualsFile`
#[track_caller]
fn assert_str_equals_file(expected: &BStr, actual_path: impl AsRef<std::path::Path>) {
    let actual_path = actual_path.as_ref();
    let act = bstr::BString::from(
        std::fs::read(actual_path)
            .unwrap_or_else(|e| panic!("read {}: {}", actual_path.display(), e))
            .replace("\r", ""),
    );
    assert_eq!(expected, act, "content of {}", actual_path.display());
}

fn match_all() -> AnyConstraint {
    AnyConstraint::MatchAll(MatchAllConstraint::new(None))
}

fn constraint(operator: &BStr, version: &BStr) -> AnyConstraint {
    AnyConstraint::Simple(SimpleConstraint::new(operator, bconcat!(version), None))
}

fn link(
    source: &BStr,
    target: &BStr,
    constraint: AnyConstraint,
    description: Option<&bstr::BStr>,
) -> Link {
    Link::new(
        bconcat!(source),
        bconcat!(target),
        constraint,
        description.map(|d| d.to_owned()),
        Some(BString::default()),
    )
}

fn requires(links: Vec<(&BStr, Link)>) -> IndexMap<BString, Link> {
    links.into_iter().map(|(k, v)| (bconcat!(k), v)).collect()
}

fn autoload(entries: Vec<(&BStr, PhpMixed)>) -> IndexMap<BString, PhpMixed> {
    entries.into_iter().map(|(k, v)| (bconcat!(k), v)).collect()
}

fn str_list(items: &[&BStr]) -> PhpMixed {
    PhpMixed::List(
        items
            .iter()
            .map(|s| PhpMixed::string(bconcat!(s)))
            .collect(),
    )
}

fn str_map(entries: &[(&BStr, PhpMixed)]) -> PhpMixed {
    PhpMixed::Array(
        entries
            .iter()
            .map(|(k, v)| (bconcat!(k), v.clone()))
            .collect(),
    )
}

fn pstr(v: &BStr) -> PhpMixed {
    PhpMixed::string(bconcat!(v))
}

fn new_root_pkg(name: &BStr) -> RootPackageHandle {
    RootPackageHandle::new(
        bconcat!(name),
        null_path(BStr::new("1.0")),
        BString::from("1.0"),
    )
}

fn new_pkg(name: &BStr) -> PackageHandle {
    PackageHandle::new(bconcat!(name), BString::from("1.0"), BString::from("1.0"))
}

fn dump(
    s: &mut SetUp,
    package: RootPackageInterfaceHandle,
    scan_psr_packages: bool,
    suffix: &BStr,
) -> anyhow::Result<shirabe_class_map_generator::class_map::ClassMap> {
    let config = s.config();
    let SetUp {
        repository,
        im,
        generator,
        ..
    } = s;
    generator.dump(
        &config,
        repository.clone(),
        package,
        im.clone(),
        BStr::new("composer"),
        scan_psr_packages,
        Some(bconcat!(suffix)),
        None,
        false,
    )
}

#[test]
#[serial]
fn test_root_package_autoloading() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));
    package.set_autoload(autoload(vec![
        (
            BStr::new("psr-0"),
            str_map(&[
                (BStr::new("Main"), pstr(BStr::new("src/"))),
                (
                    BStr::new("Lala"),
                    str_list(&[BStr::new("src/"), BStr::new("lib/")]),
                ),
            ]),
        ),
        (
            BStr::new("psr-4"),
            str_map(&[
                (BStr::new("Acme\\Fruit\\"), pstr(BStr::new("src-fruit/"))),
                (
                    BStr::new("Acme\\Cake\\"),
                    str_list(&[BStr::new("src-cake/"), BStr::new("lib-cake/")]),
                ),
            ]),
        ),
        (
            BStr::new("classmap"),
            str_list(&[BStr::new("composersrc/")]),
        ),
    ]));

    s.ensure_dir(s.working_dir.join("composer"));
    s.ensure_dir(s.working_dir.join("src/Lala/Test"));
    s.ensure_dir(s.working_dir.join("lib"));
    s.put(
        s.working_dir.join("src/Lala/ClassMapMain.php"),
        BStr::new("<?php namespace Lala; class ClassMapMain {}"),
    );
    s.put(
        s.working_dir.join("src/Lala/Test/ClassMapMainTest.php"),
        BStr::new("<?php namespace Lala\\Test; class ClassMapMainTest {}"),
    );

    s.ensure_dir(s.working_dir.join("src-fruit"));
    s.ensure_dir(s.working_dir.join("src-cake"));
    s.ensure_dir(s.working_dir.join("lib-cake"));
    s.put(
        s.working_dir.join("src-cake/ClassMapBar.php"),
        BStr::new("<?php namespace Acme\\Cake; class ClassMapBar {}"),
    );

    s.ensure_dir(s.working_dir.join("composersrc"));
    s.put(
        s.working_dir.join("composersrc/foo.php"),
        BStr::new("<?php class ClassMapFoo {}"),
    );

    let composer_out = s.vendor_dir.join("composer");
    dump(&mut s, package.into(), true, BStr::new("_1")).unwrap();

    assert_autoload_files(BStr::new("main"), &composer_out, BStr::new("namespaces"));
    assert_autoload_files(BStr::new("psr4"), &composer_out, BStr::new("psr4"));
    assert_autoload_files(BStr::new("classmap"), &composer_out, BStr::new("classmap"));
}

#[test]
#[serial]
fn test_root_package_dev_autoloading() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));
    package.set_autoload(autoload(vec![(
        BStr::new("psr-0"),
        str_map(&[(BStr::new("Main"), pstr(BStr::new("src/")))]),
    )]));
    package.set_dev_autoload(autoload(vec![
        (
            BStr::new("files"),
            str_list(&[BStr::new("devfiles/foo.php")]),
        ),
        (
            BStr::new("psr-0"),
            str_map(&[(BStr::new("Main"), pstr(BStr::new("tests/")))]),
        ),
    ]));

    s.ensure_dir(s.working_dir.join("composer"));
    s.ensure_dir(s.working_dir.join("src/Main"));
    s.put(
        s.working_dir.join("src/Main/ClassMain.php"),
        BStr::new("<?php namespace Main; class ClassMain {}"),
    );
    s.ensure_dir(s.working_dir.join("devfiles"));
    s.put(
        s.working_dir.join("devfiles/foo.php"),
        BStr::new("<?php function foo() { echo \"foo\"; }"),
    );

    let composer_out = s.vendor_dir.join("composer");
    s.generator.set_dev_mode(true);
    dump(&mut s, package.into(), true, BStr::new("_1")).unwrap();

    assert_autoload_files(BStr::new("main5"), &composer_out, BStr::new("namespaces"));
    assert_autoload_files(BStr::new("classmap7"), &composer_out, BStr::new("classmap"));
    assert_autoload_files(BStr::new("files2"), &composer_out, BStr::new("files"));
}

#[test]
#[serial]
fn test_root_package_dev_autoloading_disabled_by_default() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));
    package.set_autoload(autoload(vec![(
        BStr::new("psr-0"),
        str_map(&[(BStr::new("Main"), pstr(BStr::new("src/")))]),
    )]));
    package.set_dev_autoload(autoload(vec![(
        BStr::new("files"),
        str_list(&[BStr::new("devfiles/foo.php")]),
    )]));

    s.ensure_dir(s.working_dir.join("composer"));
    s.ensure_dir(s.working_dir.join("src/Main"));
    s.put(
        s.working_dir.join("src/Main/ClassMain.php"),
        BStr::new("<?php namespace Main; class ClassMain {}"),
    );
    s.ensure_dir(s.working_dir.join("devfiles"));
    s.put(
        s.working_dir.join("devfiles/foo.php"),
        BStr::new("<?php function foo() { echo \"foo\"; }"),
    );

    let composer_out = s.vendor_dir.join("composer");
    dump(&mut s, package.into(), true, BStr::new("_1")).unwrap();

    assert_autoload_files(BStr::new("main4"), &composer_out, BStr::new("namespaces"));
    assert_autoload_files(BStr::new("classmap7"), &composer_out, BStr::new("classmap"));
    assert!(!std::path::Path::new(&composer_out.join("autoload_files.php")).is_file());
}

#[test]
#[serial]
fn test_vendor_dir_same_as_working_dir() {
    let mut s = set_up();
    s.vendor_dir = s.working_dir.clone();
    // Re-register the install-path stub so getInstallPath uses the new vendor dir.
    s.im.borrow()
        .add_installer(std::rc::Rc::new(InstallPathStubInstaller {
            vendor_dir: s.vendor_dir.clone(),
        }));

    let package = new_root_pkg(BStr::new("root/a"));
    package.set_autoload(autoload(vec![
        (
            BStr::new("psr-0"),
            str_map(&[
                (BStr::new("Main"), pstr(BStr::new("src/"))),
                (BStr::new("Lala"), pstr(BStr::new("src/"))),
            ]),
        ),
        (
            BStr::new("psr-4"),
            str_map(&[
                (BStr::new("Acme\\Fruit\\"), pstr(BStr::new("src-fruit/"))),
                (
                    BStr::new("Acme\\Cake\\"),
                    str_list(&[BStr::new("src-cake/"), BStr::new("lib-cake/")]),
                ),
            ]),
        ),
        (
            BStr::new("classmap"),
            str_list(&[BStr::new("composersrc/")]),
        ),
    ]));

    s.ensure_dir(s.vendor_dir.join("composer"));
    s.ensure_dir(s.vendor_dir.join("src/Main"));
    s.put(
        s.vendor_dir.join("src/Main/Foo.php"),
        BStr::new("<?php namespace Main; class Foo {}"),
    );
    s.ensure_dir(s.vendor_dir.join("composersrc"));
    s.put(
        s.vendor_dir.join("composersrc/foo.php"),
        BStr::new("<?php class ClassMapFoo {}"),
    );

    let composer_out = s.vendor_dir.join("composer");
    dump(&mut s, package.into(), true, BStr::new("_2")).unwrap();
    assert_autoload_files(BStr::new("main3"), &composer_out, BStr::new("namespaces"));
    assert_autoload_files(BStr::new("psr4_3"), &composer_out, BStr::new("psr4"));
    assert_autoload_files(BStr::new("classmap3"), &composer_out, BStr::new("classmap"));
}

#[test]
#[serial]
fn test_root_package_autoloading_alternative_vendor_dir() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));
    package.set_autoload(autoload(vec![
        (
            BStr::new("psr-0"),
            str_map(&[
                (BStr::new("Main"), pstr(BStr::new("src/"))),
                (BStr::new("Lala"), pstr(BStr::new("src/"))),
            ]),
        ),
        (
            BStr::new("psr-4"),
            str_map(&[
                (BStr::new("Acme\\Fruit\\"), pstr(BStr::new("src-fruit/"))),
                (
                    BStr::new("Acme\\Cake\\"),
                    str_list(&[BStr::new("src-cake/"), BStr::new("lib-cake/")]),
                ),
            ]),
        ),
        (
            BStr::new("classmap"),
            str_list(&[BStr::new("composersrc/")]),
        ),
    ]));

    s.vendor_dir = s.vendor_dir.join("subdir");
    s.im.borrow()
        .add_installer(std::rc::Rc::new(InstallPathStubInstaller {
            vendor_dir: s.vendor_dir.clone(),
        }));

    s.ensure_dir(s.vendor_dir.join("composer"));
    s.ensure_dir(s.working_dir.join("src"));
    s.ensure_dir(s.working_dir.join("composersrc"));
    s.put(
        s.working_dir.join("composersrc/foo.php"),
        BStr::new("<?php class ClassMapFoo {}"),
    );

    let composer_out = s.vendor_dir.join("composer");
    dump(&mut s, package.into(), false, BStr::new("_3")).unwrap();
    assert_autoload_files(BStr::new("main2"), &composer_out, BStr::new("namespaces"));
    assert_autoload_files(BStr::new("psr4_2"), &composer_out, BStr::new("psr4"));
    assert_autoload_files(BStr::new("classmap2"), &composer_out, BStr::new("classmap"));
}

#[test]
#[serial]
fn test_root_package_autoloading_with_target_dir() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));
    package.set_autoload(autoload(vec![
        (
            BStr::new("psr-0"),
            str_map(&[
                (BStr::new("Main\\Foo"), pstr(BStr::new(""))),
                (BStr::new("Main\\Bar"), pstr(BStr::new(""))),
            ]),
        ),
        (
            BStr::new("classmap"),
            str_list(&[BStr::new("Main/Foo/src"), BStr::new("lib")]),
        ),
        (
            BStr::new("files"),
            str_list(&[BStr::new("foo.php"), BStr::new("Main/Foo/bar.php")]),
        ),
    ]));
    package.__set_target_dir(Some(BString::from("Main/Foo/")));

    s.ensure_dir(s.vendor_dir.join("a"));
    s.ensure_dir(s.working_dir.join("src"));
    s.ensure_dir(s.working_dir.join("lib"));
    s.put(
        s.working_dir.join("src/rootfoo.php"),
        BStr::new("<?php class ClassMapFoo {}"),
    );
    s.put(
        s.working_dir.join("lib/rootbar.php"),
        BStr::new("<?php class ClassMapBar {}"),
    );
    s.put(
        s.working_dir.join("foo.php"),
        BStr::new("<?php class FilesFoo {}"),
    );
    s.put(
        s.working_dir.join("bar.php"),
        BStr::new("<?php class FilesBar {}"),
    );

    let vendor = s.vendor_dir.clone();
    let composer_out = vendor.join("composer");
    dump(&mut s, package.into(), false, BStr::new("TargetDir")).unwrap();

    let fx = fixtures_dir();
    assert_file_content_equals(
        fx.join("autoload_target_dir.php"),
        vendor.join("autoload.php"),
    );
    assert_file_content_equals(
        fx.join("autoload_real_target_dir.php"),
        composer_out.join("autoload_real.php"),
    );
    assert_file_content_equals(
        fx.join("autoload_static_target_dir.php"),
        composer_out.join("autoload_static.php"),
    );
    assert_file_content_equals(
        fx.join("autoload_files_target_dir.php"),
        composer_out.join("autoload_files.php"),
    );
    assert_autoload_files(BStr::new("classmap6"), &composer_out, BStr::new("classmap"));
}

#[test]
#[serial]
fn test_duplicate_files_warning() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));
    package.set_autoload(autoload(vec![(
        BStr::new("files"),
        str_list(&[
            BStr::new("foo.php"),
            BStr::new("bar.php"),
            BStr::new("./foo.php"),
            BStr::new("././foo.php"),
        ]),
    )]));

    s.ensure_dir(s.vendor_dir.join("a"));
    s.ensure_dir(s.working_dir.join("src"));
    s.ensure_dir(s.working_dir.join("lib"));
    s.put(
        s.working_dir.join("foo.php"),
        BStr::new("<?php class FilesFoo {}"),
    );
    s.put(
        s.working_dir.join("bar.php"),
        BStr::new("<?php class FilesBar {}"),
    );

    let vendor = s.vendor_dir.clone();
    let composer_out = vendor.join("composer");
    dump(&mut s, package.into(), false, BStr::new("FilesWarning")).unwrap();

    assert_file_content_equals(
        fixtures_dir().join("autoload_files_duplicates.php"),
        composer_out.join("autoload_files.php"),
    );
    let expected = "<warning>The following \"files\" autoload rules are included multiple times, this may cause issues and should be resolved:</warning>\n<warning> - $baseDir . '/foo.php'</warning>\n";
    assert_eq!(expected, s.io.borrow().get_output());
}

#[test]
#[serial]
fn test_vendors_autoloading() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));
    package
        .set_requires(
            requires(vec![
                (
                    BStr::new("a/a"),
                    link(BStr::new("a"), BStr::new("a/a"), match_all(), None),
                ),
                (
                    BStr::new("b/b"),
                    link(BStr::new("a"), BStr::new("b/b"), match_all(), None),
                ),
            ])
            .into_iter()
            .collect::<indexmap::IndexMap<_, _>>(),
        )
        .unwrap();

    let a = new_pkg(BStr::new("a/a"));
    let b = new_pkg(BStr::new("b/b"));
    let c = AliasPackageHandle::new(b.clone(), BString::from("1.2"), BString::from("1.2")).unwrap();
    a.__set_autoload(autoload(vec![(
        BStr::new("psr-0"),
        str_map(&[
            (BStr::new("A"), pstr(BStr::new("src/"))),
            (BStr::new("A\\B"), pstr(BStr::new("lib/"))),
        ]),
    )]));
    b.__set_autoload(autoload(vec![(
        BStr::new("psr-0"),
        str_map(&[(BStr::new("B\\Sub\\Name"), pstr(BStr::new("src/")))]),
    )]));

    s.set_canonical_packages(vec![a.into(), b.into(), c.into()]);

    s.ensure_dir(s.vendor_dir.join("composer"));
    s.ensure_dir(s.vendor_dir.join("a/a/src"));
    s.ensure_dir(s.vendor_dir.join("a/a/lib"));
    s.ensure_dir(s.vendor_dir.join("b/b/src"));

    let composer_out = s.vendor_dir.join("composer");
    dump(&mut s, package.into(), false, BStr::new("_5")).unwrap();
    assert_autoload_files(BStr::new("vendors"), &composer_out, BStr::new("namespaces"));
    assert!(std::path::Path::new(&composer_out.join("autoload_classmap.php")).exists());
}

#[test]
#[serial]
fn test_vendors_autoloading_with_metapackages() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));
    package
        .set_requires(
            requires(vec![(
                BStr::new("a/a"),
                link(BStr::new("a"), BStr::new("a/a"), match_all(), None),
            )])
            .into_iter()
            .collect::<indexmap::IndexMap<_, _>>(),
        )
        .unwrap();

    let a = new_pkg(BStr::new("a/a"));
    let b = new_pkg(BStr::new("b/b"));
    let c = AliasPackageHandle::new(b.clone(), BString::from("1.2"), BString::from("1.2")).unwrap();
    a.__set_autoload(autoload(vec![(
        BStr::new("psr-0"),
        str_map(&[
            (BStr::new("A"), pstr(BStr::new("src/"))),
            (BStr::new("A\\B"), pstr(BStr::new("lib/"))),
        ]),
    )]));
    b.__set_autoload(autoload(vec![(
        BStr::new("psr-0"),
        str_map(&[(BStr::new("B\\Sub\\Name"), pstr(BStr::new("src/")))]),
    )]));
    a.__set_type(BString::from("metapackage"));
    a.__set_requires(
        requires(vec![(
            BStr::new("b/b"),
            link(BStr::new("a/a"), BStr::new("b/b"), match_all(), None),
        )])
        .into_iter()
        .collect::<indexmap::IndexMap<_, _>>(),
    );

    s.set_canonical_packages(vec![a.into(), b.into(), c.into()]);

    s.ensure_dir(s.vendor_dir.join("composer"));
    s.ensure_dir(s.vendor_dir.join("b/b/src"));
    s.ensure_dir(s.vendor_dir.join("a/a/src"));
    s.ensure_dir(s.vendor_dir.join("a/a/lib"));

    let composer_out = s.vendor_dir.join("composer");
    dump(&mut s, package.into(), false, BStr::new("_5")).unwrap();
    assert_autoload_files(
        BStr::new("vendors_meta"),
        &composer_out,
        BStr::new("namespaces"),
    );
    assert!(std::path::Path::new(&composer_out.join("autoload_classmap.php")).exists());
}

#[test]
#[serial]
fn test_non_dev_autoload_exclusion_with_recursion() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));
    package
        .set_requires(
            requires(vec![(
                BStr::new("a/a"),
                link(BStr::new("a"), BStr::new("a/a"), match_all(), None),
            )])
            .into_iter()
            .collect::<indexmap::IndexMap<_, _>>(),
        )
        .unwrap();

    let a = new_pkg(BStr::new("a/a"));
    let b = new_pkg(BStr::new("b/b"));
    a.__set_autoload(autoload(vec![(
        BStr::new("psr-0"),
        str_map(&[
            (BStr::new("A"), pstr(BStr::new("src/"))),
            (BStr::new("A\\B"), pstr(BStr::new("lib/"))),
        ]),
    )]));
    a.__set_requires(
        requires(vec![(
            BStr::new("b/b"),
            link(BStr::new("a/a"), BStr::new("b/b"), match_all(), None),
        )])
        .into_iter()
        .collect::<indexmap::IndexMap<_, _>>(),
    );
    b.__set_autoload(autoload(vec![(
        BStr::new("psr-0"),
        str_map(&[(BStr::new("B\\Sub\\Name"), pstr(BStr::new("src/")))]),
    )]));
    b.__set_requires(
        requires(vec![(
            BStr::new("a/a"),
            link(BStr::new("b/b"), BStr::new("a/a"), match_all(), None),
        )])
        .into_iter()
        .collect::<indexmap::IndexMap<_, _>>(),
    );

    s.set_canonical_packages(vec![a.into(), b.into()]);

    s.ensure_dir(s.vendor_dir.join("composer"));
    s.ensure_dir(s.vendor_dir.join("a/a/src"));
    s.ensure_dir(s.vendor_dir.join("a/a/lib"));
    s.ensure_dir(s.vendor_dir.join("b/b/src"));

    let composer_out = s.vendor_dir.join("composer");
    dump(&mut s, package.into(), false, BStr::new("_5")).unwrap();
    assert_autoload_files(BStr::new("vendors"), &composer_out, BStr::new("namespaces"));
    assert!(std::path::Path::new(&composer_out.join("autoload_classmap.php")).exists());
}

#[test]
#[serial]
fn test_non_dev_autoload_should_include_replaced_packages() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));
    package
        .set_requires(
            requires(vec![(
                BStr::new("a/a"),
                link(BStr::new("a"), BStr::new("a/a"), match_all(), None),
            )])
            .into_iter()
            .collect::<indexmap::IndexMap<_, _>>(),
        )
        .unwrap();

    let a = new_pkg(BStr::new("a/a"));
    let b = new_pkg(BStr::new("b/b"));
    a.__set_requires(
        requires(vec![(
            BStr::new("b/c"),
            link(BStr::new("a/a"), BStr::new("b/c"), match_all(), None),
        )])
        .into_iter()
        .collect::<indexmap::IndexMap<_, _>>(),
    );
    b.__set_autoload(autoload(vec![(
        BStr::new("psr-4"),
        str_map(&[(BStr::new("B\\"), pstr(BStr::new("src/")))]),
    )]));
    b.__set_replaces(
        requires(vec![(
            BStr::new("b/c"),
            link(
                BStr::new("b/b"),
                BStr::new("b/c"),
                constraint(BStr::new("=="), BStr::new("1.0")),
                Some(Link::TYPE_REPLACE),
            ),
        )])
        .into_iter()
        .collect::<indexmap::IndexMap<_, _>>(),
    );

    s.set_canonical_packages(vec![a.into(), b.into()]);

    s.ensure_dir(s.vendor_dir.join("b/b/src/C"));
    s.put(
        s.vendor_dir.join("b/b/src/C/C.php"),
        BStr::new("<?php namespace B\\C; class C {}"),
    );

    let vendor = s.vendor_dir.clone();
    let class_map = dump(&mut s, package.into(), true, BStr::new("_5")).unwrap();

    let mut expected: IndexMap<BString, BString> = IndexMap::new();
    expected.insert(
        BString::from("B\\C\\C"),
        bstr::BString::from(path_to_bytes(&vendor.join("b/b/src/C/C.php"))),
    );
    expected.insert(
        BString::from("Composer\\InstalledVersions"),
        bstr::BString::from(path_to_bytes(
            &vendor.join("composer/InstalledVersions.php"),
        )),
    );
    assert_eq!(&expected, class_map.get_map());
}

#[test]
#[serial]
fn test_non_dev_autoload_exclusion_with_recursion_replace() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));
    package
        .set_requires(
            requires(vec![(
                BStr::new("a/a"),
                link(BStr::new("a"), BStr::new("a/a"), match_all(), None),
            )])
            .into_iter()
            .collect::<indexmap::IndexMap<_, _>>(),
        )
        .unwrap();

    let a = new_pkg(BStr::new("a/a"));
    let b = new_pkg(BStr::new("b/b"));
    a.__set_autoload(autoload(vec![(
        BStr::new("psr-0"),
        str_map(&[
            (BStr::new("A"), pstr(BStr::new("src/"))),
            (BStr::new("A\\B"), pstr(BStr::new("lib/"))),
        ]),
    )]));
    a.__set_requires(
        requires(vec![(
            BStr::new("c/c"),
            link(BStr::new("a/a"), BStr::new("c/c"), match_all(), None),
        )])
        .into_iter()
        .collect::<indexmap::IndexMap<_, _>>(),
    );
    b.__set_autoload(autoload(vec![(
        BStr::new("psr-0"),
        str_map(&[(BStr::new("B\\Sub\\Name"), pstr(BStr::new("src/")))]),
    )]));
    b.__set_replaces(
        requires(vec![(
            BStr::new("c/c"),
            link(BStr::new("b/b"), BStr::new("c/c"), match_all(), None),
        )])
        .into_iter()
        .collect::<indexmap::IndexMap<_, _>>(),
    );

    s.set_canonical_packages(vec![a.into(), b.into()]);

    s.ensure_dir(s.vendor_dir.join("composer"));
    s.ensure_dir(s.vendor_dir.join("a/a/src"));
    s.ensure_dir(s.vendor_dir.join("a/a/lib"));
    s.ensure_dir(s.vendor_dir.join("b/b/src"));

    let composer_out = s.vendor_dir.join("composer");
    dump(&mut s, package.into(), false, BStr::new("_5")).unwrap();
    assert_autoload_files(BStr::new("vendors"), &composer_out, BStr::new("namespaces"));
    assert!(std::path::Path::new(&composer_out.join("autoload_classmap.php")).exists());
}

#[test]
#[serial]
fn test_non_dev_autoload_replaces_nested_requirements() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));
    package
        .set_requires(
            requires(vec![(
                BStr::new("a/a"),
                link(BStr::new("a"), BStr::new("a/a"), match_all(), None),
            )])
            .into_iter()
            .collect::<indexmap::IndexMap<_, _>>(),
        )
        .unwrap();

    let a = new_pkg(BStr::new("a/a"));
    let b = new_pkg(BStr::new("b/b"));
    let c = new_pkg(BStr::new("c/c"));
    let d = new_pkg(BStr::new("d/d"));
    let e = new_pkg(BStr::new("e/e"));
    a.__set_autoload(autoload(vec![(
        BStr::new("classmap"),
        str_list(&[BStr::new("src/A.php")]),
    )]));
    a.__set_requires(
        requires(vec![(
            BStr::new("b/b"),
            link(BStr::new("a/a"), BStr::new("b/b"), match_all(), None),
        )])
        .into_iter()
        .collect::<indexmap::IndexMap<_, _>>(),
    );
    b.__set_autoload(autoload(vec![(
        BStr::new("classmap"),
        str_list(&[BStr::new("src/B.php")]),
    )]));
    b.__set_requires(
        requires(vec![(
            BStr::new("e/e"),
            link(BStr::new("b/b"), BStr::new("e/e"), match_all(), None),
        )])
        .into_iter()
        .collect::<indexmap::IndexMap<_, _>>(),
    );
    c.__set_autoload(autoload(vec![(
        BStr::new("classmap"),
        str_list(&[BStr::new("src/C.php")]),
    )]));
    c.__set_replaces(
        requires(vec![(
            BStr::new("b/b"),
            link(BStr::new("c/c"), BStr::new("b/b"), match_all(), None),
        )])
        .into_iter()
        .collect::<indexmap::IndexMap<_, _>>(),
    );
    c.__set_requires(
        requires(vec![(
            BStr::new("d/d"),
            link(BStr::new("c/c"), BStr::new("d/d"), match_all(), None),
        )])
        .into_iter()
        .collect::<indexmap::IndexMap<_, _>>(),
    );
    d.__set_autoload(autoload(vec![(
        BStr::new("classmap"),
        str_list(&[BStr::new("src/D.php")]),
    )]));
    e.__set_autoload(autoload(vec![(
        BStr::new("classmap"),
        str_list(&[BStr::new("src/E.php")]),
    )]));

    s.set_canonical_packages(vec![a.into(), b.into(), c.into(), d.into(), e.into()]);

    for (name, file, class) in [
        ("a/a", "A", "A"),
        ("b/b", "B", "B"),
        ("c/c", "C", "C"),
        ("d/d", "D", "D"),
        ("e/e", "E", "E"),
    ] {
        s.ensure_dir(s.vendor_dir.join(name).join("src"));
        s.put(
            s.vendor_dir
                .join(name)
                .join("src")
                .join(path_from_bytes(&bconcat!(file, ".php"))),
            bconcat!("<?php class ", class, " {}").as_bstr(),
        );
    }

    let composer_out = s.vendor_dir.join("composer");
    dump(&mut s, package.into(), false, BStr::new("_5")).unwrap();
    assert_autoload_files(BStr::new("classmap9"), &composer_out, BStr::new("classmap"));
}

#[test]
#[serial]
fn test_phar_autoload() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));
    package
        .set_requires(
            requires(vec![(
                BStr::new("a/a"),
                link(BStr::new("a"), BStr::new("a/a"), match_all(), None),
            )])
            .into_iter()
            .collect::<indexmap::IndexMap<_, _>>(),
        )
        .unwrap();
    package.set_autoload(autoload(vec![
        (
            BStr::new("psr-0"),
            str_map(&[
                (BStr::new("Foo"), pstr(BStr::new("foo.phar"))),
                (BStr::new("Bar"), pstr(BStr::new("dir/bar.phar/src"))),
            ]),
        ),
        (
            BStr::new("psr-4"),
            str_map(&[
                (BStr::new("Baz\\"), pstr(BStr::new("baz.phar"))),
                (BStr::new("Qux\\"), pstr(BStr::new("dir/qux.phar/src"))),
            ]),
        ),
    ]));

    let vendor_package = new_pkg(BStr::new("a/a"));
    vendor_package.__set_autoload(autoload(vec![
        (
            BStr::new("psr-0"),
            str_map(&[
                (BStr::new("Lorem"), pstr(BStr::new("lorem.phar"))),
                (BStr::new("Ipsum"), pstr(BStr::new("dir/ipsum.phar/src"))),
            ]),
        ),
        (
            BStr::new("psr-4"),
            str_map(&[
                (BStr::new("Dolor\\"), pstr(BStr::new("dolor.phar"))),
                (BStr::new("Sit\\"), pstr(BStr::new("dir/sit.phar/src"))),
            ]),
        ),
    ]));

    s.set_canonical_packages(vec![vendor_package.into()]);

    let composer_out = s.vendor_dir.join("composer");
    dump(&mut s, package.into(), true, BStr::new("Phar")).unwrap();
    assert_autoload_files(BStr::new("phar"), &composer_out, BStr::new("namespaces"));
    assert_autoload_files(BStr::new("phar_psr4"), &composer_out, BStr::new("psr4"));
    assert_autoload_files(BStr::new("phar_static"), &composer_out, BStr::new("static"));
}

#[test]
#[serial]
fn test_psr_to_class_map_ignores_non_existing_dir() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));
    package.set_autoload(autoload(vec![
        (
            BStr::new("psr-0"),
            str_map(&[(
                BStr::new("Prefix"),
                pstr(BStr::new("foo/bar/non/existing/")),
            )]),
        ),
        (
            BStr::new("psr-4"),
            str_map(&[(
                BStr::new("Prefix\\"),
                pstr(BStr::new("foo/bar/non/existing2/")),
            )]),
        ),
    ]));

    let vendor = s.vendor_dir.clone();
    let composer_out = vendor.join("composer");
    let class_map = dump(&mut s, package.into(), true, BStr::new("_8")).unwrap();
    assert!(std::path::Path::new(&composer_out.join("autoload_classmap.php")).exists());

    let mut expected: IndexMap<BString, BString> = IndexMap::new();
    expected.insert(
        BString::from("Composer\\InstalledVersions"),
        bstr::BString::from(path_to_bytes(
            &vendor.join("composer/InstalledVersions.php"),
        )),
    );
    assert_eq!(&expected, class_map.get_map());
}

#[test]
#[serial]
fn test_psr_to_class_map_ignores_non_psr_classes() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));
    package.set_autoload(autoload(vec![
        (
            BStr::new("psr-0"),
            str_map(&[(BStr::new("psr0_"), pstr(BStr::new("psr0/")))]),
        ),
        (
            BStr::new("psr-4"),
            str_map(&[(BStr::new("psr4\\"), pstr(BStr::new("psr4/")))]),
        ),
    ]));

    s.ensure_dir(s.working_dir.join("psr0/psr0"));
    s.ensure_dir(s.working_dir.join("psr4"));
    s.put(
        s.working_dir.join("psr0/psr0/match.php"),
        BStr::new("<?php class psr0_match {}"),
    );
    s.put(
        s.working_dir.join("psr0/psr0/badfile.php"),
        BStr::new("<?php class psr0_badclass {}"),
    );
    s.put(
        s.working_dir.join("psr4/match.php"),
        BStr::new("<?php namespace psr4; class match {}"),
    );
    s.put(
        s.working_dir.join("psr4/badfile.php"),
        BStr::new("<?php namespace psr4; class badclass {}"),
    );

    let vendor = s.vendor_dir.clone();
    let composer_out = vendor.join("composer");
    dump(&mut s, package.into(), true, BStr::new("_1")).unwrap();
    assert!(std::path::Path::new(&composer_out.join("autoload_classmap.php")).exists());

    let expected = BString::from(
        "<?php\n\n// autoload_classmap.php @generated by Composer\n\n$vendorDir = dirname(__DIR__);\n$baseDir = dirname($vendorDir);\n\nreturn array(\n    'Composer\\\\InstalledVersions' => $vendorDir . '/composer/InstalledVersions.php',\n    'psr0_match' => $baseDir . '/psr0/psr0/match.php',\n    'psr4\\\\match' => $baseDir . '/psr4/match.php',\n);\n",
    );
    let actual =
        bstr::BString::from(std::fs::read(composer_out.join("autoload_classmap.php")).unwrap());
    assert_eq!(expected, actual);
}

#[test]
#[serial]
fn test_vendors_class_map_autoloading() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));
    package
        .set_requires(
            requires(vec![
                (
                    BStr::new("a/a"),
                    link(BStr::new("a"), BStr::new("a/a"), match_all(), None),
                ),
                (
                    BStr::new("b/b"),
                    link(BStr::new("a"), BStr::new("b/b"), match_all(), None),
                ),
            ])
            .into_iter()
            .collect::<indexmap::IndexMap<_, _>>(),
        )
        .unwrap();

    let a = new_pkg(BStr::new("a/a"));
    let b = new_pkg(BStr::new("b/b"));
    a.__set_autoload(autoload(vec![(
        BStr::new("classmap"),
        str_list(&[BStr::new("src/")]),
    )]));
    b.__set_autoload(autoload(vec![(
        BStr::new("classmap"),
        str_list(&[BStr::new("src/"), BStr::new("lib/")]),
    )]));

    s.set_canonical_packages(vec![a.into(), b.into()]);

    s.ensure_dir(s.vendor_dir.join("composer"));
    s.ensure_dir(s.vendor_dir.join("a/a/src"));
    s.ensure_dir(s.vendor_dir.join("b/b/src"));
    s.ensure_dir(s.vendor_dir.join("b/b/lib"));
    s.put(
        s.vendor_dir.join("a/a/src/a.php"),
        BStr::new("<?php class ClassMapFoo {}"),
    );
    s.put(
        s.vendor_dir.join("b/b/src/b.php"),
        BStr::new("<?php class ClassMapBar {}"),
    );
    s.put(
        s.vendor_dir.join("b/b/lib/c.php"),
        BStr::new("<?php class ClassMapBaz {}"),
    );

    let vendor = s.vendor_dir.clone();
    let composer_out = vendor.join("composer");
    let class_map = dump(&mut s, package.into(), false, BStr::new("_6")).unwrap();

    let mut expected: IndexMap<BString, BString> = IndexMap::new();
    expected.insert(
        BString::from("ClassMapBar"),
        bstr::BString::from(path_to_bytes(&vendor.join("b/b/src/b.php"))),
    );
    expected.insert(
        BString::from("ClassMapBaz"),
        bstr::BString::from(path_to_bytes(&vendor.join("b/b/lib/c.php"))),
    );
    expected.insert(
        BString::from("ClassMapFoo"),
        bstr::BString::from(path_to_bytes(&vendor.join("a/a/src/a.php"))),
    );
    expected.insert(
        BString::from("Composer\\InstalledVersions"),
        bstr::BString::from(path_to_bytes(
            &vendor.join("composer/InstalledVersions.php"),
        )),
    );
    assert_eq!(&expected, class_map.get_map());
    assert_autoload_files(BStr::new("classmap4"), &composer_out, BStr::new("classmap"));
}

#[test]
#[serial]
fn test_vendors_class_map_autoloading_with_target_dir() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));
    package
        .set_requires(
            requires(vec![
                (
                    BStr::new("a/a"),
                    link(BStr::new("a"), BStr::new("a/a"), match_all(), None),
                ),
                (
                    BStr::new("b/b"),
                    link(BStr::new("a"), BStr::new("b/b"), match_all(), None),
                ),
            ])
            .into_iter()
            .collect::<indexmap::IndexMap<_, _>>(),
        )
        .unwrap();

    let a = new_pkg(BStr::new("a/a"));
    let b = new_pkg(BStr::new("b/b"));
    a.__set_autoload(autoload(vec![(
        BStr::new("classmap"),
        str_list(&[BStr::new("target/src/"), BStr::new("lib/")]),
    )]));
    a.__set_target_dir(Some(BString::from("target")));
    b.__set_autoload(autoload(vec![(
        BStr::new("classmap"),
        str_list(&[BStr::new("src/")]),
    )]));

    s.set_canonical_packages(vec![a.into(), b.into()]);

    s.ensure_dir(s.vendor_dir.join("composer"));
    s.ensure_dir(s.vendor_dir.join("a/a/target/src"));
    s.ensure_dir(s.vendor_dir.join("a/a/target/lib"));
    s.ensure_dir(s.vendor_dir.join("b/b/src"));
    s.put(
        s.vendor_dir.join("a/a/target/src/a.php"),
        BStr::new("<?php class ClassMapFoo {}"),
    );
    s.put(
        s.vendor_dir.join("a/a/target/lib/b.php"),
        BStr::new("<?php class ClassMapBar {}"),
    );
    s.put(
        s.vendor_dir.join("b/b/src/c.php"),
        BStr::new("<?php class ClassMapBaz {}"),
    );

    let vendor = s.vendor_dir.clone();
    let composer_out = vendor.join("composer");
    let class_map = dump(&mut s, package.into(), false, BStr::new("_6")).unwrap();
    assert!(std::path::Path::new(&composer_out.join("autoload_classmap.php")).exists());

    let mut expected: IndexMap<BString, BString> = IndexMap::new();
    expected.insert(
        BString::from("ClassMapBar"),
        bstr::BString::from(path_to_bytes(&vendor.join("a/a/target/lib/b.php"))),
    );
    expected.insert(
        BString::from("ClassMapBaz"),
        bstr::BString::from(path_to_bytes(&vendor.join("b/b/src/c.php"))),
    );
    expected.insert(
        BString::from("ClassMapFoo"),
        bstr::BString::from(path_to_bytes(&vendor.join("a/a/target/src/a.php"))),
    );
    expected.insert(
        BString::from("Composer\\InstalledVersions"),
        bstr::BString::from(path_to_bytes(
            &vendor.join("composer/InstalledVersions.php"),
        )),
    );
    assert_eq!(&expected, class_map.get_map());
}

#[test]
#[serial]
#[ignore = "the `classmap => ['./']` rule yields a scanned path containing `/./` (e.g. c/c/./foo/test.php) that the ClassMapGenerator port does not collapse the way PHP's normalizePath does; needs production path-normalization fix"]
fn test_class_map_autoloading_empty_dir_and_exact_file() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));
    package
        .set_requires(
            requires(vec![
                (
                    BStr::new("a/a"),
                    link(BStr::new("a"), BStr::new("a/a"), match_all(), None),
                ),
                (
                    BStr::new("b/b"),
                    link(BStr::new("a"), BStr::new("b/b"), match_all(), None),
                ),
                (
                    BStr::new("c/c"),
                    link(BStr::new("a"), BStr::new("c/c"), match_all(), None),
                ),
            ])
            .into_iter()
            .collect::<indexmap::IndexMap<_, _>>(),
        )
        .unwrap();

    let a = new_pkg(BStr::new("a/a"));
    let b = new_pkg(BStr::new("b/b"));
    let c = new_pkg(BStr::new("c/c"));
    a.__set_autoload(autoload(vec![(
        BStr::new("classmap"),
        str_list(&[BStr::new("")]),
    )]));
    b.__set_autoload(autoload(vec![(
        BStr::new("classmap"),
        str_list(&[BStr::new("test.php")]),
    )]));
    c.__set_autoload(autoload(vec![(
        BStr::new("classmap"),
        str_list(&[BStr::new("./")]),
    )]));

    s.set_canonical_packages(vec![a.into(), b.into(), c.into()]);

    s.ensure_dir(s.vendor_dir.join("composer"));
    s.ensure_dir(s.vendor_dir.join("a/a/src"));
    s.ensure_dir(s.vendor_dir.join("b/b"));
    s.ensure_dir(s.vendor_dir.join("c/c/foo"));
    s.put(
        s.vendor_dir.join("a/a/src/a.php"),
        BStr::new("<?php class ClassMapFoo {}"),
    );
    s.put(
        s.vendor_dir.join("b/b/test.php"),
        BStr::new("<?php class ClassMapBar {}"),
    );
    s.put(
        s.vendor_dir.join("c/c/foo/test.php"),
        BStr::new("<?php class ClassMapBaz {}"),
    );

    let vendor = s.vendor_dir.clone();
    let composer_out = vendor.join("composer");
    let class_map = dump(&mut s, package.into(), false, BStr::new("_7")).unwrap();
    assert!(std::path::Path::new(&composer_out.join("autoload_classmap.php")).exists());

    let mut expected: IndexMap<BString, BString> = IndexMap::new();
    expected.insert(
        BString::from("ClassMapBar"),
        bstr::BString::from(path_to_bytes(&vendor.join("b/b/test.php"))),
    );
    expected.insert(
        BString::from("ClassMapBaz"),
        bstr::BString::from(path_to_bytes(&vendor.join("c/c/foo/test.php"))),
    );
    expected.insert(
        BString::from("ClassMapFoo"),
        bstr::BString::from(path_to_bytes(&vendor.join("a/a/src/a.php"))),
    );
    expected.insert(
        BString::from("Composer\\InstalledVersions"),
        bstr::BString::from(path_to_bytes(
            &vendor.join("composer/InstalledVersions.php"),
        )),
    );
    assert_eq!(&expected, class_map.get_map());
    assert_autoload_files(BStr::new("classmap5"), &composer_out, BStr::new("classmap"));

    let real = bstr::BString::from(std::fs::read(composer_out.join("autoload_real.php")).unwrap());
    assert!(!real.contains_str("$loader->setClassMapAuthoritative(true);"));
    assert!(!real.contains_str("$loader->setApcuPrefix("));
}

#[test]
#[serial]
fn test_class_map_autoloading_authoritative_and_apcu() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));
    package
        .set_requires(
            requires(vec![
                (
                    BStr::new("a/a"),
                    link(BStr::new("a"), BStr::new("a/a"), match_all(), None),
                ),
                (
                    BStr::new("b/b"),
                    link(BStr::new("a"), BStr::new("b/b"), match_all(), None),
                ),
                (
                    BStr::new("c/c"),
                    link(BStr::new("a"), BStr::new("c/c"), match_all(), None),
                ),
            ])
            .into_iter()
            .collect::<indexmap::IndexMap<_, _>>(),
        )
        .unwrap();

    let a = new_pkg(BStr::new("a/a"));
    let b = new_pkg(BStr::new("b/b"));
    let c = new_pkg(BStr::new("c/c"));
    a.__set_autoload(autoload(vec![(
        BStr::new("psr-4"),
        str_map(&[(BStr::new(""), pstr(BStr::new("src/")))]),
    )]));
    b.__set_autoload(autoload(vec![(
        BStr::new("psr-4"),
        str_map(&[(BStr::new(""), pstr(BStr::new("./")))]),
    )]));
    c.__set_autoload(autoload(vec![(
        BStr::new("psr-4"),
        str_map(&[(BStr::new(""), pstr(BStr::new("foo/")))]),
    )]));

    s.set_canonical_packages(vec![a.into(), b.into(), c.into()]);

    s.ensure_dir(s.vendor_dir.join("composer"));
    s.ensure_dir(s.vendor_dir.join("a/a/src"));
    s.ensure_dir(s.vendor_dir.join("b/b"));
    s.ensure_dir(s.vendor_dir.join("c/c/foo"));
    s.put(
        s.vendor_dir.join("a/a/src/ClassMapFoo.php"),
        BStr::new("<?php class ClassMapFoo {}"),
    );
    s.put(
        s.vendor_dir.join("b/b/ClassMapBar.php"),
        BStr::new("<?php class ClassMapBar {}"),
    );
    s.put(
        s.vendor_dir.join("c/c/foo/ClassMapBaz.php"),
        BStr::new("<?php class ClassMapBaz {}"),
    );

    s.generator.set_class_map_authoritative(true);
    s.generator.set_apcu(true, None);
    let vendor = s.vendor_dir.clone();
    let composer_out = vendor.join("composer");
    let class_map = dump(&mut s, package.into(), false, BStr::new("_7")).unwrap();
    assert!(std::path::Path::new(&composer_out.join("autoload_classmap.php")).exists());

    let mut expected: IndexMap<BString, BString> = IndexMap::new();
    expected.insert(
        BString::from("ClassMapBar"),
        bstr::BString::from(path_to_bytes(&vendor.join("b/b/ClassMapBar.php"))),
    );
    expected.insert(
        BString::from("ClassMapBaz"),
        bstr::BString::from(path_to_bytes(&vendor.join("c/c/foo/ClassMapBaz.php"))),
    );
    expected.insert(
        BString::from("ClassMapFoo"),
        bstr::BString::from(path_to_bytes(&vendor.join("a/a/src/ClassMapFoo.php"))),
    );
    expected.insert(
        BString::from("Composer\\InstalledVersions"),
        bstr::BString::from(path_to_bytes(
            &vendor.join("composer/InstalledVersions.php"),
        )),
    );
    assert_eq!(&expected, class_map.get_map());
    assert_autoload_files(BStr::new("classmap8"), &composer_out, BStr::new("classmap"));

    let real = bstr::BString::from(std::fs::read(composer_out.join("autoload_real.php")).unwrap());
    assert!(real.contains_str("$loader->setClassMapAuthoritative(true);"));
    assert!(real.contains_str("$loader->setApcuPrefix("));
}

#[test]
#[serial]
fn test_class_map_autoloading_authoritative_and_apcu_prefix() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));
    package
        .set_requires(
            requires(vec![
                (
                    BStr::new("a/a"),
                    link(BStr::new("a"), BStr::new("a/a"), match_all(), None),
                ),
                (
                    BStr::new("b/b"),
                    link(BStr::new("a"), BStr::new("b/b"), match_all(), None),
                ),
                (
                    BStr::new("c/c"),
                    link(BStr::new("a"), BStr::new("c/c"), match_all(), None),
                ),
            ])
            .into_iter()
            .collect::<indexmap::IndexMap<_, _>>(),
        )
        .unwrap();

    let a = new_pkg(BStr::new("a/a"));
    let b = new_pkg(BStr::new("b/b"));
    let c = new_pkg(BStr::new("c/c"));
    a.__set_autoload(autoload(vec![(
        BStr::new("psr-4"),
        str_map(&[(BStr::new(""), pstr(BStr::new("src/")))]),
    )]));
    b.__set_autoload(autoload(vec![(
        BStr::new("psr-4"),
        str_map(&[(BStr::new(""), pstr(BStr::new("./")))]),
    )]));
    c.__set_autoload(autoload(vec![(
        BStr::new("psr-4"),
        str_map(&[(BStr::new(""), pstr(BStr::new("foo/")))]),
    )]));

    s.set_canonical_packages(vec![a.into(), b.into(), c.into()]);

    s.ensure_dir(s.vendor_dir.join("composer"));
    s.ensure_dir(s.vendor_dir.join("a/a/src"));
    s.ensure_dir(s.vendor_dir.join("b/b"));
    s.ensure_dir(s.vendor_dir.join("c/c/foo"));
    s.put(
        s.vendor_dir.join("a/a/src/ClassMapFoo.php"),
        BStr::new("<?php class ClassMapFoo {}"),
    );
    s.put(
        s.vendor_dir.join("b/b/ClassMapBar.php"),
        BStr::new("<?php class ClassMapBar {}"),
    );
    s.put(
        s.vendor_dir.join("c/c/foo/ClassMapBaz.php"),
        BStr::new("<?php class ClassMapBaz {}"),
    );

    s.generator.set_class_map_authoritative(true);
    s.generator.set_apcu(true, Some("custom'Prefix".into()));
    let vendor = s.vendor_dir.clone();
    let composer_out = vendor.join("composer");
    let class_map = dump(&mut s, package.into(), false, BStr::new("_7")).unwrap();
    assert!(std::path::Path::new(&composer_out.join("autoload_classmap.php")).exists());

    let mut expected: IndexMap<BString, BString> = IndexMap::new();
    expected.insert(
        BString::from("ClassMapBar"),
        bstr::BString::from(path_to_bytes(&vendor.join("b/b/ClassMapBar.php"))),
    );
    expected.insert(
        BString::from("ClassMapBaz"),
        bstr::BString::from(path_to_bytes(&vendor.join("c/c/foo/ClassMapBaz.php"))),
    );
    expected.insert(
        BString::from("ClassMapFoo"),
        bstr::BString::from(path_to_bytes(&vendor.join("a/a/src/ClassMapFoo.php"))),
    );
    expected.insert(
        BString::from("Composer\\InstalledVersions"),
        bstr::BString::from(path_to_bytes(
            &vendor.join("composer/InstalledVersions.php"),
        )),
    );
    assert_eq!(&expected, class_map.get_map());
    assert_autoload_files(BStr::new("classmap8"), &composer_out, BStr::new("classmap"));

    let real = bstr::BString::from(std::fs::read(composer_out.join("autoload_real.php")).unwrap());
    assert!(real.contains_str("$loader->setClassMapAuthoritative(true);"));
    assert!(real.contains_str("$loader->setApcuPrefix('custom\\'Prefix');"));
}

#[test]
#[serial]
fn test_files_autoload_generation() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));
    package.set_autoload(autoload(vec![(
        BStr::new("files"),
        str_list(&[BStr::new("root.php")]),
    )]));
    package
        .set_requires(
            requires(vec![
                (
                    BStr::new("a/a"),
                    link(BStr::new("a"), BStr::new("a/a"), match_all(), None),
                ),
                (
                    BStr::new("b/b"),
                    link(BStr::new("a"), BStr::new("b/b"), match_all(), None),
                ),
                (
                    BStr::new("c/c"),
                    link(BStr::new("a"), BStr::new("c/c"), match_all(), None),
                ),
            ])
            .into_iter()
            .collect::<indexmap::IndexMap<_, _>>(),
        )
        .unwrap();

    let a = new_pkg(BStr::new("a/a"));
    let b = new_pkg(BStr::new("b/b"));
    let c = new_pkg(BStr::new("c/c"));
    a.__set_autoload(autoload(vec![(
        BStr::new("files"),
        str_list(&[BStr::new("test.php")]),
    )]));
    b.__set_autoload(autoload(vec![(
        BStr::new("files"),
        str_list(&[BStr::new("test2.php")]),
    )]));
    c.__set_autoload(autoload(vec![(
        BStr::new("files"),
        str_list(&[BStr::new("test3.php"), BStr::new("foo/bar/test4.php")]),
    )]));
    c.__set_target_dir(Some(BString::from("foo/bar")));

    s.set_canonical_packages(vec![a.into(), b.into(), c.into()]);

    s.ensure_dir(s.vendor_dir.join("a/a"));
    s.ensure_dir(s.vendor_dir.join("b/b"));
    s.ensure_dir(s.vendor_dir.join("c/c/foo/bar"));
    s.put(
        s.vendor_dir.join("a/a/test.php"),
        BStr::new("<?php function testFilesAutoloadGeneration1() {}"),
    );
    s.put(
        s.vendor_dir.join("b/b/test2.php"),
        BStr::new("<?php function testFilesAutoloadGeneration2() {}"),
    );
    s.put(
        s.vendor_dir.join("c/c/foo/bar/test3.php"),
        BStr::new("<?php function testFilesAutoloadGeneration3() {}"),
    );
    s.put(
        s.vendor_dir.join("c/c/foo/bar/test4.php"),
        BStr::new("<?php function testFilesAutoloadGeneration4() {}"),
    );
    s.put(
        s.working_dir.join("root.php"),
        BStr::new("<?php function testFilesAutoloadGenerationRoot() {}"),
    );

    let vendor = s.vendor_dir.clone();
    let composer_out = vendor.join("composer");
    dump(&mut s, package.into(), false, BStr::new("FilesAutoload")).unwrap();
    let fx = fixtures_dir();
    assert_file_content_equals(
        fx.join("autoload_functions.php"),
        vendor.join("autoload.php"),
    );
    assert_file_content_equals(
        fx.join("autoload_real_functions.php"),
        composer_out.join("autoload_real.php"),
    );
    assert_file_content_equals(
        fx.join("autoload_static_functions.php"),
        composer_out.join("autoload_static.php"),
    );
    assert_file_content_equals(
        fx.join("autoload_files_functions.php"),
        composer_out.join("autoload_files.php"),
    );
}

#[test]
#[serial]
fn test_override_vendors_autoloading() {
    let mut s = set_up();
    let working_dir = s.working_dir.clone();
    let root_package = new_root_pkg(BStr::new("root/z"));
    root_package.set_autoload(autoload(vec![
        (
            BStr::new("psr-0"),
            str_map(&[(
                BStr::new("A\\B"),
                PhpMixed::string(path_to_bytes(&working_dir.join("lib"))),
            )]),
        ),
        (
            BStr::new("classmap"),
            PhpMixed::List(vec![PhpMixed::string(path_to_bytes(
                &working_dir.join("src"),
            ))]),
        ),
    ]));
    root_package
        .set_requires(
            requires(vec![
                (
                    BStr::new("a/a"),
                    link(BStr::new("z"), BStr::new("a/a"), match_all(), None),
                ),
                (
                    BStr::new("b/b"),
                    link(BStr::new("z"), BStr::new("b/b"), match_all(), None),
                ),
            ])
            .into_iter()
            .collect::<indexmap::IndexMap<_, _>>(),
        )
        .unwrap();

    let a = new_pkg(BStr::new("a/a"));
    let b = new_pkg(BStr::new("b/b"));
    a.__set_autoload(autoload(vec![
        (
            BStr::new("psr-0"),
            str_map(&[
                (BStr::new("A"), pstr(BStr::new("src/"))),
                (BStr::new("A\\B"), pstr(BStr::new("lib/"))),
            ]),
        ),
        (BStr::new("classmap"), str_list(&[BStr::new("classmap")])),
    ]));
    b.__set_autoload(autoload(vec![(
        BStr::new("psr-0"),
        str_map(&[(BStr::new("B\\Sub\\Name"), pstr(BStr::new("src/")))]),
    )]));

    s.set_canonical_packages(vec![a.into(), b.into()]);

    s.ensure_dir(s.working_dir.join("lib/A/B"));
    s.ensure_dir(s.working_dir.join("src/"));
    s.ensure_dir(s.vendor_dir.join("composer"));
    s.ensure_dir(s.vendor_dir.join("a/a/classmap"));
    s.ensure_dir(s.vendor_dir.join("a/a/src"));
    s.ensure_dir(s.vendor_dir.join("a/a/lib/A/B"));
    s.ensure_dir(s.vendor_dir.join("b/b/src"));

    s.put(
        s.working_dir.join("lib/A/B/C.php"),
        BStr::new("<?php namespace A\\B; class C {}"),
    );
    s.put(
        s.working_dir.join("src/classes.php"),
        BStr::new("<?php namespace Foo; class Bar {}"),
    );
    s.put(
        s.vendor_dir.join("a/a/lib/A/B/C.php"),
        BStr::new("<?php namespace A\\B; class C {}"),
    );
    s.put(
        s.vendor_dir.join("a/a/classmap/classes.php"),
        BStr::new("<?php namespace Foo; class Bar {}"),
    );

    let composer_out = s.vendor_dir.join("composer");
    dump(&mut s, root_package.into(), true, BStr::new("_9")).unwrap();

    let expected_namespace = "<?php\n\n// autoload_namespaces.php @generated by Composer\n\n$vendorDir = dirname(__DIR__);\n$baseDir = dirname($vendorDir);\n\nreturn array(\n    'B\\\\Sub\\\\Name' => array($vendorDir . '/b/b/src'),\n    'A\\\\B' => array($baseDir . '/lib', $vendorDir . '/a/a/lib'),\n    'A' => array($vendorDir . '/a/a/src'),\n);\n";
    let expected_psr4 = "<?php\n\n// autoload_psr4.php @generated by Composer\n\n$vendorDir = dirname(__DIR__);\n$baseDir = dirname($vendorDir);\n\nreturn array(\n);\n";
    let expected_classmap = "<?php\n\n// autoload_classmap.php @generated by Composer\n\n$vendorDir = dirname(__DIR__);\n$baseDir = dirname($vendorDir);\n\nreturn array(\n    'A\\\\B\\\\C' => $baseDir . '/lib/A/B/C.php',\n    'Composer\\\\InstalledVersions' => $vendorDir . '/composer/InstalledVersions.php',\n    'Foo\\\\Bar' => $baseDir . '/src/classes.php',\n);\n";

    assert_eq!(
        expected_namespace,
        bstr::BString::from(std::fs::read(composer_out.join("autoload_namespaces.php")).unwrap())
    );
    assert_eq!(
        expected_psr4,
        bstr::BString::from(std::fs::read(composer_out.join("autoload_psr4.php")).unwrap())
    );
    assert_eq!(
        expected_classmap,
        bstr::BString::from(std::fs::read(composer_out.join("autoload_classmap.php")).unwrap())
    );
}

#[test]
#[serial]
fn test_include_path_file_generation() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));

    let a = new_pkg(BStr::new("a/a"));
    a.__set_include_paths(vec![BString::from("lib/")]);
    let b = new_pkg(BStr::new("b/b"));
    b.__set_include_paths(vec![BString::from("library")]);
    let c = new_pkg(BStr::new("c"));
    c.__set_include_paths(vec![BString::from("library")]);

    s.set_canonical_packages(vec![a.into(), b.into(), c.into()]);

    s.ensure_dir(s.vendor_dir.join("composer"));

    let composer_out = s.vendor_dir.join("composer");
    dump(&mut s, package.into(), false, BStr::new("_10")).unwrap();

    assert_file_content_equals(
        fixtures_dir().join("include_paths.php"),
        composer_out.join("include_paths.php"),
    );
}

#[test]
#[serial]
fn test_include_path_file_without_paths_is_skipped() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));
    let a = new_pkg(BStr::new("a/a"));
    s.set_canonical_packages(vec![a.into()]);

    s.ensure_dir(s.vendor_dir.join("composer"));

    let composer_out = s.vendor_dir.join("composer");
    dump(&mut s, package.into(), false, BStr::new("_12")).unwrap();

    assert!(!std::path::Path::new(&composer_out.join("include_paths.php")).exists());
}

#[test]
#[serial]
fn test_vendor_substring_path() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));
    package.set_autoload(autoload(vec![
        (
            BStr::new("psr-0"),
            str_map(&[(
                BStr::new("Foo"),
                pstr(BStr::new("composer-test-autoload-src/src")),
            )]),
        ),
        (
            BStr::new("psr-4"),
            str_map(&[(
                BStr::new("Acme\\Foo\\"),
                pstr(BStr::new("composer-test-autoload-src/src-psr4")),
            )]),
        ),
    ]));

    s.ensure_dir(s.vendor_dir.join("a"));

    let composer_out = s.vendor_dir.join("composer");
    dump(&mut s, package.into(), false, BStr::new("VendorSubstring")).unwrap();

    let expected_namespace = "<?php\n\n// autoload_namespaces.php @generated by Composer\n\n$vendorDir = dirname(__DIR__);\n$baseDir = dirname($vendorDir);\n\nreturn array(\n    'Foo' => array($baseDir . '/composer-test-autoload-src/src'),\n);\n";
    let expected_psr4 = "<?php\n\n// autoload_psr4.php @generated by Composer\n\n$vendorDir = dirname(__DIR__);\n$baseDir = dirname($vendorDir);\n\nreturn array(\n    'Acme\\\\Foo\\\\' => array($baseDir . '/composer-test-autoload-src/src-psr4'),\n);\n";

    assert_eq!(
        expected_namespace,
        bstr::BString::from(std::fs::read(composer_out.join("autoload_namespaces.php")).unwrap())
    );
    assert_eq!(
        expected_psr4,
        bstr::BString::from(std::fs::read(composer_out.join("autoload_psr4.php")).unwrap())
    );
}

#[test]
#[serial]
fn test_empty_paths() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));
    package.set_autoload(autoload(vec![
        (
            BStr::new("psr-0"),
            str_map(&[(BStr::new("Foo"), pstr(BStr::new("")))]),
        ),
        (
            BStr::new("psr-4"),
            str_map(&[(BStr::new("Acme\\Foo\\"), pstr(BStr::new("")))]),
        ),
        (BStr::new("classmap"), str_list(&[BStr::new("")])),
    ]));

    s.ensure_dir(s.working_dir.join("Foo"));
    s.put(
        s.working_dir.join("Foo/Bar.php"),
        BStr::new("<?php namespace Foo; class Bar {}"),
    );
    s.put(
        s.working_dir.join("class.php"),
        BStr::new("<?php namespace Classmap; class Foo {}"),
    );

    let composer_out = s.vendor_dir.join("composer");
    dump(&mut s, package.into(), true, BStr::new("_15")).unwrap();

    let expected_namespace = "<?php\n\n// autoload_namespaces.php @generated by Composer\n\n$vendorDir = dirname(__DIR__);\n$baseDir = dirname($vendorDir);\n\nreturn array(\n    'Foo' => array($baseDir . '/'),\n);\n";
    let expected_psr4 = "<?php\n\n// autoload_psr4.php @generated by Composer\n\n$vendorDir = dirname(__DIR__);\n$baseDir = dirname($vendorDir);\n\nreturn array(\n    'Acme\\\\Foo\\\\' => array($baseDir . '/'),\n);\n";
    let expected_classmap = "<?php\n\n// autoload_classmap.php @generated by Composer\n\n$vendorDir = dirname(__DIR__);\n$baseDir = dirname($vendorDir);\n\nreturn array(\n    'Classmap\\\\Foo' => $baseDir . '/class.php',\n    'Composer\\\\InstalledVersions' => $vendorDir . '/composer/InstalledVersions.php',\n    'Foo\\\\Bar' => $baseDir . '/Foo/Bar.php',\n);\n";

    assert_eq!(
        expected_namespace,
        bstr::BString::from(std::fs::read(composer_out.join("autoload_namespaces.php")).unwrap())
    );
    assert_eq!(
        expected_psr4,
        bstr::BString::from(std::fs::read(composer_out.join("autoload_psr4.php")).unwrap())
    );
    assert_eq!(
        expected_classmap,
        bstr::BString::from(std::fs::read(composer_out.join("autoload_classmap.php")).unwrap())
    );
}

#[test]
#[serial]
fn test_exclude_from_classmap() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));
    package.set_autoload(autoload(vec![
        (
            BStr::new("psr-0"),
            str_map(&[
                (BStr::new("Main"), pstr(BStr::new("src/"))),
                (
                    BStr::new("Lala"),
                    str_list(&[BStr::new("src/"), BStr::new("lib/")]),
                ),
            ]),
        ),
        (
            BStr::new("psr-4"),
            str_map(&[
                (BStr::new("Acme\\Fruit\\"), pstr(BStr::new("src-fruit/"))),
                (
                    BStr::new("Acme\\Cake\\"),
                    str_list(&[BStr::new("src-cake/"), BStr::new("lib-cake/")]),
                ),
            ]),
        ),
        (
            BStr::new("classmap"),
            str_list(&[BStr::new("composersrc/")]),
        ),
        (
            BStr::new("exclude-from-classmap"),
            str_list(&[
                BStr::new("/composersrc/foo/bar/"),
                BStr::new("/composersrc/excludedTests/"),
                BStr::new("/composersrc/ClassToExclude.php"),
                BStr::new("/composersrc/*/excluded/excsubpath"),
                BStr::new("**/excsubpath"),
                BStr::new("composers"),
                BStr::new("/src-ca/"),
            ]),
        ),
    ]));

    s.ensure_dir(s.working_dir.join("composer"));
    s.ensure_dir(s.working_dir.join("src/Lala/Test"));
    s.ensure_dir(s.working_dir.join("lib"));
    s.put(
        s.working_dir.join("src/Lala/ClassMapMain.php"),
        BStr::new("<?php namespace Lala; class ClassMapMain {}"),
    );
    s.put(
        s.working_dir.join("src/Lala/Test/ClassMapMainTest.php"),
        BStr::new("<?php namespace Lala\\Test; class ClassMapMainTest {}"),
    );

    s.ensure_dir(s.working_dir.join("src-fruit"));
    s.ensure_dir(s.working_dir.join("src-cake"));
    s.ensure_dir(s.working_dir.join("lib-cake"));
    s.put(
        s.working_dir.join("src-cake/ClassMapBar.php"),
        BStr::new("<?php namespace Acme\\Cake; class ClassMapBar {}"),
    );

    s.ensure_dir(s.working_dir.join("composersrc"));
    s.ensure_dir(s.working_dir.join("composersrc/tests"));
    s.put(
        s.working_dir.join("composersrc/foo.php"),
        BStr::new("<?php class ClassMapFoo {}"),
    );

    s.ensure_dir(s.working_dir.join("composersrc/excludedTests"));
    s.put(
        s.working_dir.join("composersrc/excludedTests/bar.php"),
        BStr::new("<?php class ClassExcludeMapFoo {}"),
    );
    s.put(
        s.working_dir.join("composersrc/ClassToExclude.php"),
        BStr::new("<?php class ClassClassToExclude {}"),
    );
    s.ensure_dir(s.working_dir.join("composersrc/long/excluded/excsubpath"));
    s.put(
        s.working_dir
            .join("composersrc/long/excluded/excsubpath/foo.php"),
        BStr::new("<?php class ClassExcludeMapFoo2 {}"),
    );
    s.put(
        s.working_dir
            .join("composersrc/long/excluded/excsubpath/bar.php"),
        BStr::new("<?php class ClassExcludeMapBar {}"),
    );

    s.ensure_dir(s.working_dir.join("composersrc/foo"));

    let composer_out = s.vendor_dir.join("composer");
    dump(&mut s, package.into(), true, BStr::new("_1")).unwrap();

    assert_autoload_files(BStr::new("classmap"), &composer_out, BStr::new("classmap"));
}

#[test]
#[ignore = "asserts function_exists() after requiring the generated autoload.php inside the test process; there is no PHP interpreter to evaluate that file"]
fn test_files_autoload_order_by_dependencies() {
    // TODO(php-runtime): PHP `require autoload.php` + function_exists() assertions have no Rust
    // equivalent (no runtime PHP file loading/class definition).
    todo!()
}

#[test]
#[serial]
fn test_files_autoload_generation_remove_extra_entities_from_autoload_files() {
    let mut s = set_up();
    let autoload_package = new_root_pkg(BStr::new("root/a"));
    autoload_package.set_autoload(autoload(vec![(
        BStr::new("files"),
        str_list(&[BStr::new("root.php")]),
    )]));
    autoload_package.__set_include_paths(vec![BString::from("/lib"), BString::from("/src")]);

    let not_autoload_package = new_root_pkg(BStr::new("root/a"));

    let requires_ = || {
        requires(vec![
            (
                BStr::new("a/a"),
                link(BStr::new("a"), BStr::new("a/a"), match_all(), None),
            ),
            (
                BStr::new("b/b"),
                link(BStr::new("a"), BStr::new("b/b"), match_all(), None),
            ),
            (
                BStr::new("c/c"),
                link(BStr::new("a"), BStr::new("c/c"), match_all(), None),
            ),
        ])
    };
    autoload_package
        .set_requires(
            requires_()
                .into_iter()
                .collect::<indexmap::IndexMap<_, _>>(),
        )
        .unwrap();
    not_autoload_package
        .set_requires(
            requires_()
                .into_iter()
                .collect::<indexmap::IndexMap<_, _>>(),
        )
        .unwrap();

    let a = new_pkg(BStr::new("a/a"));
    let b = new_pkg(BStr::new("b/b"));
    let c = new_pkg(BStr::new("c/c"));
    a.__set_autoload(autoload(vec![(
        BStr::new("files"),
        str_list(&[BStr::new("test.php")]),
    )]));
    a.__set_include_paths(vec![BString::from("lib1"), BString::from("src1")]);
    b.__set_autoload(autoload(vec![(
        BStr::new("files"),
        str_list(&[BStr::new("test2.php")]),
    )]));
    b.__set_include_paths(vec![BString::from("lib2")]);
    c.__set_autoload(autoload(vec![(
        BStr::new("files"),
        str_list(&[BStr::new("test3.php"), BStr::new("foo/bar/test4.php")]),
    )]));
    c.__set_include_paths(vec![BString::from("lib3")]);
    c.__set_target_dir(Some(BString::from("foo/bar")));
    let autoload_packages: Vec<PackageInterfaceHandle> = vec![a.into(), b.into(), c.into()];

    // PHP re-creates the three packages without autoload/include-path settings for the second and
    // third `getCanonicalPackages` calls; the repository contents are swapped between dumps here.
    let not_autoload_packages = || -> Vec<PackageInterfaceHandle> {
        vec![
            new_pkg(BStr::new("a/a")).into(),
            new_pkg(BStr::new("b/b")).into(),
            new_pkg(BStr::new("c/c")).into(),
        ]
    };

    s.ensure_dir(s.vendor_dir.join("a/a"));
    s.ensure_dir(s.vendor_dir.join("b/b"));
    s.ensure_dir(s.vendor_dir.join("c/c/foo/bar"));
    s.put(
        s.vendor_dir.join("a/a/test.php"),
        BStr::new("<?php function testFilesAutoloadGeneration1() {}"),
    );
    s.put(
        s.vendor_dir.join("b/b/test2.php"),
        BStr::new("<?php function testFilesAutoloadGeneration2() {}"),
    );
    s.put(
        s.vendor_dir.join("c/c/foo/bar/test3.php"),
        BStr::new("<?php function testFilesAutoloadGeneration3() {}"),
    );
    s.put(
        s.vendor_dir.join("c/c/foo/bar/test4.php"),
        BStr::new("<?php function testFilesAutoloadGeneration4() {}"),
    );
    s.put(
        s.working_dir.join("root.php"),
        BStr::new("<?php function testFilesAutoloadGenerationRoot() {}"),
    );

    let vendor = s.vendor_dir.clone();
    let composer_out = vendor.join("composer");
    let fx = fixtures_dir();

    s.set_canonical_packages(autoload_packages);
    dump(
        &mut s,
        autoload_package.clone().into(),
        false,
        BStr::new("FilesAutoload"),
    )
    .unwrap();
    assert_file_content_equals(
        fx.join("autoload_functions.php"),
        vendor.join("autoload.php"),
    );
    assert_file_content_equals(
        fx.join("autoload_real_functions_with_include_paths.php"),
        composer_out.join("autoload_real.php"),
    );
    assert_file_content_equals(
        fx.join("autoload_static_functions_with_include_paths.php"),
        composer_out.join("autoload_static.php"),
    );
    assert_file_content_equals(
        fx.join("autoload_files_functions.php"),
        composer_out.join("autoload_files.php"),
    );
    assert_file_content_equals(
        fx.join("include_paths_functions.php"),
        composer_out.join("include_paths.php"),
    );

    s.repository = shirabe::repository::RepositoryInterfaceHandle::new(
        InstalledArrayRepository::new().unwrap(),
    );
    s.set_canonical_packages(not_autoload_packages());
    dump(
        &mut s,
        autoload_package.clone().into(),
        false,
        BStr::new("FilesAutoload"),
    )
    .unwrap();
    assert_file_content_equals(
        fx.join("autoload_functions.php"),
        vendor.join("autoload.php"),
    );
    assert_file_content_equals(
        fx.join("autoload_real_functions_with_include_paths.php"),
        composer_out.join("autoload_real.php"),
    );
    assert_file_content_equals(
        fx.join("autoload_files_functions_with_removed_extra.php"),
        composer_out.join("autoload_files.php"),
    );
    assert_file_content_equals(
        fx.join("include_paths_functions_with_removed_extra.php"),
        composer_out.join("include_paths.php"),
    );

    s.repository = shirabe::repository::RepositoryInterfaceHandle::new(
        InstalledArrayRepository::new().unwrap(),
    );
    s.set_canonical_packages(not_autoload_packages());
    dump(
        &mut s,
        not_autoload_package.into(),
        false,
        BStr::new("FilesAutoload"),
    )
    .unwrap();
    assert_file_content_equals(
        fx.join("autoload_functions.php"),
        vendor.join("autoload.php"),
    );
    assert_file_content_equals(
        fx.join("autoload_real_functions_with_removed_include_paths_and_autolad_files.php"),
        composer_out.join("autoload_real.php"),
    );
    assert_file_content_equals(
        fx.join("autoload_static_functions_with_removed_include_paths_and_autolad_files.php"),
        composer_out.join("autoload_static.php"),
    );
    assert!(!std::path::Path::new(&composer_out.join("autoload_files.php")).exists());
    assert!(!std::path::Path::new(&composer_out.join("include_paths.php")).exists());
}

#[test]
#[ignore = "asserts PHP get_include_path() after require autoload.php"]
fn test_include_paths_are_prepended_in_autoload_file() {
    // TODO(php-runtime): asserts PHP get_include_path() after `require autoload.php`; no Rust
    // equivalent for PHP's include path / runtime require.
    todo!()
}

#[test]
#[ignore = "asserts PHP get_include_path() after require autoload.php"]
fn test_include_paths_in_root_package() {
    // TODO(php-runtime): asserts PHP get_include_path() after `require autoload.php`; no Rust
    // equivalent for PHP's include path / runtime require.
    todo!()
}

#[test]
#[serial]
fn test_pre_and_post_events_are_dispatched_during_autoload_dump() {
    let mut s = set_up();

    let series = std::rc::Rc::new(std::cell::RefCell::new(vec![
        (bstr::BStr::new(ScriptEvents::PRE_AUTOLOAD_DUMP), false),
        (bstr::BStr::new(ScriptEvents::POST_AUTOLOAD_DUMP), false),
    ]));
    let dispatched = std::rc::Rc::new(std::cell::Cell::new(0));
    let series_for_cb = series.clone();
    let dispatched_for_cb = dispatched.clone();
    s.event_dispatcher
        .borrow_mut()
        .__set_dispatch_script_override(Box::new(move |r#type, dev, _args, _flags| {
            dispatched_for_cb.set(dispatched_for_cb.get() + 1);
            let expected = series_for_cb.borrow_mut().remove(0);
            assert_eq!(expected, (r#type, dev));

            Ok(0)
        }));

    let package = new_root_pkg(BStr::new("root/a"));
    package.set_autoload(autoload(vec![(
        BStr::new("psr-0"),
        str_map(&[(
            BStr::new("Prefix"),
            pstr(BStr::new("foo/bar/non/existing/")),
        )]),
    )]));

    s.generator.set_run_scripts(true);
    dump(&mut s, package.into(), true, BStr::new("_8")).unwrap();

    assert_eq!(2, dispatched.get());
}

#[test]
#[serial]
fn test_use_global_include_path() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));
    package.set_autoload(autoload(vec![(
        BStr::new("psr-0"),
        str_map(&[
            (BStr::new("Main\\Foo"), pstr(BStr::new(""))),
            (BStr::new("Main\\Bar"), pstr(BStr::new(""))),
        ]),
    )]));
    package.__set_target_dir(Some(BString::from("Main/Foo/")));

    s.use_include_path = true;

    s.ensure_dir(s.vendor_dir.join("a"));

    let composer_out = s.vendor_dir.join("composer");
    dump(&mut s, package.into(), false, BStr::new("IncludePath")).unwrap();
    let fx = fixtures_dir();
    assert_file_content_equals(
        fx.join("autoload_real_include_path.php"),
        composer_out.join("autoload_real.php"),
    );
    assert_file_content_equals(
        fx.join("autoload_static_include_path.php"),
        composer_out.join("autoload_static.php"),
    );
}

#[test]
#[serial]
fn test_vendor_dir_excluded_from_working_dir() {
    let mut s = set_up();
    let working_dir = s.vendor_dir.join("working-dir");
    let vendor_dir = working_dir.join("../vendor");

    s.ensure_dir(&working_dir);
    std::env::set_current_dir(&working_dir).unwrap();

    let package = new_root_pkg(BStr::new("root/a"));
    package.set_autoload(autoload(vec![
        (
            BStr::new("psr-0"),
            str_map(&[(BStr::new("Foo"), pstr(BStr::new("src")))]),
        ),
        (
            BStr::new("psr-4"),
            str_map(&[(BStr::new("Acme\\Foo\\"), pstr(BStr::new("src-psr4")))]),
        ),
        (BStr::new("classmap"), str_list(&[BStr::new("classmap")])),
        (BStr::new("files"), str_list(&[BStr::new("test.php")])),
    ]));
    package
        .set_requires(
            requires(vec![(
                BStr::new("b/b"),
                link(BStr::new("a"), BStr::new("b/b"), match_all(), None),
            )])
            .into_iter()
            .collect::<indexmap::IndexMap<_, _>>(),
        )
        .unwrap();

    let vendor_package = new_pkg(BStr::new("b/b"));
    vendor_package.__set_autoload(autoload(vec![
        (
            BStr::new("psr-0"),
            str_map(&[(BStr::new("Bar"), pstr(BStr::new("lib")))]),
        ),
        (
            BStr::new("psr-4"),
            str_map(&[(BStr::new("Acme\\Bar\\"), pstr(BStr::new("lib-psr4")))]),
        ),
        (BStr::new("classmap"), str_list(&[BStr::new("classmaps")])),
        (BStr::new("files"), str_list(&[BStr::new("bootstrap.php")])),
    ]));

    s.set_canonical_packages(vec![vendor_package.into()]);

    let im: std::rc::Rc<std::cell::RefCell<dyn shirabe::installer::InstallationManagerInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(make_installation_manager(
            &vendor_dir,
            s.io.clone(),
        )));

    s.ensure_dir(working_dir.join("src/Foo"));
    s.ensure_dir(working_dir.join("classmap"));
    s.ensure_dir(vendor_dir.join("composer"));
    s.ensure_dir(vendor_dir.join("b/b/lib/Bar"));
    s.ensure_dir(vendor_dir.join("b/b/classmaps"));
    s.put(
        working_dir.join("src/Foo/Bar.php"),
        BStr::new("<?php namespace Foo; class Bar {}"),
    );
    s.put(
        working_dir.join("classmap/classes.php"),
        BStr::new("<?php namespace Foo; class Foo {}"),
    );
    s.put(
        working_dir.join("test.php"),
        BStr::new("<?php class Foo {}"),
    );
    s.put(
        vendor_dir.join("b/b/lib/Bar/Foo.php"),
        BStr::new("<?php namespace Bar; class Foo {}"),
    );
    s.put(
        vendor_dir.join("b/b/classmaps/classes.php"),
        BStr::new("<?php namespace Bar; class Bar {}"),
    );
    s.put(
        vendor_dir.join("b/b/bootstrap.php"),
        BStr::new("<?php class Bar {}"),
    );

    s.vendor_dir = vendor_dir.clone();
    let config = s.config();
    s.generator
        .dump(
            &config,
            s.repository.clone(),
            package.into(),
            im,
            BStr::new("composer"),
            true,
            Some(BString::from("_13")),
            None,
            false,
        )
        .unwrap();

    let expected_namespace = BStr::new(
        "<?php\n\n// autoload_namespaces.php @generated by Composer\n\n$vendorDir = dirname(__DIR__);\n$baseDir = dirname($vendorDir).'/working-dir';\n\nreturn array(\n    'Foo' => array($baseDir . '/src'),\n    'Bar' => array($vendorDir . '/b/b/lib'),\n);\n",
    );

    let expected_psr4 = BStr::new(
        "<?php\n\n// autoload_psr4.php @generated by Composer\n\n$vendorDir = dirname(__DIR__);\n$baseDir = dirname($vendorDir).'/working-dir';\n\nreturn array(\n    'Acme\\\\Foo\\\\' => array($baseDir . '/src-psr4'),\n    'Acme\\\\Bar\\\\' => array($vendorDir . '/b/b/lib-psr4'),\n);\n",
    );

    let expected_classmap = BStr::new(
        "<?php\n\n// autoload_classmap.php @generated by Composer\n\n$vendorDir = dirname(__DIR__);\n$baseDir = dirname($vendorDir).'/working-dir';\n\nreturn array(\n    'Bar\\\\Bar' => $vendorDir . '/b/b/classmaps/classes.php',\n    'Bar\\\\Foo' => $vendorDir . '/b/b/lib/Bar/Foo.php',\n    'Composer\\\\InstalledVersions' => $vendorDir . '/composer/InstalledVersions.php',\n    'Foo\\\\Bar' => $baseDir . '/src/Foo/Bar.php',\n    'Foo\\\\Foo' => $baseDir . '/classmap/classes.php',\n);\n",
    );

    assert_str_equals_file(
        expected_namespace,
        vendor_dir.join("composer/autoload_namespaces.php"),
    );
    assert_str_equals_file(expected_psr4, vendor_dir.join("composer/autoload_psr4.php"));
    assert_str_equals_file(
        expected_classmap,
        vendor_dir.join("composer/autoload_classmap.php"),
    );
    let files = bstr::BString::from(
        std::fs::read(vendor_dir.join("composer/autoload_files.php"))
            .unwrap()
            .replace("\r", ""),
    );
    assert!(files.contains_str("$vendorDir . '/b/b/bootstrap.php',\n"));
    assert!(files.contains_str("$baseDir . '/test.php',\n"));
}

#[test]
#[serial]
fn test_up_level_relative_paths() {
    let mut s = set_up();
    let working_dir = s.working_dir.join("working-dir");
    std::fs::create_dir_all(&working_dir).unwrap();
    std::env::set_current_dir(&working_dir).unwrap();

    let package = new_root_pkg(BStr::new("root/a"));
    package.set_autoload(autoload(vec![
        (
            BStr::new("psr-0"),
            str_map(&[(BStr::new("Foo"), pstr(BStr::new("../path/../src")))]),
        ),
        (
            BStr::new("psr-4"),
            str_map(&[(
                BStr::new("Acme\\Foo\\"),
                pstr(BStr::new("../path/../src-psr4")),
            )]),
        ),
        (
            BStr::new("classmap"),
            str_list(&[
                BStr::new("../classmap"),
                BStr::new("../classmap2/subdir"),
                BStr::new("classmap3"),
                BStr::new("classmap4"),
            ]),
        ),
        (BStr::new("files"), str_list(&[BStr::new("../test.php")])),
        (
            BStr::new("exclude-from-classmap"),
            str_list(&[
                BStr::new("./../classmap/excluded"),
                BStr::new("../classmap2"),
                BStr::new("classmap3/classes.php"),
                BStr::new("classmap4/*/classes.php"),
            ]),
        ),
    ]));

    s.ensure_dir(s.working_dir.join("src/Foo"));
    s.ensure_dir(s.working_dir.join("classmap/excluded"));
    s.ensure_dir(s.working_dir.join("classmap2/subdir"));
    s.ensure_dir(s.working_dir.join("working-dir/classmap3"));
    s.ensure_dir(s.working_dir.join("working-dir/classmap4/foo/"));
    s.put(
        s.working_dir.join("src/Foo/Bar.php"),
        BStr::new("<?php namespace Foo; class Bar {}"),
    );
    s.put(
        s.working_dir.join("classmap/classes.php"),
        BStr::new("<?php namespace Foo; class Foo {}"),
    );
    s.put(
        s.working_dir.join("classmap/excluded/classes.php"),
        BStr::new("<?php namespace Foo; class Boo {}"),
    );
    s.put(
        s.working_dir.join("classmap2/subdir/classes.php"),
        BStr::new("<?php namespace Foo; class Boo2 {}"),
    );
    s.put(
        s.working_dir.join("working-dir/classmap3/classes.php"),
        BStr::new("<?php namespace Foo; class Boo3 {}"),
    );
    s.put(
        s.working_dir.join("working-dir/classmap4/foo/classes.php"),
        BStr::new("<?php namespace Foo; class Boo4 {}"),
    );
    s.put(
        s.working_dir.join("test.php"),
        BStr::new("<?php class Foo {}"),
    );

    dump(&mut s, package.into(), true, BStr::new("_14")).unwrap();

    let expected_namespace = BStr::new(
        "<?php\n\n// autoload_namespaces.php @generated by Composer\n\n$vendorDir = dirname(__DIR__);\n$baseDir = dirname($vendorDir).'/working-dir';\n\nreturn array(\n    'Foo' => array($baseDir . '/../src'),\n);\n",
    );

    let expected_psr4 = BStr::new(
        "<?php\n\n// autoload_psr4.php @generated by Composer\n\n$vendorDir = dirname(__DIR__);\n$baseDir = dirname($vendorDir).'/working-dir';\n\nreturn array(\n    'Acme\\\\Foo\\\\' => array($baseDir . '/../src-psr4'),\n);\n",
    );

    let expected_classmap = BStr::new(
        "<?php\n\n// autoload_classmap.php @generated by Composer\n\n$vendorDir = dirname(__DIR__);\n$baseDir = dirname($vendorDir).'/working-dir';\n\nreturn array(\n    'Composer\\\\InstalledVersions' => $vendorDir . '/composer/InstalledVersions.php',\n    'Foo\\\\Bar' => $baseDir . '/../src/Foo/Bar.php',\n    'Foo\\\\Foo' => $baseDir . '/../classmap/classes.php',\n);\n",
    );

    assert_str_equals_file(
        expected_namespace,
        s.vendor_dir.join("composer/autoload_namespaces.php"),
    );
    assert_str_equals_file(
        expected_psr4,
        s.vendor_dir.join("composer/autoload_psr4.php"),
    );
    assert_str_equals_file(
        expected_classmap,
        s.vendor_dir.join("composer/autoload_classmap.php"),
    );
    let files = bstr::BString::from(
        std::fs::read(s.vendor_dir.join("composer/autoload_files.php"))
            .unwrap()
            .replace("\r", ""),
    );
    assert!(files.contains_str("$baseDir . '/../test.php',\n"));
}

#[test]
#[serial]
fn test_autoload_rules_in_package_that_does_not_exist_on_disk() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("root/a"));
    package
        .set_requires(
            requires(vec![(
                BStr::new("dep/a"),
                link(
                    BStr::new("root/a"),
                    BStr::new("dep/a"),
                    match_all(),
                    Some(BStr::new("requires")),
                ),
            )])
            .into_iter()
            .collect::<indexmap::IndexMap<_, _>>(),
        )
        .unwrap();
    let dep = CompletePackageHandle::new(
        BString::from("dep/a"),
        BString::from("1.0"),
        BString::from("1.0"),
    );

    s.set_canonical_packages(vec![dep.clone().into()]);

    dep.__set_autoload(autoload(vec![(
        BStr::new("psr-0"),
        str_map(&[(BStr::new("Foo"), pstr(BStr::new("./src")))]),
    )]));
    dump(&mut s, package.clone().into(), true, BStr::new("_19")).unwrap();

    let expected_namespace = BStr::new(
        "<?php\n\n// autoload_namespaces.php @generated by Composer\n\n$vendorDir = dirname(__DIR__);\n$baseDir = dirname($vendorDir);\n\nreturn array(\n    'Foo' => array($vendorDir . '/dep/a/src'),\n);\n",
    );
    assert_str_equals_file(
        expected_namespace,
        s.vendor_dir.join("composer/autoload_namespaces.php"),
    );

    dep.__set_autoload(autoload(vec![(
        BStr::new("psr-4"),
        str_map(&[(BStr::new("Acme\\Foo\\"), pstr(BStr::new("./src-psr4")))]),
    )]));
    dump(&mut s, package.clone().into(), true, BStr::new("_19")).unwrap();

    let expected_psr4 = BStr::new(
        "<?php\n\n// autoload_psr4.php @generated by Composer\n\n$vendorDir = dirname(__DIR__);\n$baseDir = dirname($vendorDir);\n\nreturn array(\n    'Acme\\\\Foo\\\\' => array($vendorDir . '/dep/a/src-psr4'),\n);\n",
    );
    assert_str_equals_file(
        expected_psr4,
        s.vendor_dir.join("composer/autoload_psr4.php"),
    );

    dep.__set_autoload(autoload(vec![(
        BStr::new("classmap"),
        str_list(&[BStr::new("classmap")]),
    )]));
    let expected_message = bconcat!(
        "Could not scan for classes inside \"",
        s.vendor_dir,
        "/dep/a/classmap\" which does not appear to be a file nor a folder"
    );
    if let Err(e) = dump(&mut s, package.clone().into(), true, BStr::new("_19")) {
        assert_eq!(expected_message, error_message(&e));
    }

    dep.__set_autoload(autoload(vec![(
        BStr::new("files"),
        str_list(&[BStr::new("./test.php")]),
    )]));
    dump(&mut s, package.clone().into(), true, BStr::new("_19")).unwrap();
    let files = bstr::BString::from(
        std::fs::read(s.vendor_dir.join("composer/autoload_files.php")).unwrap(),
    );
    assert!(files.contains_str("$vendorDir . '/dep/a/test.php',\n"));

    package.set_autoload(autoload(vec![(
        BStr::new("exclude-from-classmap"),
        str_list(&[BStr::new("../excludedroot"), BStr::new("root/excl")]),
    )]));
    dep.__set_autoload(autoload(vec![(
        BStr::new("exclude-from-classmap"),
        str_list(&[BStr::new("../../excluded"), BStr::new("foo/bar")]),
    )]));
    let map = s
        .generator
        .build_package_map(
            s.im.clone(),
            package.clone().into(),
            vec![dep.clone().into()],
        )
        .unwrap();
    let parsed = s
        .generator
        .parse_autoloads(map, package.into(), PhpMixed::Bool(false));
    let excluded_root = bconcat!(
        preg_quote(
            strtr(
                realpath(path_from_bytes(&dirname(path_to_bytes(&s.working_dir))))
                    .map(|p| bstr::BString::from(path_to_bytes(&p)))
                    .unwrap_or_default(),
                "\\",
                "/"
            ),
            None
        ),
        "/excludedroot($|/)"
    );
    let root_excl = bconcat!(
        preg_quote(
            strtr(
                realpath(&s.working_dir)
                    .map(|p| bstr::BString::from(path_to_bytes(&p)))
                    .unwrap_or_default(),
                "\\",
                "/"
            ),
            None
        ),
        "/root/excl($|/)"
    );
    assert_eq!(
        str_map(&[
            (BStr::new("0"), pstr(excluded_root.to_owned().as_bstr())),
            (BStr::new("1"), pstr(root_excl.to_owned().as_bstr()))
        ]),
        parsed[B("exclude-from-classmap")]
    );
}

/// ref: AutoloadGeneratorTest::platformCheckProvider — builds the link map for a requires/provides/
/// replaces entry. Each tuple is `(key, target, constraint)` mirroring `new Link('a', target, parse)`.
fn platform_links(
    vp: &VersionParser,
    entries: &[(&BStr, &BStr, &BStr)],
) -> IndexMap<BString, Link> {
    requires(
        entries
            .iter()
            .map(|(key, target, c)| {
                (
                    *key,
                    link(
                        BStr::new("a"),
                        target,
                        vp.parse_constraints(c).unwrap(),
                        None,
                    ),
                )
            })
            .collect(),
    )
}

#[test]
#[serial]
#[ignore = "get_platform_check: an extension whose provider/replacer constraint does NOT satisfy the requirement (ext-pdo 7.1.* vs ^7.2) is still skipped, so platform_check.php is not generated where Composer generates it"]
fn test_generates_platform_check() {
    let vp = VersionParser;

    // ref: AutoloadGeneratorTest::platformCheckProvider (all 13 scenarios).
    struct Scenario {
        name: &'static BStr,
        requires: Vec<(&'static BStr, &'static BStr, &'static BStr)>,
        expected_fixture: Option<&'static BStr>,
        provides: Vec<(&'static BStr, &'static BStr, &'static BStr)>,
        replaces: Vec<(&'static BStr, &'static BStr, &'static BStr)>,
        ignore_platform_reqs: PhpMixed,
    }

    let scenarios = vec![
        Scenario {
            name: BStr::new("Typical project requirements"),
            requires: vec![
                (BStr::new("php"), BStr::new("php"), BStr::new("^7.2")),
                (BStr::new("ext-xml"), BStr::new("ext-xml"), BStr::new("*")),
                (BStr::new("ext-json"), BStr::new("ext-json"), BStr::new("*")),
            ],
            expected_fixture: Some(BStr::new("typical")),
            provides: vec![],
            replaces: vec![],
            ignore_platform_reqs: PhpMixed::Bool(false),
        },
        Scenario {
            name: BStr::new("No PHP lower bound"),
            requires: vec![(BStr::new("php"), BStr::new("php"), BStr::new("< 8"))],
            expected_fixture: None,
            provides: vec![],
            replaces: vec![],
            ignore_platform_reqs: PhpMixed::Bool(false),
        },
        Scenario {
            name: BStr::new("No PHP upper bound"),
            requires: vec![(BStr::new("php"), BStr::new("php"), BStr::new(">= 7.2"))],
            expected_fixture: Some(BStr::new("no_php_upper_bound")),
            provides: vec![],
            replaces: vec![],
            ignore_platform_reqs: PhpMixed::Bool(false),
        },
        Scenario {
            name: BStr::new("Specific PHP release version"),
            requires: vec![(BStr::new("php"), BStr::new("php"), BStr::new("^7.2.8"))],
            expected_fixture: Some(BStr::new("specific_php_release")),
            provides: vec![],
            replaces: vec![],
            ignore_platform_reqs: PhpMixed::Bool(false),
        },
        Scenario {
            name: BStr::new("Specific 64-bit PHP version"),
            requires: vec![(
                BStr::new("php-64bit"),
                BStr::new("php-64bit"),
                BStr::new("^7.2.8"),
            )],
            expected_fixture: Some(BStr::new("specific_php_64bit_required")),
            provides: vec![],
            replaces: vec![],
            ignore_platform_reqs: PhpMixed::Bool(false),
        },
        Scenario {
            name: BStr::new("64-bit PHP required"),
            requires: vec![(
                BStr::new("php-64bit"),
                BStr::new("php-64bit"),
                BStr::new("*"),
            )],
            expected_fixture: Some(BStr::new("php_64bit_required")),
            provides: vec![],
            replaces: vec![],
            ignore_platform_reqs: PhpMixed::Bool(false),
        },
        Scenario {
            name: BStr::new("No PHP required"),
            requires: vec![
                (BStr::new("ext-xml"), BStr::new("ext-xml"), BStr::new("*")),
                (BStr::new("ext-json"), BStr::new("ext-json"), BStr::new("*")),
            ],
            expected_fixture: Some(BStr::new("no_php_required")),
            provides: vec![],
            replaces: vec![],
            ignore_platform_reqs: PhpMixed::Bool(false),
        },
        Scenario {
            name: BStr::new("Ignoring all platform requirements skips check completely"),
            requires: vec![
                (BStr::new("php"), BStr::new("php"), BStr::new("^7.2")),
                (BStr::new("ext-xml"), BStr::new("ext-xml"), BStr::new("*")),
                (BStr::new("ext-json"), BStr::new("ext-json"), BStr::new("*")),
            ],
            expected_fixture: None,
            provides: vec![],
            replaces: vec![],
            ignore_platform_reqs: PhpMixed::Bool(true),
        },
        Scenario {
            name: BStr::new("Ignored platform requirements are not checked for"),
            requires: vec![
                (BStr::new("php"), BStr::new("php"), BStr::new("^7.2.8")),
                (BStr::new("ext-xml"), BStr::new("ext-xml"), BStr::new("*")),
                (BStr::new("ext-json"), BStr::new("ext-json"), BStr::new("*")),
                (BStr::new("ext-pdo"), BStr::new("ext-pdo"), BStr::new("*")),
            ],
            expected_fixture: Some(BStr::new("no_php_required")),
            provides: vec![],
            replaces: vec![],
            ignore_platform_reqs: str_list(&[BStr::new("php"), BStr::new("ext-pdo")]),
        },
        Scenario {
            name: BStr::new("Via wildcard ignored platform requirements are not checked for"),
            requires: vec![
                (BStr::new("php"), BStr::new("php"), BStr::new("^7.2.8")),
                (BStr::new("ext-xml"), BStr::new("ext-xml"), BStr::new("*")),
                (BStr::new("ext-json"), BStr::new("ext-json"), BStr::new("*")),
                (
                    BStr::new("ext-fileinfo"),
                    BStr::new("ext-fileinfo"),
                    BStr::new("*"),
                ),
                (
                    BStr::new("ext-filesystem"),
                    BStr::new("ext-filesystem"),
                    BStr::new("*"),
                ),
                (
                    BStr::new("ext-filter"),
                    BStr::new("ext-filter"),
                    BStr::new("*"),
                ),
            ],
            expected_fixture: Some(BStr::new("no_php_required")),
            provides: vec![],
            replaces: vec![],
            ignore_platform_reqs: str_list(&[BStr::new("php"), BStr::new("ext-fil*")]),
        },
        Scenario {
            name: BStr::new("No extensions required"),
            requires: vec![(BStr::new("php"), BStr::new("php"), BStr::new("^7.2"))],
            expected_fixture: Some(BStr::new("no_extensions_required")),
            provides: vec![],
            replaces: vec![],
            ignore_platform_reqs: PhpMixed::Bool(false),
        },
        Scenario {
            name: BStr::new(
                "Replaced/provided extensions are not checked for + checking case insensitivity",
            ),
            requires: vec![
                (
                    BStr::new("ext-xml"),
                    BStr::new("ext-xml"),
                    BStr::new("^7.2"),
                ),
                (
                    BStr::new("ext-pdo"),
                    BStr::new("ext-Pdo"),
                    BStr::new("^7.2"),
                ),
                (
                    BStr::new("ext-bcmath"),
                    BStr::new("ext-bcMath"),
                    BStr::new("^7.2"),
                ),
            ],
            expected_fixture: Some(BStr::new("replaced_provided_exts")),
            // constraint does not satisfy all the ^7.2 requirement so we do not accept ext-pdo as being replaced;
            // valid replace of bcmath so no need to check for it
            replaces: vec![
                (
                    BStr::new("ext-pdo"),
                    BStr::new("ext-PDO"),
                    BStr::new("7.1.*"),
                ),
                (
                    BStr::new("ext-bcmath"),
                    BStr::new("ext-BCMath"),
                    BStr::new("^7.1"),
                ),
            ],
            // valid provide of ext-xml so no need to check for it
            provides: vec![(BStr::new("ext-xml"), BStr::new("ext-XML"), BStr::new("*"))],
            ignore_platform_reqs: PhpMixed::Bool(false),
        },
    ];

    for scenario in scenarios {
        let mut s = set_up();
        let package = new_root_pkg(BStr::new("root/a"));
        package
            .set_requires(
                platform_links(&vp, &scenario.requires)
                    .into_iter()
                    .collect::<indexmap::IndexMap<_, _>>(),
            )
            .unwrap();

        if !scenario.provides.is_empty() {
            package
                .set_provides(
                    platform_links(&vp, &scenario.provides)
                        .into_iter()
                        .collect::<indexmap::IndexMap<_, _>>(),
                )
                .unwrap();
        }

        if !scenario.replaces.is_empty() {
            package
                .set_replaces(
                    platform_links(&vp, &scenario.replaces)
                        .into_iter()
                        .collect::<indexmap::IndexMap<_, _>>(),
                )
                .unwrap();
        }

        s.generator.set_platform_requirement_filter(
            PlatformRequirementFilterFactory::from_bool_or_list(
                scenario.ignore_platform_reqs.clone(),
            )
            .unwrap(),
        );
        dump(&mut s, package.into(), true, BStr::new("_1")).unwrap();

        let platform_check = s.vendor_dir.join("composer/platform_check.php");
        let autoload_real = bstr::BString::from(
            std::fs::read(s.vendor_dir.join("composer/autoload_real.php"))
                .unwrap()
                .replace("\r", ""),
        );
        match scenario.expected_fixture {
            None => {
                assert!(
                    !std::path::Path::new(&platform_check).exists(),
                    "[{}] platform_check.php should not exist",
                    scenario.name.display_lossy()
                );
                assert!(
                    !autoload_real.contains_str("require __DIR__ . '/platform_check.php';"),
                    "[{}] autoload_real.php should not require platform_check.php",
                    scenario.name.display_lossy()
                );
            }
            Some(fixture) => {
                assert_file_content_equals(
                    fixtures_dir()
                        .join("platform")
                        .join(path_from_bytes(&bconcat!(fixture, ".php"))),
                    &platform_check,
                );
                assert!(
                    autoload_real.contains_str("require __DIR__ . '/platform_check.php';"),
                    "[{}] autoload_real.php should require platform_check.php",
                    scenario.name.display_lossy()
                );
            }
        }
    }
}

#[test]
#[serial]
fn test_absolute_symlink_with_psr4_does_not_generate_warnings() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("test/package"));

    // Create a directory structure with PSR-4 autoloading
    s.ensure_dir(
        s.working_dir
            .join("tools-real/vendor/phpunit/phpunit/src/Framework/Exception"),
    );
    s.put(
        s.working_dir
            .join("tools-real/vendor/phpunit/phpunit/src/Framework/Exception/Exception.php"),
        BStr::new("<?php namespace PHPUnit\\Framework; class Exception extends \\Exception {}"),
    );

    // Create an absolute symlink
    let target = s.working_dir.join("tools-real");
    let r#link = s.working_dir.join("tools");
    assert!(symlink(&target, &r#link).is_ok());

    package.set_autoload(autoload(vec![
        (
            BStr::new("psr-4"),
            str_map(&[(BStr::new("MyTools\\"), pstr(BStr::new("tools/")))]),
        ),
        (
            BStr::new("exclude-from-classmap"),
            str_list(&[BStr::new("**/vendor/")]),
        ),
    ]));

    dump(&mut s, package.into(), true, BStr::new("_9")).unwrap();

    let output = s.io.borrow().get_output();

    // Should not contain PSR-4 violation warnings
    assert!(!output.contains_str("does not comply with psr-4 autoloading standard"));
}

#[test]
#[serial]
fn test_absolute_symlink_with_classmap_exclude_from_classmap() {
    let mut s = set_up();
    let package = new_root_pkg(BStr::new("test/package"));

    // Create a directory structure with files
    s.ensure_dir(
        s.working_dir
            .join("tools-real/vendor/phpunit/phpunit/src/Framework"),
    );
    s.put(
        s.working_dir
            .join("tools-real/vendor/phpunit/phpunit/src/Framework/Exception.php"),
        BStr::new("<?php namespace PHPUnit\\Framework; class Exception extends \\Exception {}"),
    );
    s.put(
        s.working_dir.join("tools-real/MyClass.php"),
        BStr::new("<?php class MyClass {}"),
    );

    // Create an absolute symlink
    let target = s.working_dir.join("tools-real");
    let r#link = s.working_dir.join("tools");
    assert!(symlink(&target, &r#link).is_ok());

    package.set_autoload(autoload(vec![
        (BStr::new("classmap"), str_list(&[BStr::new("tools/")])),
        (
            BStr::new("exclude-from-classmap"),
            str_list(&[BStr::new("**/vendor/")]),
        ),
    ]));

    let class_map = dump(&mut s, package.into(), false, BStr::new("_9")).unwrap();

    // Check that MyClass is included but vendor files are excluded
    assert!(class_map.get_map().contains_key(B("MyClass")));
    assert!(
        !class_map
            .get_map()
            .contains_key(B("PHPUnit\\Framework\\Exception"))
    );
}
