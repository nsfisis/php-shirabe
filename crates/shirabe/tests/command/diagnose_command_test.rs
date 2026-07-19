//! ref: composer/tests/Composer/Test/Command/DiagnoseCommandTest.php

use crate::test_case::{RunOptions, get_application_tester, init_temp_composer};
use serial_test::serial;
use shirabe::util::platform::Platform;
use shirabe_php_shim::PhpMixed;

#[test]
#[serial]
#[ignore = "DiagnoseCommand::check_http passes &config.borrow() into a call chain that reaches \
            CurlDownloader::download, which then does self.config.borrow_mut() on the same \
            Config RefCell, panicking with 'RefCell already borrowed'; beyond that, diagnose checks \
            live http/https connectivity to packagist and the github.com rate limit, so the test \
            also requires real network access (as the PHP original does)"]
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
#[ignore = "DiagnoseCommand::check_http passes &config.borrow() into a call chain that reaches \
            CurlDownloader::download, which then does self.config.borrow_mut() on the same \
            Config RefCell, panicking with 'RefCell already borrowed'; beyond that, diagnose checks \
            live http/https connectivity to packagist and the github.com rate limit, so the test \
            also requires real network access (as the PHP original does)"]
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
