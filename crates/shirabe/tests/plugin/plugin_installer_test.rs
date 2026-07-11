//! ref: composer/tests/Composer/Test/Plugin/PluginInstallerTest.php

use crate::config_stub::ConfigStubBuilder;
use indexmap::IndexMap;
use shirabe::composer::{Composer, ComposerHandle, PartialOrFullComposer};
use shirabe::config::Config;
use shirabe::factory::DisablePlugins;
use shirabe::installer::InstallationManager;
use shirabe::io::IOInterface;
use shirabe::io::buffer_io::BufferIO;
use shirabe::json::JsonFile;
use shirabe::package::{Locker, LockerInterface};
use shirabe::plugin::plugin_interface::PluginInterface;
use shirabe::plugin::{Capable, PluginManager};
use shirabe::util::Platform;
use shirabe::util::http_downloader::HttpDownloader;
use shirabe::util::r#loop::Loop;
use shirabe::util::process_executor::ProcessExecutor;
use shirabe_external_packages::symfony::console::output::output_interface::VERBOSITY_NORMAL;
use shirabe_php_shim::PhpMixed;

/// Equivalent to PHP setUp()'s `new InstallationManager(...)`, used only to satisfy
/// `Locker::new`'s constructor argument; it is never exercised by the currently-portable
/// tests below.
fn installation_manager(
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
    #[allow(dead_code)]
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    pm: std::rc::Rc<std::cell::RefCell<PluginManager>>,
    // Keeps the Composer alive; PluginManager only holds a weak back-reference to it.
    #[allow(dead_code)]
    composer: ComposerHandle,
}

/// Builds a `Composer` the way PHP's setUp() does (config with `allow-plugins => true`
/// and a `Locker` backed by /dev/null) and constructs a `PluginManager` from it.
///
/// PHP's setUp() additionally mocks DownloadManager/RepositoryManager/InstallationManager/
/// EventDispatcher, loads 8 plugin-vN fixture packages, and creates a temp fixtures
/// directory. None of that is reproduced here: every test that would exercise it depends on
/// `PluginManager::register_package` actually instantiating a plugin class, which is an
/// unported runtime concern (`TODO(plugin)` in `plugin/plugin_manager.rs`) — those tests stay
/// `#[ignore]` below. Only the tests that call `PluginManager::get_plugin_capability` directly
/// with a hand-built plugin object are portable, and they need nothing more than `pm` itself.
fn set_up() -> SetUp {
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = std::rc::Rc::new(
        std::cell::RefCell::new(BufferIO::new(String::new(), VERBOSITY_NORMAL, None).unwrap()),
    );

    let config = ConfigStubBuilder::new()
        .with("allow-plugins", PhpMixed::Bool(true))
        .build_shared();

    let json_file = JsonFile::new(Platform::get_dev_null(), None, Some(io.clone())).unwrap();
    let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
        io.clone(),
    ))));
    let locker: std::rc::Rc<std::cell::RefCell<dyn LockerInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(Locker::new(
            io.clone(),
            json_file,
            installation_manager(&io),
            "{}",
            process,
        )));

    let mut composer = Composer::new();
    composer.set_config(config);
    composer.set_locker(locker);

    let composer = ComposerHandle::from_rc_unchecked(std::rc::Rc::new(std::cell::RefCell::new(
        PartialOrFullComposer::Full(composer),
    )));

    let pm = PluginManager::new(io.clone(), composer.downgrade(), None, DisablePlugins::None);

    SetUp {
        io,
        pm: std::rc::Rc::new(std::cell::RefCell::new(pm)),
        composer,
    }
}

/// PHP's tearDown() removes the temp fixtures directory created by setUp(); `set_up` above
/// creates no such directory, so there is nothing to clean up.
fn tear_down() {}

struct TearDown;

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down();
    }
}

// The plugin system requires the PHP runtime to load and instantiate plugin classes.
// `PluginInstaller::install`/`update` never call `PluginManager::register_package` (the calls
// are commented out in installer/plugin_installer.rs, TODO(plugin)), and `register_package`
// itself never instantiates a plugin class or calls `add_plugin` (TODO(plugin) in
// plugin/plugin_manager.rs). So `PluginManager::get_plugins()` can never contain the plugin
// instances these tests assert on.
#[ignore = "PluginInstaller and runtime loading of fixture plugin PHP classes (plugin-v1) are not implemented (TODO(plugin))"]
#[test]
fn test_install_new_plugin() {
    // TODO(phase-d): PluginInstaller and runtime loading of fixture plugin PHP classes (plugin-v1)
    // are not implemented (TODO(plugin)).
    todo!()
}

#[ignore = "PluginInstaller and runtime loading of fixture plugin PHP classes are not implemented (TODO(plugin))"]
#[test]
fn test_install_plugin_with_root_package_having_files_autoload() {
    // TODO(phase-d): PluginInstaller and runtime loading of fixture plugin PHP classes are not
    // implemented (TODO(plugin)).
    todo!()
}

#[ignore = "PluginInstaller and runtime loading of fixture plugin PHP classes (plugin-v4) are not implemented (TODO(plugin))"]
#[test]
fn test_install_multiple_plugins() {
    // TODO(phase-d): PluginInstaller and runtime loading of fixture plugin PHP classes (plugin-v4)
    // are not implemented (TODO(plugin)).
    todo!()
}

#[ignore = "PluginInstaller.update and runtime plugin class loading/deactivation are not implemented (TODO(plugin))"]
#[test]
fn test_upgrade_with_new_class_name() {
    // TODO(phase-d): PluginInstaller.update and runtime plugin class loading/deactivation are not
    // implemented (TODO(plugin)).
    todo!()
}

#[ignore = "PluginInstaller.uninstall and runtime plugin class loading/uninstall hook are not implemented (TODO(plugin))"]
#[test]
fn test_uninstall() {
    // TODO(phase-d): PluginInstaller.uninstall and runtime plugin class loading/uninstall hook are
    // not implemented (TODO(plugin)).
    todo!()
}

#[ignore = "PluginInstaller.update and runtime plugin class loading/deactivation are not implemented (TODO(plugin))"]
#[test]
fn test_upgrade_with_same_class_name() {
    // TODO(phase-d): PluginInstaller.update and runtime plugin class loading/deactivation are not
    // implemented (TODO(plugin)).
    todo!()
}

#[ignore = "PluginInstaller and runtime loading of fixture plugin PHP classes are not implemented (TODO(plugin))"]
#[test]
fn test_register_plugin_only_one_time() {
    // TODO(phase-d): PluginInstaller and runtime loading of fixture plugin PHP classes are not
    // implemented (TODO(plugin)).
    todo!()
}

// PluginManager::register_package's version-constraint check against composer-plugin-api is
// fully ported and does gate loading correctly, but `getPluginApiVersion()` returns a hardcoded
// constant (plugin_interface::PLUGIN_API_VERSION) with no seam to override it per-test the way
// PHP's `getMockBuilder(PluginManager::class)->onlyMethods(['getPluginApiVersion'])` does, and
// even a matching version can never produce a registered plugin (register_package's
// instantiate-and-add_plugin step is an unported TODO(plugin) stub). Both blockers must be
// resolved together; a partial port (e.g. only the count==0 branches) would drop assertions the
// test relies on, which is disallowed.
#[ignore = "Requires mocking getPluginApiVersion and runtime loading of fixture plugin PHP classes; not implemented (TODO(plugin))"]
#[test]
fn test_star_plugin_version_works_with_any_api_version() {
    // TODO(phase-d): requires mocking getPluginApiVersion and runtime loading of fixture plugin PHP
    // classes; not implemented (TODO(plugin)).
    todo!()
}

#[ignore = "Requires mocking getPluginApiVersion and runtime loading of fixture plugin PHP classes; not implemented (TODO(plugin))"]
#[test]
fn test_plugin_constraint_works_only_with_certain_api_version() {
    // TODO(phase-d): requires mocking getPluginApiVersion and runtime loading of fixture plugin PHP
    // classes; not implemented (TODO(plugin)).
    todo!()
}

#[ignore = "Requires mocking getPluginApiVersion and runtime loading of fixture plugin PHP classes; not implemented (TODO(plugin))"]
#[test]
fn test_plugin_range_constraints_work_only_with_certain_api_version() {
    // TODO(phase-d): requires mocking getPluginApiVersion and runtime loading of fixture plugin PHP
    // classes; not implemented (TODO(plugin)).
    todo!()
}

#[ignore = "get_plugin_capabilities requires a registered plugin, which register_package never produces (TODO(plugin) in plugin/plugin_manager.rs); Capability::CommandProvider/BaseCommand runtime instantiation is also unported"]
#[test]
fn test_command_provider_capability() {
    // TODO(phase-d): get_plugin_capabilities requires a registered plugin, which register_package
    // never produces (TODO(plugin) in plugin/plugin_manager.rs); Capability::CommandProvider/
    // BaseCommand runtime instantiation is also unported.
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
        _composer: &ComposerHandle,
        _io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) {
    }

    fn deactivate(
        &mut self,
        _composer: &ComposerHandle,
        _io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) {
    }

    fn uninstall(
        &mut self,
        _composer: &ComposerHandle,
        _io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) {
    }
}

#[test]
fn test_incapable_plugin_is_correctly_detected() {
    let set_up = set_up();
    let _tear_down = TearDown;

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
        _composer: &ComposerHandle,
        _io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) {
    }

    fn deactivate(
        &mut self,
        _composer: &ComposerHandle,
        _io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) {
    }

    fn uninstall(
        &mut self,
        _composer: &ComposerHandle,
        _io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) {
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
    let _tear_down = TearDown;

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
