//! ref: composer/tests/Composer/Test/Command/SelfUpdateCommandTest.php

use crate::test_case::{RunOptions, get_application_tester, init_temp_composer};
use indexmap::IndexMap;
use serial_test::serial;
use shirabe_php_shim::{PHP_BINARY, PhpMixed};
use shirabe_symfony_process::Process;

/// ref: SelfUpdateCommandTest::setUp. The `composer-test.phar` copy PHP also performs here lives in
/// `set_up_with_phar` instead, so the one test that never touches the phar is not blocked by the
/// missing fixture.
fn set_up() -> crate::test_case::TearDown {
    init_temp_composer(None, None, None, true)
}

/// ref: SelfUpdateCommandTest::setUp, including the `composer-test.phar` copy. Returns the tear-down
/// guard and `$this->phar`.
fn set_up_with_phar() -> (crate::test_case::TearDown, String) {
    let tear_down = set_up();
    let phar = tear_down.working_dir().join("composer.phar");
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../composer/tests/composer-test.phar"),
        &phar,
    )
    .unwrap();

    (tear_down, phar.display().to_string())
}

/// ref: SelfUpdateCommandTest::channelOptions
fn channel_options() -> Vec<(&'static str, &'static str)> {
    vec![
        ("--stable", "stable channel"),
        ("--preview", "preview channel"),
        ("--snapshot", "snapshot channel"),
    ]
}

#[test]
#[serial]
#[ignore = "composer-test.phar is built by AllFunctionalTest::testBuildPhar via bin/compile, which has no equivalent here, so the fixture set_up_with_phar copies can never exist"]
fn test_successful_update() {
    let (_tear_down, phar) = set_up_with_phar();

    if shirabe::composer::VERSION != concat!("@package_version", "@") {
        eprintln!(
            "skipping: On releases this test can fail to upgrade as we are already on latest version"
        );
        return;
    }

    let mut app_tester = Process::new(
        vec![PHP_BINARY.to_string(), phar, "self-update".to_string()],
        None,
        None,
        PhpMixed::Null,
        None,
    )
    .unwrap();
    let status = app_tester.run(None, IndexMap::new()).unwrap();
    assert_eq!(0, status, "{}", app_tester.get_error_output().unwrap());

    assert!(
        app_tester
            .get_output()
            .unwrap()
            .contains("Upgrading to version")
    );
}

#[test]
#[serial]
#[ignore = "composer-test.phar is built by AllFunctionalTest::testBuildPhar via bin/compile, which has no equivalent here, so the fixture set_up_with_phar copies can never exist"]
fn test_update_to_specific_version() {
    let (_tear_down, phar) = set_up_with_phar();

    let mut app_tester = Process::new(
        vec![
            PHP_BINARY.to_string(),
            phar,
            "self-update".to_string(),
            "2.4.0".to_string(),
        ],
        None,
        None,
        PhpMixed::Null,
        None,
    )
    .unwrap();
    let status = app_tester.run(None, IndexMap::new()).unwrap();
    assert_eq!(0, status, "{}", app_tester.get_error_output().unwrap());

    assert!(
        app_tester
            .get_output()
            .unwrap()
            .contains("Upgrading to version 2.4.0")
    );
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
#[ignore = "composer-test.phar is built by AllFunctionalTest::testBuildPhar via bin/compile, which has no equivalent here, so the fixture set_up_with_phar copies can never exist"]
fn test_update_to_different_channel() {
    for (option, expected_output) in channel_options() {
        let (_tear_down, phar) = set_up_with_phar();

        if shirabe::composer::VERSION != concat!("@package_version", "@")
            && ["--stable", "--preview"].contains(&option)
        {
            eprintln!(
                "skipping: On releases this test can fail to upgrade as we are already on latest version"
            );
            continue;
        }

        let mut app_tester = Process::new(
            vec![
                PHP_BINARY.to_string(),
                phar,
                "self-update".to_string(),
                option.to_string(),
            ],
            None,
            None,
            PhpMixed::Null,
            None,
        )
        .unwrap();
        let status = app_tester.run(None, IndexMap::new()).unwrap();
        assert_eq!(0, status, "{}", app_tester.get_error_output().unwrap());

        let output = app_tester.get_output().unwrap();
        assert!(output.contains("Upgrading to version"));
        assert!(output.contains(expected_output));
    }
}
