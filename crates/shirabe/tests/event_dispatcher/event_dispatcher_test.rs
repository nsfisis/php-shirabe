//! ref: composer/tests/Composer/Test/EventDispatcher/EventDispatcherTest.php

use crate::process_executor_mock::{ProcessExecutorMockGuard, cmd, get_process_executor_mock};
use indexmap::IndexMap;
use serial_test::serial;
use shirabe::composer::{ComposerHandle, PartialOrFullComposer};
use shirabe::config::Config;
use shirabe::dependency_resolver::Transaction;
use shirabe::event_dispatcher::{Callable, EventDispatcher, EventInterface};
use shirabe::installer::InstallerEvents;
use shirabe::io::IOInterface;
use shirabe::io::buffer_io::BufferIO;
use shirabe::package::{RootPackageHandle, RootPackageInterfaceHandle};
use shirabe::script::Event as ScriptEvent;
use shirabe::script::ScriptEvents;
use shirabe::util::platform::Platform;
use shirabe::util::process_executor::{MockHandler, ProcessExecutor};
use shirabe_external_packages::symfony::console::output::output_interface;
use shirabe_php_shim::PHP_EOL;
use std::cell::RefCell;
use std::rc::Rc;

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
    let composer =
        ComposerHandle::from_rc_unchecked(Rc::new(RefCell::new(PartialOrFullComposer::new_full())));
    let config = Rc::new(RefCell::new(Config::new(true, None)));
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

fn null_io() -> Rc<RefCell<dyn IOInterface>> {
    Rc::new(RefCell::new(shirabe::io::null_io::NullIO::new()))
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

fn buffer_io_verbose() -> Rc<RefCell<BufferIO>> {
    Rc::new(RefCell::new(
        BufferIO::new(String::new(), output_interface::VERBOSITY_VERBOSE, None).unwrap(),
    ))
}

/// Builds the EventDispatcher used by the script tests with a mocked `getListeners`, mirroring
/// `getMockBuilder(EventDispatcher)->onlyMethods(['getListeners'])`.
fn dispatcher_with_listeners(
    composer: &ComposerHandle,
    io: Rc<RefCell<dyn IOInterface>>,
    process: Rc<RefCell<ProcessExecutor>>,
    callback: Box<dyn Fn(&dyn EventInterface) -> Vec<Callable>>,
) -> EventDispatcher {
    let mut dispatcher = EventDispatcher::new(composer.upcast().downgrade(), io, Some(process));
    dispatcher.__set_get_listeners_override(callback);
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
    let io_dyn: Rc<RefCell<dyn IOInterface>> = io.clone();

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
    let io_dyn: Rc<RefCell<dyn IOInterface>> = io.clone();

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
        err.downcast_ref::<shirabe_php_shim::RuntimeException>()
            .is_some(),
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

// The remaining tests drive listeners that invoke PHP scripts (`Class::method`), require a
// PHPUnit-style spy on AutoloadGenerator::setDevMode, run real shell commands through an unmocked
// ProcessExecutor, or rely on ReflectionMethod / object-identity callables. None of those seams
// exist in the Rust port (the PHP-script invocation path is an unimplemented plugin-runtime `todo!`),
// so they remain ignored.

#[test]
#[ignore = "listener `EventDispatcherTest::call` is a PHP-script callable; dynamic static-method invocation requires the plugin runtime (execute_event_php_script is todo!())"]
fn test_listener_exceptions_are_caught() {
    let _tear_down = TearDown;
    todo!()
}

#[test]
#[ignore = "requires a PHPUnit spy on AutoloadGenerator::setDevMode plus Event::isDevMode mocking; no mock infrastructure exists"]
fn test_dispatcher_pass_dev_mode_to_autoload_generator_for_script_events() {
    let _tear_down = TearDown;
    todo!()
}

#[test]
#[ignore = "listeners are object-method array callables ([\\$this, 'someMethod']) invoked + removed by object identity; the array-callable invocation path is an unimplemented plugin-runtime stub"]
fn test_dispatcher_remove_listener() {
    let _tear_down = TearDown;
    todo!()
}

#[test]
#[ignore = "mixes a PHP-script listener (EventDispatcherTest::someMethod) into the stack; dynamic static-method invocation requires the plugin runtime (execute_event_php_script is todo!())"]
fn test_dispatcher_can_execute_cli_and_php_in_same_event_script_stack() {
    let _tear_down = TearDown;
    todo!()
}

#[test]
#[ignore = "second listener EventDispatcherTest::getTestEnv is a PHP-script callable; dynamic static-method invocation requires the plugin runtime (execute_event_php_script is todo!())"]
fn test_dispatcher_can_put_env() {
    let _tear_down = TearDown;
    todo!()
}

#[test]
#[ignore = "listeners are PHP-script callables (createsVendorBinFolderChecksEnv*) asserting on PATH; dynamic static-method invocation requires the plugin runtime (execute_event_php_script is todo!())"]
fn test_dispatcher_appends_dir_bin_on_path_for_every_listener() {
    let _tear_down = TearDown;
    todo!()
}

#[test]
#[ignore = "requires ReflectionMethod(getPhpExecCommand) and a real PHP binary to compute the expected @php command; getPhpExecCommand has no test seam"]
fn test_dispatcher_support_for_additional_args() {
    let _tear_down = TearDown;
    todo!()
}

#[test]
#[ignore = "uses an unmocked ProcessExecutor running a real `echo foo` and a PHPUnit IO spy on writeError/writeRaw; no real-shell-output IO mocking exists"]
fn test_dispatcher_outputs_command() {
    let _tear_down = TearDown;
    todo!()
}

#[test]
#[ignore = "uses an unmocked ProcessExecutor running a real `exit 1`; depends on real shell execution"]
fn test_dispatcher_outputs_error_on_failed_command() {
    let _tear_down = TearDown;
    todo!()
}
