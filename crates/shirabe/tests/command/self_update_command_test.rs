//! ref: composer/tests/Composer/Test/Command/SelfUpdateCommandTest.php

use crate::test_case::{RunOptions, get_application_tester, init_temp_composer};
use serial_test::serial;
use shirabe_php_shim::PhpMixed;

/// ref: SelfUpdateCommandTest::setUp (portable part: initTempComposer; the composer-test.phar copy
/// is omitted because the phar fixture and Symfony Process are not ported).
fn set_up() -> crate::test_case::TearDown {
    init_temp_composer(None, None, None, true)
}

#[test]
#[serial]
#[ignore = "spawns `new Process([PHP_BINARY, $this->phar, 'self-update'])` running composer-test.phar \
            over HTTP; requires Symfony Process and the composer-test.phar fixture, neither ported"]
fn test_successful_update() {
    let _tear_down = set_up();

    // TODO(phase-d): spawns `new Process([PHP_BINARY, $this->phar, 'self-update'])` running
    // composer-test.phar over HTTP; requires Symfony Process and the composer-test.phar fixture,
    // neither ported.
    todo!()
}

#[test]
#[serial]
#[ignore = "spawns `new Process([PHP_BINARY, $this->phar, 'self-update', '2.4.0'])` running \
            composer-test.phar over HTTP; requires Symfony Process and the composer-test.phar \
            fixture, neither ported"]
fn test_update_to_specific_version() {
    let _tear_down = set_up();

    // TODO(phase-d): spawns `new Process([PHP_BINARY, $this->phar, 'self-update', '2.4.0'])`
    // running composer-test.phar over HTTP; requires Symfony Process and the composer-test.phar
    // fixture, neither ported.
    todo!()
}

#[test]
#[serial]
fn test_update_with_invalid_option_throws_exception() {
    let _tear_down = set_up();

    let mut app_tester = get_application_tester();
    let err = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("self-update")),
                (PhpMixed::from("invalid-option"), PhpMixed::from(true)),
            ],
            RunOptions::default(),
        )
        .expect_err("expected InvalidArgumentException for the unknown argument");

    assert!(
        err.to_string()
            .contains("The \"invalid-option\" argument does not exist."),
        "expected error about unknown argument, got: {:?}",
        err,
    );
}

#[test]
#[serial]
#[ignore = "spawns `new Process([PHP_BINARY, $this->phar, 'self-update', $option])` running \
            composer-test.phar over HTTP (data provider: --stable/--preview/--snapshot); requires \
            Symfony Process and the composer-test.phar fixture, neither ported"]
fn test_update_to_different_channel() {
    let _tear_down = set_up();

    // TODO(phase-d): spawns `new Process([PHP_BINARY, $this->phar, 'self-update', $option])`
    // running composer-test.phar over HTTP (data provider: --stable/--preview/--snapshot);
    // requires Symfony Process and the composer-test.phar fixture, neither ported.
    todo!()
}
