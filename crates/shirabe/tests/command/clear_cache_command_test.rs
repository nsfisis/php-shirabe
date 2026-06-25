//! ref: composer/tests/Composer/Test/Command/ClearCacheCommandTest.php

use crate::test_case::{RunOptions, get_application_tester};
use serial_test::serial;
use shirabe::util::platform::Platform;
use shirabe_php_shim::PhpMixed;

fn tear_down() {
    // --no-cache triggers the env to change so make sure the env is cleaned up after these tests run
    Platform::clear_env("COMPOSER_CACHE_DIR");
}

struct TearDown;

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down();
    }
}

#[test]
#[serial]
fn test_clear_cache_command_success() {
    let _tear_down = TearDown;

    let mut app_tester = get_application_tester();
    app_tester
        .run(
            vec![(PhpMixed::from("command"), PhpMixed::from("clear-cache"))],
            RunOptions::default(),
        )
        .unwrap();

    assert_eq!(0, app_tester.get_status_code());

    let output = app_tester.get_display();
    assert!(
        output.contains("All caches cleared."),
        "expected output to contain 'All caches cleared.', got: {:?}",
        output,
    );
}

#[test]
#[serial]
fn test_clear_cache_command_with_option_garbage_collection() {
    let _tear_down = TearDown;

    let mut app_tester = get_application_tester();
    app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("clear-cache")),
                (PhpMixed::from("--gc"), PhpMixed::Bool(true)),
            ],
            RunOptions::default(),
        )
        .unwrap();

    assert_eq!(0, app_tester.get_status_code());

    let output = app_tester.get_display();
    assert!(
        output.contains("All caches garbage-collected."),
        "expected output to contain 'All caches garbage-collected.', got: {:?}",
        output,
    );
}

#[test]
#[serial]
fn test_clear_cache_command_with_option_no_cache() {
    let _tear_down = TearDown;

    let mut app_tester = get_application_tester();
    app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("clear-cache")),
                (PhpMixed::from("--no-cache"), PhpMixed::Bool(true)),
            ],
            RunOptions::default(),
        )
        .unwrap();

    assert_eq!(0, app_tester.get_status_code());

    let output = app_tester.get_display();
    assert!(
        output.contains("Cache is not enabled"),
        "expected output to contain 'Cache is not enabled', got: {:?}",
        output,
    );
}
