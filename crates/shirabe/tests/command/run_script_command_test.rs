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
#[ignore = "PHP mocks RunScriptCommand itself (onlyMethods incl. requireComposer -> a composer \
            whose EventDispatcher is a hasEventListeners/dispatchScript recording mock) and \
            drives run() with mocked Input/Output. The Rust RunScriptCommand has no \
            requireComposer override seam and Input/Output are concrete types, so the mocked \
            harness is inexpressible; the event-side isDevMode downcast now exists \
            (EventInterface::as_any), but that alone does not unblock the test."]
fn test_detect_and_pass_dev_mode_to_event_and_to_dispatching() {
    // TODO(phase-d): PHP mocks RunScriptCommand itself (onlyMethods incl. requireComposer -> a
    // composer whose EventDispatcher is a hasEventListeners/dispatchScript recording mock) and
    // drives run() with mocked Input/Output. The Rust RunScriptCommand has no requireComposer
    // override seam and Input/Output are concrete types, so the mocked harness is
    // inexpressible; the event-side isDevMode downcast now exists (EventInterface::as_any), but
    // that alone does not unblock the test.
    todo!()
}

/// ref: RunScriptCommandTest::testCanListScripts
#[test]
#[serial]
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
#[ignore = "the test invokes the script name as a top-level composer command, which requires Application::do_run to import the user's PHP Command class (MyCommand.php) as a live application command (todo!() in application.rs: the worker-side console application exists, but the import arm is not wired to it), and the command's output would go to the worker's inherited stdio, which the in-process application tester cannot capture"]
fn test_execution_of_simple_symfony_command() {
    // TODO(phase-d): the test invokes the script name as a top-level composer command, which
    // requires Application::do_run to import the user's PHP Command class (MyCommand.php) as a
    // live application command (todo!() in application.rs: the worker-side console application exists, but the import arm is not wired to it), and the worker writes to inherited stdio the tester cannot capture.
    todo!()
}

#[test]
#[ignore = "the test invokes the script name as a top-level composer command, which requires Application::do_run to import the user's PHP Command class (MyCommandWithDefinitions.php) as a live application command (todo!() in application.rs: the worker-side console application exists, but the import arm is not wired to it), and the command's output would go to the worker's inherited stdio, which the in-process application tester cannot capture"]
fn test_execution_of_symfony_command_with_configuration() {
    // TODO(phase-d): the test invokes the script name as a top-level composer command, which
    // requires Application::do_run to import the user's PHP Command class (MyCommandWithDefinitions.php)
    // as a live application command (todo!() in application.rs: the worker-side console application exists, but the import arm is not wired to it), and the worker writes to inherited stdio the tester cannot capture.
    todo!()
}
