//! ref: composer/tests/Composer/Test/Command/DiagnoseCommandTest.php

use crate::test_case::{RunOptions, get_application_tester, init_temp_composer};
use serial_test::serial;
use shirabe::util::platform::Platform;
use shirabe_php_shim::PhpMixed;

#[test]
#[serial]
fn test_cmd_fail() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({ "name": "foo/bar", "description": "test pkg" })),
        None,
        None,
        false,
    );

    let mut app_tester = get_application_tester();
    app_tester
        .run(
            vec![(PhpMixed::from("command"), PhpMixed::from("diagnose"))],
            RunOptions::default(),
        )
        .unwrap();

    if Platform::get_env("COMPOSER_LOWEST_DEPS_TEST").as_deref() == Some("1") {
        assert!(app_tester.get_status_code() >= 1);
    } else {
        assert_eq!(1, app_tester.get_status_code());
    }

    let output = app_tester.get_display();
    assert!(output.contains(
        "Checking composer.json: <warning>WARNING</warning>
<warning>No license specified, it is recommended to do so. For closed-source software you may use \"proprietary\" as license.</warning>"
    ));

    assert!(output.contains(
        "Checking http connectivity to packagist: OK
Checking https connectivity to packagist: OK
Checking github.com rate limit: "
    ));

    drop(tear_down);
}

#[test]
#[serial]
#[ignore = "shirabe_php_shim::OPENSSL_VERSION_NUMBER is a hardcoded stub (0), which always trips \
            check_platform's `< 0x1000100f` TLSv1.1/1.2 support check regardless of the real \
            linked OpenSSL, forcing a non-zero exit code; diagnose also checks live http/https \
            connectivity to packagist and the github.com rate limit (as the PHP original does), \
            so the test additionally requires real network access"]
fn test_cmd_success() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "name": "foo/bar",
            "description": "test pkg",
            "license": "MIT",
        })),
        None,
        None,
        false,
    );

    let mut app_tester = get_application_tester();
    app_tester
        .run(
            vec![(PhpMixed::from("command"), PhpMixed::from("diagnose"))],
            RunOptions::default(),
        )
        .unwrap();

    if Platform::get_env("COMPOSER_LOWEST_DEPS_TEST").as_deref() != Some("1") {
        // assertCommandIsSuccessful
        assert_eq!(
            0,
            app_tester.get_status_code(),
            "{}",
            app_tester.get_display()
        );
    }

    let output = app_tester.get_display();
    assert!(output.contains("Checking composer.json: OK"));

    assert!(output.contains(
        "Checking http connectivity to packagist: OK
Checking https connectivity to packagist: OK
Checking github.com rate limit: "
    ));

    drop(tear_down);
}
