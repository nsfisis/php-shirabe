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

// The remaining ignored tests drive listeners that invoke PHP scripts (`Class::method`), require
// the autoloader rebuild of `make_autoloader` (an intentional no-op in the port), or rely on
// object-identity callables. None of those seams exist in the Rust port (the PHP-script
// invocation path is an unimplemented plugin-runtime `todo!`), so they remain ignored.

#[test]
#[ignore = "listener `EventDispatcherTest::call` is a PHP-script callable; dynamic static-method invocation requires the plugin runtime (execute_event_php_script is todo!())"]
fn test_listener_exceptions_are_caught() {
    let _tear_down = TearDown;
    // TODO(phase-d): listener `EventDispatcherTest::call` is a PHP-script callable; dynamic
    // static-method invocation requires the plugin runtime (execute_event_php_script is todo!())
    todo!()
}

#[test]
#[ignore = "EventDispatcher::make_autoloader (PHP makeAutoloader, called from doDispatch's script branches) is an intentional no-op in the port, so AutoloadGeneratorInterface::set_dev_mode is never invoked and a set_dev_mode spy would observe nothing"]
fn test_dispatcher_pass_dev_mode_to_autoload_generator_for_script_events() {
    let _tear_down = TearDown;
    // TODO(phase-d): the PHP test spies on AutoloadGenerator::setDevMode, which PHP calls from
    // makeAutoloader (invoked from doDispatch's script branches; it rebuilds and registers the
    // project autoloader — loader->unregister, setDevMode(event->isDevMode()), buildPackageMap,
    // parseAutoloads, createLoader->register — so that PHP-script listeners can be invoked). The
    // Rust EventDispatcher::make_autoloader is an intentional no-op (see its TODO(plugin)
    // marker), so set_dev_mode is never reached. A spy could be written against
    // `dyn AutoloadGeneratorInterface` once make_autoloader does the real work.
    todo!()
}

#[test]
#[ignore = "listeners are object-method array callables ([\\$this, 'someMethod']) invoked + removed by object identity; the array-callable invocation path is an unimplemented plugin-runtime stub"]
fn test_dispatcher_remove_listener() {
    let _tear_down = TearDown;
    // TODO(phase-d): listeners are object-method array callables ([$this, 'someMethod']) invoked
    // and removed by object identity; the array-callable invocation path is an unimplemented
    // plugin-runtime stub
    todo!()
}

#[test]
#[ignore = "mixes a PHP-script listener (EventDispatcherTest::someMethod) into the stack; dynamic static-method invocation requires the plugin runtime (execute_event_php_script is todo!())"]
fn test_dispatcher_can_execute_cli_and_php_in_same_event_script_stack() {
    let _tear_down = TearDown;
    // TODO(phase-d): mixes a PHP-script listener (EventDispatcherTest::someMethod) into the
    // stack; dynamic static-method invocation requires the plugin runtime
    // (execute_event_php_script is todo!())
    todo!()
}

#[test]
#[ignore = "second listener EventDispatcherTest::getTestEnv is a PHP-script callable; dynamic static-method invocation requires the plugin runtime (execute_event_php_script is todo!())"]
fn test_dispatcher_can_put_env() {
    let _tear_down = TearDown;
    // TODO(phase-d): second listener EventDispatcherTest::getTestEnv is a PHP-script callable;
    // dynamic static-method invocation requires the plugin runtime (execute_event_php_script is
    // todo!())
    todo!()
}

#[test]
#[ignore = "listeners are PHP-script callables (createsVendorBinFolderChecksEnv*) asserting on PATH; dynamic static-method invocation requires the plugin runtime (execute_event_php_script is todo!())"]
fn test_dispatcher_appends_dir_bin_on_path_for_every_listener() {
    let _tear_down = TearDown;
    // TODO(phase-d): listeners are PHP-script callables (createsVendorBinFolderChecksEnv*)
    // asserting on PATH; dynamic static-method invocation requires the plugin runtime
    // (execute_event_php_script is todo!())
    todo!()
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
