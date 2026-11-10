//! ref: composer/tests/Composer/Test/Command/ClearCacheCommandTest.php sha1:a97e8743c57def78525051f4cb4a8ea6edfee4b4

use crate::test_case::{RunOptions, get_application_tester};
use bstr::ByteSlice as _;
use shirabe::util::Platform;
use shirabe_symfony_console::input::{InputValue, ParameterName};

fn tear_down() {
    // --no-cache triggers the env to change so make sure the env is cleaned up after these tests run
    Platform::clear_env("COMPOSER_CACHE_DIR").unwrap();
}

struct TearDown;

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down();
    }
}

#[test]
#[serial_test::serial]
fn test_clear_cache_command_success() {
    let _tear_down = TearDown;

    let mut app_tester = get_application_tester();
    app_tester
        .run(
            vec![(
                ParameterName::of("command"),
                InputValue::from("clear-cache"),
            )],
            RunOptions::default(),
        )
        .unwrap();

    assert_eq!(0, app_tester.get_status_code());

    let output = app_tester.get_display();
    assert!(
        output.contains_str("All caches cleared."),
        "expected output to contain 'All caches cleared.', got: {:?}",
        output,
    );
}

#[test]
#[serial_test::serial]
fn test_clear_cache_command_with_option_garbage_collection() {
    let _tear_down = TearDown;

    let mut app_tester = get_application_tester();
    app_tester
        .run(
            vec![
                (
                    ParameterName::of("command"),
                    InputValue::from("clear-cache"),
                ),
                (ParameterName::of("--gc"), InputValue::Bool(true)),
            ],
            RunOptions::default(),
        )
        .unwrap();

    assert_eq!(0, app_tester.get_status_code());

    let output = app_tester.get_display();
    assert!(
        output.contains_str("All caches garbage-collected."),
        "expected output to contain 'All caches garbage-collected.', got: {:?}",
        output,
    );
}

#[test]
#[serial_test::serial]
fn test_clear_cache_command_with_option_no_cache() {
    let _tear_down = TearDown;

    let mut app_tester = get_application_tester();
    app_tester
        .run(
            vec![
                (
                    ParameterName::of("command"),
                    InputValue::from("clear-cache"),
                ),
                (ParameterName::of("--no-cache"), InputValue::Bool(true)),
            ],
            RunOptions::default(),
        )
        .unwrap();

    assert_eq!(0, app_tester.get_status_code());

    let output = app_tester.get_display();
    assert!(
        output.contains_str("Cache is not enabled"),
        "expected output to contain 'Cache is not enabled', got: {:?}",
        output,
    );
}
