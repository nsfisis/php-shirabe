//! ref: composer/tests/Composer/Test/Plugin/PluginInstallerTest.php

use crate::async_runtime::run;
use indexmap::IndexMap;
use shirabe::autoload::AutoloadGenerator;
use shirabe::composer::{Composer, ComposerHandle, PartialOrFullComposer};
use shirabe::config::Config;
use shirabe::dependency_resolver::operation::AnyOperation;
use shirabe::downloader::{DownloadManagerInterface, DownloaderInterface};
use shirabe::event_dispatcher::EventDispatcher;
use shirabe::factory::DisablePlugins;
use shirabe::installer::{
    InstallationManager, InstallationManagerInterface, InstallerInterface, PluginInstaller,
};
use shirabe::io::IOInterface;
use shirabe::io::buffer_io::BufferIO;
use shirabe::json::JsonFile;
use shirabe::package::loader::{ArrayLoader, JsonLoader, JsonLoaderInput};
use shirabe::package::{Locker, LockerInterface, PackageInterfaceHandle, RootPackageHandle};
use shirabe::plugin::plugin_interface::PluginInterface;
use shirabe::plugin::{Capable, PluginManager};
use shirabe::repository::{
    InstalledArrayRepository, InstalledRepositoryInterfaceHandle, RepositoryInterfaceHandle,
    RepositoryManagerInterface,
};
use shirabe::util::Platform;
use shirabe::util::http_downloader::HttpDownloader;
use shirabe::util::r#loop::Loop;
use shirabe::util::process_executor::ProcessExecutor;
use shirabe_external_packages::symfony::console::output::output_interface::VERBOSITY_NORMAL;
use shirabe_external_packages::symfony::process::PhpExecutableFinder;
use shirabe_php_shim::PhpMixed;
use tempfile::TempDir;

/// The register/activate flow runs the plugin in the real PHP worker; without a PHP binary the
/// worker cannot start. Tests exercising it return early, following the convention of the
/// non-mock tests in `shirabe-php-rpc`.
fn php_runtime_available() -> bool {
    PhpExecutableFinder::new().find(false).is_some()
}

/// All tests in this binary share the single PHP worker, whose loaded-class table persists
/// across tests just like PHPUnit's single-process runs (that sharing is what exercises the
/// `_composer_tmp` rename path). Interleaving two tests would let one test's class definitions
/// race the other's `class_exists` checks, so the worker-touching tests run serialized.
static PHP_WORKER_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn lock_php_worker() -> std::sync::MutexGuard<'static, ()> {
    PHP_WORKER_TESTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// `__DIR__ . '/Fixtures'` of the upstream test class.
fn fixtures_dir() -> String {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../composer/tests/Composer/Test/Plugin/Fixtures");
    dir.canonicalize()
        .expect("the Composer checkout must provide the plugin fixtures")
        .to_str()
        .unwrap()
        .to_string()
}

// PHP mocks `Composer\Downloader\DownloadManager`; install/update/remove resolve to null and the
// other methods are never reached by these tests.
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
        ) -> anyhow::Result<Option<std::rc::Rc<std::cell::RefCell<dyn DownloaderInterface>>>>;
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

/// PHP mocks `Composer\Repository\RepositoryManager` so that getLocalRepository returns the
/// test repository; the other methods are never reached.
#[derive(Debug)]
struct MockRepositoryManager {
    local: RepositoryInterfaceHandle,
    repositories: Vec<RepositoryInterfaceHandle>,
}

impl RepositoryManagerInterface for MockRepositoryManager {
    fn get_local_repository(&self) -> RepositoryInterfaceHandle {
        self.local.clone()
    }

    fn get_repositories(&self) -> &Vec<RepositoryInterfaceHandle> {
        &self.repositories
    }

    fn create_repository(
        &self,
        _type: &str,
        _config: IndexMap<String, PhpMixed>,
        _name: Option<&str>,
    ) -> anyhow::Result<RepositoryInterfaceHandle> {
        unimplemented!("not exercised by PluginInstallerTest")
    }

    fn add_repository(&mut self, _repository: RepositoryInterfaceHandle) {
        unimplemented!("not exercised by PluginInstallerTest")
    }

    fn set_local_repository(&mut self, repository: RepositoryInterfaceHandle) {
        self.local = repository;
    }
}

/// PHP mocks `Composer\Installer\InstallationManager` so that getInstallPath maps a package to
/// `__DIR__.'/Fixtures/'.$package->getPrettyName()`; every other method keeps the PHPUnit mock
/// default (no-op / falsy).
#[derive(Debug)]
struct MockInstallationManager;

impl InstallationManagerInterface for MockInstallationManager {
    fn add_installer(&mut self, _installer: Box<dyn InstallerInterface>) {}

    fn remove_installer(&mut self, _installer: &dyn InstallerInterface) {}

    fn disable_plugins(&mut self) {}

    fn is_package_installed(
        &mut self,
        _repo: &InstalledRepositoryInterfaceHandle,
        _package: PackageInterfaceHandle,
    ) -> anyhow::Result<bool> {
        Ok(false)
    }

    fn ensure_binaries_presence(&mut self, _package: PackageInterfaceHandle) {}

    fn execute(
        &mut self,
        _repo: &InstalledRepositoryInterfaceHandle,
        _operations: Vec<AnyOperation>,
        _dev_mode: bool,
        _run_scripts: bool,
        _download_only: bool,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    fn get_install_path(&self, package: PackageInterfaceHandle) -> Option<String> {
        Some(format!("{}/{}", fixtures_dir(), package.get_pretty_name()))
    }

    fn set_output_progress(&mut self, _output_progress: bool) {}

    fn notify_installs(&mut self, _io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>) {}
}

/// Equivalent to PHP setUp()'s `new InstallationManager(...)`, used only to satisfy
/// `Locker::new`'s concrete constructor argument (PHP hands the same InstallationManager mock to
/// the Locker, which never touches it in these tests).
fn locker_installation_manager(
    io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
) -> std::rc::Rc<std::cell::RefCell<InstallationManager>> {
    let config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(false, None)));
    let http_downloader = std::rc::Rc::new(std::cell::RefCell::new(HttpDownloader::new(
        io.clone(),
        config,
        IndexMap::new(),
        true,
    )));
    let r#loop = std::rc::Rc::new(std::cell::RefCell::new(Loop::new(http_downloader, None)));
    std::rc::Rc::new(std::cell::RefCell::new(InstallationManager::new(
        r#loop,
        io.clone(),
        None,
    )))
}

#[derive(Debug)]
struct SetUp {
    io: std::rc::Rc<std::cell::RefCell<BufferIO>>,
    io_dyn: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    pm: std::rc::Rc<std::cell::RefCell<PluginManager>>,
    autoload_generator: std::rc::Rc<std::cell::RefCell<AutoloadGenerator>>,
    packages: Vec<PackageInterfaceHandle>,
    repository: InstalledRepositoryInterfaceHandle,
    // Keeps the Composer alive; PluginManager only holds a weak back-reference to it.
    composer: ComposerHandle,
    // PHP's tearDown() removes this directory; TempDir does the same on drop.
    _directory: TempDir,
}

fn set_up() -> SetUp {
    let loader = JsonLoader::new(Box::new(ArrayLoader::new(None, false)));
    let mut packages = vec![];
    let directory = TempDir::new().unwrap();
    let directory_path = directory.path().to_str().unwrap().to_string();
    for i in 1..=8 {
        std::fs::create_dir_all(format!("{}/Fixtures/plugin-v{}", directory_path, i)).unwrap();
        packages.push(
            loader
                .load(JsonLoaderInput::String(format!(
                    "{}/plugin-v{}/composer.json",
                    fixtures_dir(),
                    i
                )))
                .unwrap(),
        );
    }

    let mut dm = MockDownloadManager::new();
    dm.expect_install().returning(|_, _| Ok(None));
    dm.expect_update().returning(|_, _, _| Ok(None));
    dm.expect_remove().returning(|_, _| Ok(None));

    let repository =
        InstalledRepositoryInterfaceHandle::new(InstalledArrayRepository::new().unwrap());

    let repository_manager = MockRepositoryManager {
        local: repository.as_repository_handle(),
        repositories: vec![],
    };

    let installation_manager = MockInstallationManager;

    let io = std::rc::Rc::new(std::cell::RefCell::new(
        BufferIO::new(String::new(), VERBOSITY_NORMAL, None).unwrap(),
    ));
    let io_dyn: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io.clone();

    let composer_rc = std::rc::Rc::new(std::cell::RefCell::new(PartialOrFullComposer::Full(
        Composer::new(),
    )));
    let composer = ComposerHandle::from_rc_unchecked(composer_rc.clone());

    // PHP hands the AutoloadGenerator a mocked EventDispatcher (disabled constructor); the Rust
    // AutoloadGenerator requires a concrete EventDispatcher, and none of these tests reach it.
    let dispatcher = std::rc::Rc::new(std::cell::RefCell::new(EventDispatcher::new(
        composer.upcast().downgrade(),
        io_dyn.clone(),
        None,
    )));
    let autoload_generator = std::rc::Rc::new(std::cell::RefCell::new(AutoloadGenerator::new(
        dispatcher,
        Some(io_dyn.clone()),
    )));

    let mut config = Config::new(false, None);
    let mut config_section: IndexMap<String, PhpMixed> = IndexMap::new();
    config_section.insert(
        "vendor-dir".to_string(),
        PhpMixed::String(format!("{}/Fixtures/", directory_path)),
    );
    config_section.insert(
        "home".to_string(),
        PhpMixed::String(format!("{}/Fixtures", directory_path)),
    );
    config_section.insert(
        "bin-dir".to_string(),
        PhpMixed::String(format!("{}/Fixtures/bin", directory_path)),
    );
    config_section.insert("allow-plugins".to_string(), PhpMixed::Bool(true));
    let mut merged: IndexMap<String, PhpMixed> = IndexMap::new();
    merged.insert("config".to_string(), PhpMixed::Array(config_section));
    config.merge(&merged, Config::SOURCE_UNKNOWN);

    {
        let mut c = composer.borrow_mut();
        c.set_config(std::rc::Rc::new(std::cell::RefCell::new(config)));
        c.set_download_manager(std::rc::Rc::new(std::cell::RefCell::new(dm)));
        c.set_repository_manager(std::rc::Rc::new(std::cell::RefCell::new(
            repository_manager,
        )));
        c.set_installation_manager(std::rc::Rc::new(std::cell::RefCell::new(
            installation_manager,
        )));
        c.set_autoload_generator(autoload_generator.clone());
    }
    let real_dispatcher = std::rc::Rc::new(std::cell::RefCell::new(EventDispatcher::new(
        composer.upcast().downgrade(),
        io_dyn.clone(),
        None,
    )));
    composer.borrow_mut().set_event_dispatcher(real_dispatcher);
    composer.borrow_mut().set_package(
        RootPackageHandle::new(
            "dummy/root".to_string(),
            "1.0.0.0".to_string(),
            "1.0.0".to_string(),
        )
        .into(),
    );
    {
        let json_file =
            JsonFile::new(Platform::get_dev_null(), None, Some(io_dyn.clone())).unwrap();
        let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
            io_dyn.clone(),
        ))));
        let locker: std::rc::Rc<std::cell::RefCell<dyn LockerInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(Locker::new(
                io_dyn.clone(),
                json_file,
                locker_installation_manager(&io_dyn),
                "{}",
                process,
            )));
        composer.borrow_mut().set_locker(locker);
    }

    let pm = std::rc::Rc::new(std::cell::RefCell::new(PluginManager::new(
        io_dyn.clone(),
        composer.downgrade(),
        None,
        DisablePlugins::None,
    )));
    composer.borrow_mut().set_plugin_manager(pm.clone());

    SetUp {
        io,
        io_dyn,
        pm,
        autoload_generator,
        packages,
        repository,
        composer,
        _directory: directory,
    }
}

/// PHPUnit asserts `$plugins[$i]->version` etc.; the plugin entity lives in the PHP child, so
/// the property is read over RPC through the proxy's test helper.
fn plugin_property(
    plugin: &std::rc::Rc<std::cell::RefCell<dyn PluginInterface>>,
    name: &str,
) -> String {
    let plugin = plugin.borrow();
    let proxy = plugin
        .as_php_plugin_proxy()
        .expect("registered plugins are PHP-backed proxies");
    match proxy.__get_property(name).unwrap() {
        PhpMixed::String(s) => s,
        other => panic!("property `{name}` is not a string: {other:?}"),
    }
}

fn new_installer(set_up: &SetUp) -> PluginInstaller {
    PluginInstaller::new(
        set_up.io_dyn.clone(),
        set_up.composer.upcast().downgrade(),
        None,
        None,
    )
}

#[test]
fn test_install_new_plugin() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    let set_up = set_up();
    // PHP: $this->repository->getPackages() returns [].
    let installer = new_installer(&set_up);
    set_up.pm.borrow_mut().load_installed_plugins().unwrap();

    run(installer.install(&set_up.repository, set_up.packages[0].clone())).unwrap();

    let pm = set_up.pm.borrow();
    let plugins = pm.get_plugins();
    assert_eq!("installer-v1", plugin_property(&plugins[0], "version"));
    assert_eq!("activate v1\n", set_up.io.borrow().get_output());
}

#[test]
fn test_install_plugin_with_root_package_having_files_autoload() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    let set_up = set_up();
    // PHP: $this->repository->getPackages() returns [].
    let installer = new_installer(&set_up);
    set_up.pm.borrow_mut().load_installed_plugins().unwrap();

    set_up.autoload_generator.borrow_mut().set_dev_mode(true);
    let files_autoload = format!("{}/files_autoload_which_should_not_run.php", fixtures_dir());
    let mut autoload: IndexMap<String, PhpMixed> = IndexMap::new();
    autoload.insert(
        "files".to_string(),
        PhpMixed::List(vec![PhpMixed::String(files_autoload)]),
    );
    let root = set_up.composer.borrow().get_package().clone();
    root.set_autoload(autoload.clone());
    root.set_dev_autoload(autoload);

    run(installer.install(&set_up.repository, set_up.packages[0].clone())).unwrap();

    let pm = set_up.pm.borrow();
    let plugins = pm.get_plugins();
    assert_eq!("activate v1\n", set_up.io.borrow().get_output());
    assert_eq!("installer-v1", plugin_property(&plugins[0], "version"));
}

#[test]
fn test_install_multiple_plugins() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    let set_up = set_up();
    // PHP: $this->repository->getPackages() returns [$this->packages[3]].
    set_up
        .repository
        .borrow_mut()
        .add_package(set_up.packages[3].clone())
        .unwrap();
    let installer = new_installer(&set_up);
    set_up.pm.borrow_mut().load_installed_plugins().unwrap();

    run(installer.install(&set_up.repository, set_up.packages[3].clone())).unwrap();

    let pm = set_up.pm.borrow();
    let plugins = pm.get_plugins();
    assert_eq!("plugin1", plugin_property(&plugins[0], "name"));
    assert_eq!("installer-v4", plugin_property(&plugins[0], "version"));
    assert_eq!("plugin2", plugin_property(&plugins[1], "name"));
    assert_eq!("installer-v4", plugin_property(&plugins[1], "version"));
    assert_eq!(
        "activate v4-plugin1\nactivate v4-plugin2\n",
        set_up.io.borrow().get_output()
    );
}

#[test]
fn test_upgrade_with_new_class_name() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    let set_up = set_up();
    // PHP: getPackages returns [$this->packages[0]]; hasPackage answers (true, false), which a
    // real repository seeded with the initial package reproduces naturally.
    set_up
        .repository
        .borrow_mut()
        .add_package(set_up.packages[0].clone())
        .unwrap();
    let installer = new_installer(&set_up);
    set_up.pm.borrow_mut().load_installed_plugins().unwrap();

    run(installer.update(
        &set_up.repository,
        set_up.packages[0].clone(),
        set_up.packages[1].clone(),
    ))
    .unwrap();

    let pm = set_up.pm.borrow();
    let plugins = pm.get_plugins();
    // PHP: assertCount(1, $plugins); $plugins[1]->version — unset() keeps array keys, so the
    // remaining plugin sits at key 1. The Vec port reindexes; the remaining plugin is [0].
    assert_eq!(1, plugins.len());
    assert_eq!("installer-v2", plugin_property(&plugins[0], "version"));
    assert_eq!(
        "activate v1\ndeactivate v1\nactivate v2\n",
        set_up.io.borrow().get_output()
    );
}

#[test]
fn test_uninstall() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    let set_up = set_up();
    set_up
        .repository
        .borrow_mut()
        .add_package(set_up.packages[0].clone())
        .unwrap();
    let installer = new_installer(&set_up);
    set_up.pm.borrow_mut().load_installed_plugins().unwrap();

    run(installer.uninstall(&set_up.repository, set_up.packages[0].clone())).unwrap();

    let pm = set_up.pm.borrow();
    let plugins = pm.get_plugins();
    assert_eq!(0, plugins.len());
    assert_eq!(
        "activate v1\ndeactivate v1\nuninstall v1\n",
        set_up.io.borrow().get_output()
    );
}

#[test]
fn test_upgrade_with_same_class_name() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    let set_up = set_up();
    // PHP: getPackages returns [$this->packages[1]]; hasPackage answers (true, false).
    set_up
        .repository
        .borrow_mut()
        .add_package(set_up.packages[1].clone())
        .unwrap();
    let installer = new_installer(&set_up);
    set_up.pm.borrow_mut().load_installed_plugins().unwrap();

    run(installer.update(
        &set_up.repository,
        set_up.packages[1].clone(),
        set_up.packages[2].clone(),
    ))
    .unwrap();

    let pm = set_up.pm.borrow();
    let plugins = pm.get_plugins();
    assert_eq!("installer-v3", plugin_property(&plugins[0], "version"));
    assert_eq!(
        "activate v2\ndeactivate v2\nactivate v3\n",
        set_up.io.borrow().get_output()
    );
}

#[test]
fn test_register_plugin_only_one_time() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    let set_up = set_up();
    // PHP: $this->repository->getPackages() returns [].
    let installer = new_installer(&set_up);
    set_up.pm.borrow_mut().load_installed_plugins().unwrap();

    run(installer.install(&set_up.repository, set_up.packages[0].clone())).unwrap();
    run(installer.install(
        &set_up.repository,
        PackageInterfaceHandle::dup(&set_up.packages[0]),
    ))
    .unwrap();

    let pm = set_up.pm.borrow();
    let plugins = pm.get_plugins();
    assert_eq!(1, plugins.len());
    assert_eq!("installer-v1", plugin_property(&plugins[0], "version"));
    assert_eq!("activate v1\n", set_up.io.borrow().get_output());
}

// PluginManager::get_plugin_api_version returns a hardcoded constant
// (plugin_interface::PLUGIN_API_VERSION) with no seam to override it per-test the way PHP's
// `getMockBuilder(PluginManager::class)->onlyMethods(['getPluginApiVersion'])` does.
#[ignore = "Requires mocking getPluginApiVersion; PluginManager has no such seam (TODO(plugin))"]
#[test]
fn test_star_plugin_version_works_with_any_api_version() {
    // TODO(phase-d): requires mocking getPluginApiVersion; PluginManager has no such seam
    // (TODO(plugin)).
    todo!()
}

#[ignore = "Requires mocking getPluginApiVersion; PluginManager has no such seam (TODO(plugin))"]
#[test]
fn test_plugin_constraint_works_only_with_certain_api_version() {
    // TODO(phase-d): requires mocking getPluginApiVersion; PluginManager has no such seam
    // (TODO(plugin)).
    todo!()
}

#[ignore = "Requires mocking getPluginApiVersion; PluginManager has no such seam (TODO(plugin))"]
#[test]
fn test_plugin_range_constraints_work_only_with_certain_api_version() {
    // TODO(phase-d): requires mocking getPluginApiVersion; PluginManager has no such seam
    // (TODO(plugin)).
    todo!()
}

#[ignore = "get_plugin_capability never instantiates a capability class (TODO(plugin) in plugin/plugin_manager.rs); Capability::CommandProvider/BaseCommand runtime instantiation is unported"]
#[test]
fn test_command_provider_capability() {
    // TODO(phase-d): get_plugin_capability never instantiates a capability class (TODO(plugin)
    // in plugin/plugin_manager.rs); Capability::CommandProvider/BaseCommand runtime
    // instantiation is also unported.
    todo!()
}

// A hand-written stub is used in place of PHPUnit's
// `getMockBuilder('Composer\Plugin\PluginInterface')->getMock()`: PluginInterface's
// `as_capable()` default already returns None, exactly matching a bare (non-Capable) plugin
// mock.
#[derive(Debug)]
struct NoopPlugin;

impl PluginInterface for NoopPlugin {
    fn activate(
        &mut self,
        _composer: ComposerHandle,
        _io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    fn deactivate(
        &mut self,
        _composer: ComposerHandle,
        _io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    fn uninstall(
        &mut self,
        _composer: ComposerHandle,
        _io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    fn get_class_name(&self) -> String {
        "NoopPlugin".to_string()
    }
}

#[test]
fn test_incapable_plugin_is_correctly_detected() {
    let set_up = set_up();

    let plugin = NoopPlugin;
    let result = set_up
        .pm
        .borrow()
        .get_plugin_capability(&plugin, "Fake\\Ability", IndexMap::new())
        .unwrap();
    assert!(result.is_none());
}

#[ignore = "Requires runtime instantiation of Mock\\Capability via get_plugin_capability; not implemented (TODO(plugin))"]
#[test]
fn test_capability_implements_composer_plugin_api_class_and_is_constructed_with_args() {
    // TODO(phase-d): requires runtime instantiation of Mock\Capability via
    // get_plugin_capability; not implemented (TODO(plugin)).
    todo!()
}

// PluginManager::get_capability_implementation_class_name (via Capable::get_capabilities)
// resolves capability class names through an IndexMap<String, String>, so most of PHP's
// invalidImplementationClassNames data provider (null, 0, 1000, [1], [], stdClass) cannot be
// represented at all in the ported type — only the string entries ("", "   ") could be
// constructed. Per the phase-d rule against porting a subset of a data provider, this whole
// test must stay unported rather than dropping the non-string cases.
#[ignore = "Capable::get_capabilities is typed IndexMap<String, String>; most of the invalidImplementationClassNames data provider (null, 0, 1000, [1], [], stdClass) is not representable, and porting only the string cases would drop data-provider entries (TODO(phase-d))"]
#[test]
fn test_querying_with_invalid_capability_class_name_throws() {
    // TODO(phase-d): Capable::get_capabilities is typed IndexMap<String, String>; most of the
    // invalidImplementationClassNames data provider (null, 0, 1000, [1], [], stdClass) is not
    // representable, and porting only the string cases would drop data-provider entries.
    todo!()
}

// A hand-written stub plays the role of PHPUnit's
// `getMockBuilder('Composer\Test\Plugin\Mock\CapablePluginInterface')->getMock()`, i.e. a
// PluginInterface that is also Capable. `->expects($this->once())->method('getCapabilities')`
// is reproduced with a call counter asserted after the call.
#[derive(Debug)]
struct CapablePlugin {
    get_capabilities_calls: std::cell::RefCell<i64>,
}

impl PluginInterface for CapablePlugin {
    fn activate(
        &mut self,
        _composer: ComposerHandle,
        _io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    fn deactivate(
        &mut self,
        _composer: ComposerHandle,
        _io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    fn uninstall(
        &mut self,
        _composer: ComposerHandle,
        _io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    fn get_class_name(&self) -> String {
        "CapablePlugin".to_string()
    }

    fn as_capable(&self) -> Option<&dyn Capable> {
        Some(self)
    }
}

impl Capable for CapablePlugin {
    fn get_capabilities(&self) -> IndexMap<String, String> {
        *self.get_capabilities_calls.borrow_mut() += 1;
        IndexMap::new()
    }
}

#[test]
fn test_querying_non_provided_capability_returns_null_safely() {
    let set_up = set_up();

    let plugin = CapablePlugin {
        get_capabilities_calls: std::cell::RefCell::new(0),
    };

    let result = set_up
        .pm
        .borrow()
        .get_plugin_capability(
            &plugin,
            "Composer\\Plugin\\Capability\\MadeUpCapability",
            IndexMap::new(),
        )
        .unwrap();
    assert!(result.is_none());
    assert_eq!(1, *plugin.get_capabilities_calls.borrow());
}

#[ignore = "Requires runtime get_plugin_capability with PHP-class-name capability lookup (class_exists/instanceof checks are unported TODO(plugin)); not implemented"]
#[test]
fn test_querying_with_non_existing_or_wrong_capability_class_types_throws() {
    // TODO(phase-d): requires runtime get_plugin_capability with PHP-class-name capability lookup
    // (class_exists/instanceof checks are unported TODO(plugin)); not implemented.
    todo!()
}
