//! ref: composer/tests/Composer/Test/Command/DiagnoseCommandTest.php

use crate::test_case::{RunOptions, get_application_tester, init_temp_composer};
use serial_test::serial;
use shirabe::util::platform::Platform;
use shirabe_symfony_console::input::InputValue;
use shirabe_symfony_console::input::ParameterName;

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
            vec![(ParameterName::of("command"), InputValue::from("diagnose"))],
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
#[ignore = "the audit covers composer/composer at the version reported by Composer::VERSION, and \
            packagist has advisories against 2.9.7, so whenever the advisories API answers, \
            diagnose warns and exits 1 where the test expects 0; upstream Composer 2.9.7 reports \
            the same advisories"]
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
            vec![(ParameterName::of("command"), InputValue::from("diagnose"))],
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
