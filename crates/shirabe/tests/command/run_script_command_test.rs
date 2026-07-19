//! ref: composer/tests/Composer/Test/Command/RunScriptCommandTest.php

use crate::test_case::{RunOptions, get_application_tester, init_temp_composer};
use serial_test::serial;
use shirabe_php_shim::PhpMixed;

/// ref: RunScriptCommandTest::testDetectAndPassDevModeToEventAndToDispatching
///
/// The `getDevOptions` dataProvider drives four `(dev, noDev)` cases; for each, PHP asserts that the
/// `ScriptEvent` passed to `hasEventListeners` matches the script name AND its `isDevMode()` equals
/// the computed dev mode (`dev || !noDev`) -- the latter being the whole point of the test.
#[test]
#[ignore = "PHP asserts (a) via mocked hasEventListeners that the ScriptEvent has isDevMode() == \
            (dev || !no_dev) and (b) via mocked dispatchScript that it is called once with \
            ($script, $expectedDevMode, []). Neither expectation is expressible: (a) the \
            __set_get_listeners_override callback only sees &dyn EventInterface \
            (src/event_dispatcher/event.rs:49), which has no as_any/downcast seam to reach the \
            concrete ScriptEvent::is_dev_mode (src/script/event.rs:51) -- adding one is a \
            cross-cutting trait change over every event type, not a small test seam; (b) \
            dispatch_script is a concrete method with no call-recording seam, and letting the \
            real one run would execute listeners for real. The faithful body is therefore \
            inexpressible and is left as todo!()."]
fn test_detect_and_pass_dev_mode_to_event_and_to_dispatching() {
    // TODO(phase-d): PHP asserts (a) via mocked hasEventListeners that the ScriptEvent has
    // isDevMode() == (dev || !no_dev) and (b) via mocked dispatchScript that it is called once
    // with ($script, $expectedDevMode, []). Neither expectation is expressible: (a) the
    // __set_get_listeners_override callback only sees &dyn EventInterface
    // (src/event_dispatcher/event.rs:49), which has no as_any/downcast seam to reach the concrete
    // ScriptEvent::is_dev_mode (src/script/event.rs:51) -- adding one is a cross-cutting trait
    // change over every event type, not a small test seam; (b) dispatch_script is a concrete
    // method with no call-recording seam, and letting the real one run would execute listeners
    // for real. The faithful body is therefore inexpressible and is left as todo!().
    todo!()
}

/// ref: RunScriptCommandTest::testCanListScripts
#[test]
#[serial]
#[ignore = "Application::do_run registers composer.json scripts as commands; that path calls loader.register (class_loader.rs:288 -> spl_autoload_register at runtime.rs:231) which is a todo!() stub. With a 'scripts' key present, app_tester.run() panics there before the command executes"]
fn test_can_list_scripts() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "scripts": {
                "test": "@php test",
                "fix-cs": "php-cs-fixer fix",
            },
            "scripts-descriptions": {
                "fix-cs": "Run the codestyle fixer",
            },
        })),
        None,
        None,
        true,
    );

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("run-script")),
                (PhpMixed::from("--list"), PhpMixed::from(true)),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code, "assertCommandIsSuccessful");

    let output = app_tester.get_display();

    assert!(
        output.contains("Runs the test script as defined in composer.json"),
        "The default description for the test script should be printed"
    );
    assert!(
        output.contains("Run the codestyle fixer"),
        "The custom description for the fix-cs script should be printed"
    );

    drop(tear_down);
}

/// ref: RunScriptCommandTest::testCanDefineAliases
#[test]
#[serial]
#[ignore = "Application::do_run registers composer.json scripts as commands; that path calls loader.register (class_loader.rs:288 -> spl_autoload_register at runtime.rs:231) which is a todo!() stub. With a 'scripts' key present, app_tester.run() panics there before the command executes"]
fn test_can_define_aliases() {
    let expected_aliases = vec!["one", "two", "three"];

    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "scripts": {
                "test": "@php test",
            },
            "scripts-aliases": {
                "test": expected_aliases,
            },
        })),
        None,
        None,
        true,
    );

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("test")),
                (PhpMixed::from("--help"), PhpMixed::from(true)),
                (PhpMixed::from("--format"), PhpMixed::from("json")),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code, "assertCommandIsSuccessful");

    let output = app_tester.get_display();
    let array: serde_json::Value = serde_json::from_str(&output).unwrap();
    let mut actual_aliases: Vec<serde_json::Value> = array["usage"].as_array().unwrap().clone();
    actual_aliases.remove(0);

    let expected: Vec<serde_json::Value> = expected_aliases
        .iter()
        .map(|s| serde_json::Value::String(s.to_string()))
        .collect();
    assert_eq!(
        expected, actual_aliases,
        "The custom aliases for the test command should be printed"
    );

    drop(tear_down);
}

#[test]
#[ignore = "requires writing and executing a PHP-generated Symfony Command class (file_put_contents MyCommand.php) loaded via composer autoload; fundamentally unportable, no PHP runtime command loading in shirabe"]
fn test_execution_of_simple_symfony_command() {
    // TODO(phase-d): requires writing and executing a PHP-generated Symfony Command class
    // (file_put_contents MyCommand.php) loaded via composer autoload; fundamentally unportable, no
    // PHP runtime command loading in shirabe.
    todo!()
}

#[test]
#[ignore = "requires writing and executing a PHP-generated Symfony Command class (file_put_contents MyCommandWithDefinitions.php) loaded via composer autoload; fundamentally unportable, no PHP runtime command loading in shirabe"]
fn test_execution_of_symfony_command_with_configuration() {
    // TODO(phase-d): requires writing and executing a PHP-generated Symfony Command class
    // (file_put_contents MyCommandWithDefinitions.php) loaded via composer autoload; fundamentally
    // unportable, no PHP runtime command loading in shirabe.
    todo!()
}
