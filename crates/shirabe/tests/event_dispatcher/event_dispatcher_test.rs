//! ref: composer/tests/Composer/Test/EventDispatcher/EventDispatcherTest.php

use crate::io_mock::{Expectation, get_io_mock};
use crate::process_executor_mock::{ProcessExecutorMockGuard, cmd, get_process_executor_mock};
use indexmap::IndexMap;
use serial_test::serial;
use shirabe::autoload::{AutoloadGeneratorInterface, ClassLoader};
use shirabe::composer::{ComposerHandle, PartialOrFullComposer};
use shirabe::config::Config;
use shirabe::dependency_resolver::Transaction;
use shirabe::dependency_resolver::operation::AnyOperation;
use shirabe::event_dispatcher::{Callable, EventDispatcher, EventInterface};
use shirabe::filter::PlatformRequirementFilterInterface;
use shirabe::installer::{InstallationManagerInterface, InstallerEvents, InstallerInterface};
use shirabe::io::IOInterface;
use shirabe::io::buffer_io::BufferIO;
use shirabe::io::io_interface;
use shirabe::package::{
    LockerInterface, PackageInterfaceHandle, RootPackageHandle, RootPackageInterfaceHandle,
};
use shirabe::repository::{
    InstalledArrayRepository, RepositoryInterfaceHandle, RepositoryManagerInterface,
};
use shirabe::script::Event as ScriptEvent;
use shirabe::script::ScriptEvents;
use shirabe::util::platform::Platform;
use shirabe::util::process_executor::{MockHandler, ProcessExecutor};
use shirabe_class_map_generator::class_map::ClassMap;
use shirabe_external_packages::symfony::console::output::output_interface;
use shirabe_php_shim::Catch as _;
use shirabe_php_shim::{PHP_EOL, PhpMixed};

fn tear_down() {
    Platform::clear_env("COMPOSER_SKIP_SCRIPTS");
    Platform::clear_env("PHP_BINARY");
}

struct TearDown;

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down();
    }
}

/// ref: EventDispatcherTest::createComposerInstance.
///
/// The PHP helper wires a RepositoryManager / AutoloadGenerator / InstallationManager so that the
/// autoloader-rebuild path (only reached for PHP-script and array callables) works. The tests ported
/// here drive only command-line / composer-script listeners, which never touch those collaborators,
/// so a minimal full Composer carrying a Config and a RootPackage is sufficient.
fn create_composer_instance() -> ComposerHandle {
    let composer = ComposerHandle::from_rc_unchecked(std::rc::Rc::new(std::cell::RefCell::new(
        PartialOrFullComposer::new_full(),
    )));
    let config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(true, None)));
    composer.borrow_mut().set_config(config);
    let package: RootPackageInterfaceHandle = RootPackageHandle::new(
        "foo".to_string(),
        "1.0.0.0".to_string(),
        "1.0.0".to_string(),
    )
    .into();
    composer.borrow_mut().set_package(package);
    composer
}

fn null_io() -> std::rc::Rc<std::cell::RefCell<dyn IOInterface>> {
    std::rc::Rc::new(std::cell::RefCell::new(shirabe::io::null_io::NullIO::new()))
}

/// Locates a `php` executable on PATH and points `PHP_BINARY` at it.
///
/// When the dispatcher runs a plain command-line listener it probes for the PHP interpreter (to
/// export `PHP_BINARY` for the child). PHP's own test suite always runs under an interpreter, so
/// `PHP_BINARY` is implicitly set; the Rust `PhpExecutableFinder` fallbacks that read the running
/// SAPI are unportable, so we seed `PHP_BINARY` the way the PHP runtime would. Tests that reach the
/// command-line branch must call this. Returns false (skipping the assertion) when no PHP is found.
fn ensure_php_binary() -> bool {
    if Platform::get_env("PHP_BINARY")
        .filter(|v| !v.is_empty())
        .is_some()
    {
        return true;
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("php");
        if candidate.is_file() {
            Platform::put_env("PHP_BINARY", &candidate.to_string_lossy());
            return true;
        }
    }
    false
}

fn buffer_io_verbose() -> std::rc::Rc<std::cell::RefCell<BufferIO>> {
    std::rc::Rc::new(std::cell::RefCell::new(
        BufferIO::new(String::new(), output_interface::VERBOSITY_VERBOSE, None).unwrap(),
    ))
}

/// Builds the EventDispatcher used by the script tests with a mocked `getListeners`, mirroring
/// `getMockBuilder(EventDispatcher)->onlyMethods(['getListeners'])`.
fn dispatcher_with_listeners(
    composer: &ComposerHandle,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    callback: Box<dyn Fn(&dyn EventInterface) -> Vec<Callable>>,
) -> EventDispatcher {
    let mut dispatcher = EventDispatcher::new(composer.upcast().downgrade(), io, Some(process));
    dispatcher.__set_get_listeners_override(callback);
    dispatcher
}

/// ref: EventDispatcherTest::getDispatcherStubForListenersTest — same mocked `getListeners`, but
/// constructed without a ProcessExecutor.
fn dispatcher_stub_for_listeners_test(
    composer: &ComposerHandle,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    listeners: Vec<&str>,
) -> EventDispatcher {
    let mut dispatcher = EventDispatcher::new(composer.upcast().downgrade(), io, None);
    dispatcher.__set_get_listeners_override(listeners_const(listeners));
    dispatcher
}

fn listeners_const(listeners: Vec<&str>) -> Box<dyn Fn(&dyn EventInterface) -> Vec<Callable>> {
    let listeners: Vec<String> = listeners.into_iter().map(|s| s.to_string()).collect();
    Box::new(move |_event| listeners.iter().cloned().map(Callable::String).collect())
}

#[test]
#[serial]
fn test_dispatcher_can_execute_single_command_line_script() {
    let _tear_down = TearDown;
    if !ensure_php_binary() {
        eprintln!("skipping: no php binary on PATH");
        return;
    }
    for command in ["phpunit", "echo foo", "echo -n foo"] {
        let (process, _process_guard): (_, ProcessExecutorMockGuard) =
            get_process_executor_mock(vec![cmd(command)], true, MockHandler::default());

        let composer = create_composer_instance();
        let mut dispatcher = dispatcher_with_listeners(
            &composer,
            null_io(),
            process,
            listeners_const(vec![command]),
        );

        dispatcher
            .dispatch_script(
                ScriptEvents::POST_INSTALL_CMD,
                false,
                vec![],
                IndexMap::new(),
            )
            .unwrap();
    }
}

#[test]
#[serial]
fn test_dispatcher_can_execute_composer_script_groups() {
    let _tear_down = TearDown;
    if !ensure_php_binary() {
        eprintln!("skipping: no php binary on PATH");
        return;
    }
    let (process, _process_guard) = get_process_executor_mock(
        vec![cmd("echo -n foo"), cmd("echo -n baz"), cmd("echo -n bar")],
        true,
        MockHandler::default(),
    );

    let composer = create_composer_instance();
    let io = buffer_io_verbose();
    let io_dyn: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io.clone();

    let callback: Box<dyn Fn(&dyn EventInterface) -> Vec<Callable>> =
        Box::new(|event| match event.get_name() {
            "root" => vec![Callable::String("@group".to_string())],
            "group" => vec![
                Callable::String("echo -n foo".to_string()),
                Callable::String("@subgroup".to_string()),
                Callable::String("echo -n bar".to_string()),
            ],
            "subgroup" => vec![Callable::String("echo -n baz".to_string())],
            _ => vec![],
        });
    let mut dispatcher = dispatcher_with_listeners(&composer, io_dyn.clone(), process, callback);

    let mut event = ScriptEvent::new(
        "root".to_string(),
        composer.downgrade(),
        io_dyn,
        false,
        vec![],
        IndexMap::new(),
    );
    dispatcher.dispatch(Some("root"), Some(&mut event)).unwrap();

    let expected = format!(
        "> root: @group{eol}> group: echo -n foo{eol}> group: @subgroup{eol}> subgroup: echo -n baz{eol}> group: echo -n bar{eol}",
        eol = PHP_EOL
    );
    assert_eq!(expected, io.borrow().get_output());
}

#[test]
#[serial]
fn test_recursion_in_scripts_names() {
    let _tear_down = TearDown;
    if !ensure_php_binary() {
        eprintln!("skipping: no php binary on PATH");
        return;
    }
    let (process, _process_guard) = get_process_executor_mock(
        vec![cmd(format!(
            "echo Hello {}",
            ProcessExecutor::escape("World")
        ))],
        true,
        MockHandler::default(),
    );

    let composer = create_composer_instance();
    let io = buffer_io_verbose();
    let io_dyn: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io.clone();

    let callback: Box<dyn Fn(&dyn EventInterface) -> Vec<Callable>> =
        Box::new(|event| match event.get_name() {
            "hello" => vec![Callable::String("echo Hello".to_string())],
            "helloWorld" => vec![Callable::String("@hello World".to_string())],
            _ => vec![],
        });
    let mut dispatcher = dispatcher_with_listeners(&composer, io_dyn.clone(), process, callback);

    let mut event = ScriptEvent::new(
        "helloWorld".to_string(),
        composer.downgrade(),
        io_dyn,
        false,
        vec![],
        IndexMap::new(),
    );
    dispatcher
        .dispatch(Some("helloWorld"), Some(&mut event))
        .unwrap();

    let expected = format!(
        "> helloWorld: @hello World{eol}> hello: echo Hello {world}{eol}",
        eol = PHP_EOL,
        world = ProcessExecutor::escape("World"),
    );
    assert_eq!(expected, io.borrow().get_output());
}

#[test]
#[serial]
fn test_dispatcher_detect_infinite_recursion() {
    let _tear_down = TearDown;
    let (process, _process_guard) =
        get_process_executor_mock(vec![], false, MockHandler::default());

    let composer = create_composer_instance();
    let io = null_io();

    let callback: Box<dyn Fn(&dyn EventInterface) -> Vec<Callable>> =
        Box::new(|event| match event.get_name() {
            "root" => vec![Callable::String("@recurse".to_string())],
            "recurse" => vec![Callable::String("@root".to_string())],
            _ => vec![],
        });
    let mut dispatcher = dispatcher_with_listeners(&composer, io.clone(), process, callback);

    let mut event = ScriptEvent::new(
        "root".to_string(),
        composer.downgrade(),
        io,
        false,
        vec![],
        IndexMap::new(),
    );
    let result = dispatcher.dispatch(Some("root"), Some(&mut event));
    let err = result.expect_err("infinite recursion must raise a RuntimeException");
    assert!(
        err.is_instanceof::<shirabe_php_shim::RuntimeException>(),
        "expected RuntimeException, got: {err:?}"
    );
}

#[test]
#[serial]
fn test_dispatcher_installer_events() {
    let _tear_down = TearDown;
    let (process, _process_guard) =
        get_process_executor_mock(vec![], false, MockHandler::default());

    let composer = create_composer_instance();
    let mut dispatcher =
        dispatcher_with_listeners(&composer, null_io(), process, listeners_const(vec![]));

    let transaction = Transaction::new(vec![], vec![]);

    dispatcher
        .dispatch_installer_event(
            InstallerEvents::PRE_OPERATIONS_EXEC,
            true,
            true,
            transaction,
        )
        .unwrap();
}

#[test]
#[serial]
fn test_dispatcher_doesnt_return_skipped_scripts() {
    let _tear_down = TearDown;
    Platform::put_env("COMPOSER_SKIP_SCRIPTS", "scriptName");

    let composer = create_composer_instance();
    let mut scripts: IndexMap<String, Vec<String>> = IndexMap::new();
    scripts.insert("scriptName".to_string(), vec!["scriptName".to_string()]);
    composer.borrow().get_package().set_scripts(scripts);

    let (process, _process_guard) =
        get_process_executor_mock(vec![], false, MockHandler::default());
    let mut dispatcher =
        EventDispatcher::new(composer.upcast().downgrade(), null_io(), Some(process));

    let mut event = ScriptEvent::new(
        "scriptName".to_string(),
        composer.downgrade(),
        null_io(),
        false,
        vec![],
        IndexMap::new(),
    );

    assert!(!dispatcher.has_event_listeners(&event));
    // keep `event` mutable use silenced after the assert (PHP passes by reference)
    let _ = &mut event;
}

// The remaining ignored tests use, as their listeners, static methods of the PHPUnit test class
// `Composer\Test\EventDispatcher\EventDispatcherTest` itself (or object-identity array
// callables). The PHP-script invocation path is implemented (execute_event_php_script sends a
// CallStaticMethod over the RPC channel), but the worker child process cannot load that test
// class: it extends PHPUnit\Framework\TestCase and phpunit is not part of composer/vendor.
// Making these pass needs a decision on how to provide the listener methods to the child (e.g. a
// stand-in fixture class with the same FQCN and method bodies), which is not a call to make
// unilaterally under the no-test-alteration rule.

#[test]
#[serial]
#[ignore = "listener `EventDispatcherTest::call` is a static method of the PHPUnit test class itself; the PHP worker cannot load it (extends PHPUnit\\Framework\\TestCase, phpunit absent from composer/vendor) — see the note above the ignored block"]
fn test_listener_exceptions_are_caught() {
    let _tear_down = TearDown;

    let (io_mock, _io_guard) = get_io_mock(io_interface::NORMAL).unwrap();
    let io_dyn: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io_mock.clone();

    let composer = create_composer_instance();
    let mut dispatcher = dispatcher_stub_for_listeners_test(
        &composer,
        io_dyn,
        vec!["Composer\\Test\\EventDispatcher\\EventDispatcherTest::call"],
    );

    io_mock
        .borrow_mut()
        .expects(
            vec![
                Expectation::text("> Composer\\Test\\EventDispatcher\\EventDispatcherTest::call"),
                Expectation::text(
                    "Script Composer\\Test\\EventDispatcher\\EventDispatcherTest::call handling the post-install-cmd event terminated with an exception",
                ),
            ],
            true,
        )
        .unwrap();

    let result = dispatcher.dispatch_script(
        ScriptEvents::POST_INSTALL_CMD,
        false,
        vec![],
        IndexMap::new(),
    );

    let e = result.expect_err("expected RuntimeException");
    assert!(
        e.is_instanceof::<shirabe_php_shim::RuntimeException>(),
        "got: {e:?}"
    );
}

// PHP mocks `Composer\Autoload\AutoloadGenerator` with onlyMethods(['buildPackageMap',
// 'parseAutoloads', 'createLoader', 'setDevMode']).
mockall::mock! {
    #[derive(Debug)]
    pub AutoloadGenerator {}
    impl AutoloadGeneratorInterface for AutoloadGenerator {
        fn set_dev_mode(&self, dev_mode: bool);
        fn set_class_map_authoritative(&self, class_map_authoritative: bool);
        fn set_apcu(&self, apcu: bool, apcu_prefix: Option<String>);
        fn set_run_scripts(&self, run_scripts: bool);
        fn set_dry_run(&self, dry_run: bool);
        fn set_platform_requirement_filter(
            &self,
            platform_requirement_filter: std::rc::Rc<dyn PlatformRequirementFilterInterface>,
        );
        fn dump(
            &self,
            config: &Config,
            local_repo: shirabe::repository::RepositoryInterfaceHandle,
            root_package: RootPackageInterfaceHandle,
            installation_manager: std::rc::Rc<std::cell::RefCell<dyn InstallationManagerInterface>>,
            target_dir: &str,
            scan_psr_packages: bool,
            suffix: Option<String>,
            locker: Option<std::rc::Rc<std::cell::RefCell<dyn LockerInterface>>>,
            strict_ambiguous: bool,
        ) -> anyhow::Result<ClassMap>;
        fn build_package_map(
            &self,
            installation_manager: std::rc::Rc<std::cell::RefCell<dyn InstallationManagerInterface>>,
            root_package: RootPackageInterfaceHandle,
            packages: Vec<PackageInterfaceHandle>,
        ) -> anyhow::Result<Vec<(PackageInterfaceHandle, Option<String>)>>;
        fn parse_autoloads(
            &self,
            package_map: Vec<(PackageInterfaceHandle, Option<String>)>,
            root_package: RootPackageInterfaceHandle,
            filtered_dev_packages: PhpMixed,
        ) -> IndexMap<String, PhpMixed>;
        fn create_loader<'a>(
            &self,
            autoloads: &IndexMap<String, PhpMixed>,
            vendor_dir: Option<String>,
        ) -> ClassLoader;
    }
}

// PHP mocks `Composer\Repository\RepositoryManager` with onlyMethods(['getLocalRepository']).
mockall::mock! {
    #[derive(Debug)]
    pub RepositoryManager {}
    impl RepositoryManagerInterface for RepositoryManager {
        fn get_local_repository(&self) -> RepositoryInterfaceHandle;
        fn get_repositories(&self) -> &Vec<RepositoryInterfaceHandle>;
        fn create_repository<'a>(
            &self,
            r#type: &str,
            config: IndexMap<String, PhpMixed>,
            name: Option<&'a str>,
        ) -> anyhow::Result<RepositoryInterfaceHandle>;
        fn add_repository(&mut self, repository: RepositoryInterfaceHandle);
        fn set_local_repository(&mut self, repository: RepositoryInterfaceHandle);
    }
}

// PHP mocks `Composer\Installer\InstallationManager` with disableOriginalConstructor().
mockall::mock! {
    #[derive(Debug)]
    pub InstallationManager {}
    impl InstallationManagerInterface for InstallationManager {
        fn add_installer(&self, installer: std::rc::Rc<dyn InstallerInterface>);
        fn remove_installer(&self, installer: &dyn InstallerInterface);
        fn disable_plugins(&mut self);
        fn is_package_installed(
            &mut self,
            repo: &shirabe::repository::InstalledRepositoryInterfaceHandle,
            package: PackageInterfaceHandle,
        ) -> anyhow::Result<bool>;
        fn ensure_binaries_presence(&mut self, package: PackageInterfaceHandle);
        fn execute(
            &self,
            repo: &shirabe::repository::InstalledRepositoryInterfaceHandle,
            operations: Vec<AnyOperation>,
            dev_mode: bool,
            run_scripts: bool,
            download_only: bool,
        ) -> anyhow::Result<()>;
        fn get_install_path(&self, package: PackageInterfaceHandle) -> Option<String>;
        fn set_output_progress(&mut self, output_progress: bool);
        fn notify_installs(&mut self, io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>);
    }
}

/// ref: EventDispatcherTest::testDispatcherPassDevModeToAutoloadGeneratorForScriptEvents
#[test]
#[serial]
fn test_dispatcher_pass_dev_mode_to_autoload_generator_for_script_events() {
    let _tear_down = TearDown;
    if !ensure_php_binary() {
        // The php-script listener path queries class_exists through the PHP worker.
        return;
    }

    // dataProvider provideDevModes
    for dev_mode in [true, false] {
        let composer = create_composer_instance();

        let mut generator = MockAutoloadGenerator::new();
        generator
            .expect_set_dev_mode()
            .with(mockall::predicate::eq(dev_mode))
            .times(1..)
            .return_const(());
        generator
            .expect_build_package_map()
            .returning(|_, _, _| Ok(Vec::new()));
        generator.expect_parse_autoloads().returning(|_, _, _| {
            [
                ("psr-0".to_string(), PhpMixed::List(vec![])),
                ("psr-4".to_string(), PhpMixed::List(vec![])),
                ("classmap".to_string(), PhpMixed::List(vec![])),
                ("files".to_string(), PhpMixed::List(vec![])),
                ("exclude-from-classmap".to_string(), PhpMixed::List(vec![])),
            ]
            .into_iter()
            .collect()
        });
        generator
            .expect_create_loader()
            .returning(|_, _| ClassLoader::new(None));
        composer
            .borrow_mut()
            .set_autoload_generator(std::rc::Rc::new(std::cell::RefCell::new(generator)));

        let package: RootPackageInterfaceHandle = RootPackageHandle::new(
            "foo".to_string(),
            "1.0.0.0".to_string(),
            "1.0.0".to_string(),
        )
        .into();
        let mut scripts: IndexMap<String, Vec<String>> = IndexMap::new();
        scripts.insert(
            "scriptName".to_string(),
            vec!["ClassName::testMethod".to_string()],
        );
        package.set_scripts(scripts);
        composer.borrow_mut().set_package(package);

        let mut repository_manager = MockRepositoryManager::new();
        repository_manager
            .expect_get_local_repository()
            .returning(|| RepositoryInterfaceHandle::new(InstalledArrayRepository::new().unwrap()));
        composer
            .borrow_mut()
            .set_repository_manager(std::rc::Rc::new(std::cell::RefCell::new(
                repository_manager,
            )));
        composer
            .borrow_mut()
            .set_installation_manager(std::rc::Rc::new(std::cell::RefCell::new(
                MockInstallationManager::new(),
            )));

        let (process, _process_guard) =
            get_process_executor_mock(vec![], false, MockHandler::default());
        let mut dispatcher =
            EventDispatcher::new(composer.upcast().downgrade(), null_io(), Some(process));

        let mut event = ScriptEvent::new(
            "scriptName".to_string(),
            composer.downgrade(),
            null_io(),
            dev_mode,
            Vec::new(),
            IndexMap::new(),
        );

        dispatcher
            .dispatch(Some("scriptName"), Some(&mut event))
            .unwrap();
    }
}

#[test]
#[serial]
#[ignore = "the object-method listeners are methods of the PHPUnit test class itself; invoking them sends a CallMethod for a phandle that has no counterpart in the worker, which cannot load that class (extends PHPUnit\\Framework\\TestCase, phpunit absent from composer/vendor) — see the note above the ignored block"]
fn test_dispatcher_remove_listener() {
    let _tear_down = TearDown;

    let composer = create_composer_instance();

    let mut repository_manager = MockRepositoryManager::new();
    repository_manager
        .expect_get_local_repository()
        .returning(|| RepositoryInterfaceHandle::new(InstalledArrayRepository::new().unwrap()));
    composer
        .borrow_mut()
        .set_repository_manager(std::rc::Rc::new(std::cell::RefCell::new(
            repository_manager,
        )));
    composer
        .borrow_mut()
        .set_installation_manager(std::rc::Rc::new(std::cell::RefCell::new(
            MockInstallationManager::new(),
        )));

    let (process, _process_guard) =
        get_process_executor_mock(vec![], false, MockHandler::default());
    let io = buffer_io_verbose();
    let io_dyn: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io.clone();
    let mut dispatcher = EventDispatcher::new(composer.upcast().downgrade(), io_dyn, Some(process));

    // PHP's `[$this, 'someMethod']` is an array callable whose object half is the test instance.
    // `Callable::PhpMethod` is the port's shape for an object-half callable: it carries the
    // cross-RPC identity `remove_listener` compares, and echoes `Class->method` like PHP does.
    let this = shirabe_php_rpc::PhpObjHandle {
        phandle: 1,
        class: "Composer\\Test\\EventDispatcher\\EventDispatcherTest".to_string(),
        implements: vec![],
    };
    let listener = Callable::PhpMethod(this.clone(), "someMethod".to_string());
    let listener2 = Callable::PhpMethod(this.clone(), "someMethod2".to_string());
    let listener3 = Callable::String(
        "Composer\\Test\\EventDispatcher\\EventDispatcherTest::someMethod".to_string(),
    );

    dispatcher.add_listener("ev1", listener.clone(), 0);
    dispatcher.add_listener("ev1", listener.clone(), 1);
    dispatcher.add_listener("ev1", listener2, 1);
    dispatcher.add_listener("ev1", listener3.clone(), 0);
    dispatcher.add_listener("ev2", listener3, 0);
    dispatcher.add_listener("ev2", listener, 0);
    dispatcher.dispatch(Some("ev1"), None).unwrap();
    dispatcher.dispatch(Some("ev2"), None).unwrap();

    let mut expected = format!(
        "> ev1: Composer\\Test\\EventDispatcher\\EventDispatcherTest->someMethod{eol}\
         > ev1: Composer\\Test\\EventDispatcher\\EventDispatcherTest->someMethod2{eol}\
         > ev1: Composer\\Test\\EventDispatcher\\EventDispatcherTest->someMethod{eol}\
         > ev1: Composer\\Test\\EventDispatcher\\EventDispatcherTest::someMethod{eol}\
         > ev2: Composer\\Test\\EventDispatcher\\EventDispatcherTest::someMethod{eol}\
         > ev2: Composer\\Test\\EventDispatcher\\EventDispatcherTest->someMethod{eol}",
        eol = PHP_EOL
    );
    assert_eq!(expected, io.borrow().get_output());

    dispatcher.remove_listener(&this);
    dispatcher.dispatch(Some("ev1"), None).unwrap();
    dispatcher.dispatch(Some("ev2"), None).unwrap();

    expected += &format!(
        "> ev1: Composer\\Test\\EventDispatcher\\EventDispatcherTest::someMethod{eol}\
         > ev2: Composer\\Test\\EventDispatcher\\EventDispatcherTest::someMethod{eol}",
        eol = PHP_EOL
    );
    assert_eq!(expected, io.borrow().get_output());
}

#[test]
#[serial]
#[ignore = "listener `EventDispatcherTest::someMethod` is a static method of the PHPUnit test class itself; the PHP worker cannot load it — see the note above the ignored block"]
fn test_dispatcher_can_execute_cli_and_php_in_same_event_script_stack() {
    let _tear_down = TearDown;

    let (process, _process_guard) = get_process_executor_mock(
        vec![cmd("echo -n foo"), cmd("echo -n bar")],
        true,
        MockHandler::default(),
    );

    let composer = create_composer_instance();
    let io = buffer_io_verbose();
    let io_dyn: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io.clone();

    let mut dispatcher = dispatcher_with_listeners(
        &composer,
        io_dyn,
        process,
        listeners_const(vec![
            "echo -n foo",
            "Composer\\Test\\EventDispatcher\\EventDispatcherTest::someMethod",
            "echo -n bar",
        ]),
    );

    dispatcher
        .dispatch_script(
            ScriptEvents::POST_INSTALL_CMD,
            false,
            vec![],
            IndexMap::new(),
        )
        .unwrap();

    let expected = format!(
        "> post-install-cmd: echo -n foo{eol}> post-install-cmd: Composer\\Test\\EventDispatcher\\EventDispatcherTest::someMethod{eol}> post-install-cmd: echo -n bar{eol}",
        eol = PHP_EOL
    );
    assert_eq!(expected, io.borrow().get_output());
}

#[test]
#[serial]
#[ignore = "listener `EventDispatcherTest::getTestEnv` is a static method of the PHPUnit test class itself; the PHP worker cannot load it — see the note above the ignored block"]
fn test_dispatcher_can_put_env() {
    let _tear_down = TearDown;

    let (process, _process_guard) =
        get_process_executor_mock(vec![], false, MockHandler::default());

    let composer = create_composer_instance();
    let io = buffer_io_verbose();
    let io_dyn: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io.clone();

    let mut dispatcher = dispatcher_with_listeners(
        &composer,
        io_dyn,
        process,
        listeners_const(vec![
            "@putenv ABC=123",
            "Composer\\Test\\EventDispatcher\\EventDispatcherTest::getTestEnv",
        ]),
    );

    dispatcher
        .dispatch_script(
            ScriptEvents::POST_INSTALL_CMD,
            false,
            vec![],
            IndexMap::new(),
        )
        .unwrap();

    let expected = format!(
        "> post-install-cmd: @putenv ABC=123{eol}> post-install-cmd: Composer\\Test\\EventDispatcher\\EventDispatcherTest::getTestEnv{eol}",
        eol = PHP_EOL
    );
    assert_eq!(expected, io.borrow().get_output());
}

#[test]
#[serial]
#[ignore = "listeners (createsVendorBinFolderChecksEnv*) are static methods of the PHPUnit test class itself; the PHP worker cannot load them — see the note above the ignored block"]
fn test_dispatcher_appends_dir_bin_on_path_for_every_listener() {
    let _tear_down = TearDown;

    let current_directory_bkp = Platform::get_cwd(false).unwrap();
    let composer_bin_dir_bkp = Platform::get_env("COMPOSER_BIN_DIR");
    // ref: __DIR__ of EventDispatcherTest.php, where the listeners create `vendor/bin`.
    let php_test_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../composer/tests/Composer/Test/EventDispatcher")
        .canonicalize()
        .unwrap();
    std::env::set_current_dir(&php_test_dir).unwrap();
    Platform::put_env(
        "COMPOSER_BIN_DIR",
        &format!("{}/vendor/bin", php_test_dir.display()),
    );

    let (process, _process_guard) =
        get_process_executor_mock(vec![], false, MockHandler::default());

    let composer = create_composer_instance();
    let io = buffer_io_verbose();
    let io_dyn: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io.clone();

    let mut dispatcher = dispatcher_with_listeners(
        &composer,
        io_dyn,
        process,
        listeners_const(vec![
            "Composer\\Test\\EventDispatcher\\EventDispatcherTest::createsVendorBinFolderChecksEnvDoesNotContainsBin",
            "Composer\\Test\\EventDispatcher\\EventDispatcherTest::createsVendorBinFolderChecksEnvContainsBin",
        ]),
    );

    dispatcher
        .dispatch_script(
            ScriptEvents::POST_INSTALL_CMD,
            false,
            vec![],
            IndexMap::new(),
        )
        .unwrap();
    std::fs::remove_dir(php_test_dir.join("vendor/bin")).unwrap();
    std::fs::remove_dir(php_test_dir.join("vendor")).unwrap();

    std::env::set_current_dir(&current_directory_bkp).unwrap();
    match composer_bin_dir_bkp {
        Some(dir) if !dir.is_empty() => Platform::put_env("COMPOSER_BIN_DIR", &dir),
        _ => Platform::clear_env("COMPOSER_BIN_DIR"),
    }
}

#[test]
#[serial]
fn test_dispatcher_support_for_additional_args() {
    let _tear_down = TearDown;
    if !ensure_php_binary() {
        eprintln!("skipping: no php binary on PATH");
        return;
    }

    let composer = create_composer_instance();
    let io = buffer_io_verbose();
    let io_dyn: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io.clone();

    // PHP obtains phpCmd via `new \ReflectionMethod($dispatcher, 'getPhpExecCommand')`;
    // __get_php_exec_command is that reflection seam. It only inspects the environment, so a
    // throwaway dispatcher yields the same value as the dispatcher under test.
    let php_cmd = EventDispatcher::new(composer.upcast().downgrade(), io_dyn.clone(), None)
        .__get_php_exec_command()
        .unwrap();

    let args = format!(
        "{} {} {}",
        ProcessExecutor::escape("ARG"),
        ProcessExecutor::escape("ARG2"),
        ProcessExecutor::escape("--arg"),
    );

    let (process, _process_guard) = get_process_executor_mock(
        vec![
            cmd("echo -n foo"),
            cmd(format!("{} foo.php {} then the rest", php_cmd, args)),
            cmd(format!("echo -n bar {}", args)),
        ],
        true,
        MockHandler::default(),
    );

    let mut dispatcher = dispatcher_with_listeners(
        &composer,
        io_dyn,
        process,
        listeners_const(vec![
            "echo -n foo @no_additional_args",
            "@php foo.php @additional_args then the rest",
            "echo -n bar",
        ]),
    );

    dispatcher
        .dispatch_script(
            ScriptEvents::POST_INSTALL_CMD,
            false,
            vec!["ARG".to_string(), "ARG2".to_string(), "--arg".to_string()],
            IndexMap::new(),
        )
        .unwrap();

    let expected = format!(
        "> post-install-cmd: echo -n foo{eol}> post-install-cmd: @php foo.php {args} then the rest{eol}> post-install-cmd: echo -n bar {args}{eol}",
        eol = PHP_EOL,
        args = args,
    );
    assert_eq!(expected, io.borrow().get_output());
}

#[test]
#[serial]
fn test_dispatcher_outputs_command() {
    let _tear_down = TearDown;

    let composer = create_composer_instance();
    let io = std::rc::Rc::new(std::cell::RefCell::new(crate::io_stub::IOStub::new()));
    let io_dyn: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io.clone();
    let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
        io_dyn.clone(),
    ))));

    let mut dispatcher = dispatcher_with_listeners(
        &composer,
        io_dyn,
        process,
        listeners_const(vec!["echo foo"]),
    );

    dispatcher
        .dispatch_script(
            ScriptEvents::POST_INSTALL_CMD,
            false,
            vec![],
            IndexMap::new(),
        )
        .unwrap();

    // ref: $io->expects($this->once())->method('writeError')->with('> echo foo')
    let write_error_messages: Vec<String> = io
        .borrow()
        .write_error_calls()
        .into_iter()
        .map(|(message, _newline)| message)
        .collect();
    assert_eq!(write_error_messages, vec!["> echo foo".to_string()]);

    // ref: $io->expects($this->once())->method('writeRaw')->with('foo'.PHP_EOL, false)
    assert_eq!(
        io.borrow().write_raw_calls(),
        vec![(format!("foo{PHP_EOL}"), false)]
    );
}

#[test]
#[serial]
fn test_dispatcher_outputs_error_on_failed_command() {
    let _tear_down = TearDown;

    let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(None)));
    let composer = create_composer_instance();
    let io = std::rc::Rc::new(std::cell::RefCell::new(
        BufferIO::new(String::new(), output_interface::VERBOSITY_NORMAL, None).unwrap(),
    ));
    let io_dyn: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io.clone();

    let code = "exit 1";
    let mut dispatcher =
        dispatcher_with_listeners(&composer, io_dyn, process, listeners_const(vec![code]));

    let result = dispatcher.dispatch_script(
        ScriptEvents::POST_INSTALL_CMD,
        false,
        vec![],
        IndexMap::new(),
    );

    let e = result.expect_err("expected ScriptExecutionException");
    assert!(e.to_string().contains("Error Output: "), "got: {e}");

    let expected = format!(
        "> exit 1{eol}Script exit 1 handling the post-install-cmd event returned with error code 1{eol}",
        eol = PHP_EOL
    );
    assert_eq!(expected, io.borrow().get_output());
}
