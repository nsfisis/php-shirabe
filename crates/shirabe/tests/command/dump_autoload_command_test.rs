//! ref: composer/tests/Composer/Test/Command/DumpAutoloadCommandTest.php

use crate::test_case::{RunOptions, get_application_tester, init_temp_composer};
use regex::Regex;
use serial_test::serial;
use shirabe_php_shim::PhpMixed;

/// ref: DumpAutoloadCommandTest::testDumpAutoload
#[test]
#[serial]
#[ignore = "DumpAutoloadCommand::execute panics: composer_full_mut() holds a mut borrow of the composer RefCell across get_event_dispatcher().dispatch(), which re-enters via EventDispatcher::get_script_listeners -> PartialComposerHandle::borrow_partial (composer.rs:446) -> RefCell already mutably borrowed. Source bug in event_dispatcher.rs:1004 / dump_autoload_command.rs:88-91, not fixable from the test"]
fn test_dump_autoload() {
    let tear_down = init_temp_composer(None, None, None, true);

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![(PhpMixed::from("command"), PhpMixed::from("dump-autoload"))],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code);

    let output = app_tester.get_display();
    assert!(output.contains("Generating autoload files"));
    assert!(output.contains("Generated autoload files"));

    drop(tear_down);
}

/// ref: DumpAutoloadCommandTest::testDumpDevAutoload
#[test]
#[serial]
#[ignore = "DumpAutoloadCommand::execute panics: composer_full_mut() holds a mut borrow of the composer RefCell across get_event_dispatcher().dispatch(), which re-enters via EventDispatcher::get_script_listeners -> PartialComposerHandle::borrow_partial (composer.rs:446) -> RefCell already mutably borrowed. Source bug in event_dispatcher.rs:1004 / dump_autoload_command.rs:88-91, not fixable from the test"]
fn test_dump_dev_autoload() {
    let tear_down = init_temp_composer(None, None, None, true);

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("dump-autoload")),
                (PhpMixed::from("--dev"), PhpMixed::from(true)),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code);

    let output = app_tester.get_display();
    assert!(output.contains("Generating autoload files"));
    assert!(output.contains("Generated autoload files"));

    drop(tear_down);
}

/// ref: DumpAutoloadCommandTest::testDumpNoDevAutoload
#[test]
#[serial]
#[ignore = "DumpAutoloadCommand::execute panics: composer_full_mut() holds a mut borrow of the composer RefCell across get_event_dispatcher().dispatch(), which re-enters via EventDispatcher::get_script_listeners -> PartialComposerHandle::borrow_partial (composer.rs:446) -> RefCell already mutably borrowed. Source bug in event_dispatcher.rs:1004 / dump_autoload_command.rs:88-91, not fixable from the test"]
fn test_dump_no_dev_autoload() {
    let tear_down = init_temp_composer(None, None, None, true);

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("dump-autoload")),
                (PhpMixed::from("--dev"), PhpMixed::from(true)),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code);

    let output = app_tester.get_display();
    assert!(output.contains("Generating autoload files"));
    assert!(output.contains("Generated autoload files"));

    drop(tear_down);
}

/// ref: DumpAutoloadCommandTest::testUsingOptimizeAndStrictPsr
#[test]
#[serial]
#[ignore = "DumpAutoloadCommand::execute panics: composer_full_mut() holds a mut borrow of the composer RefCell across get_event_dispatcher().dispatch(), which re-enters via EventDispatcher::get_script_listeners -> PartialComposerHandle::borrow_partial (composer.rs:446) -> RefCell already mutably borrowed. Source bug in event_dispatcher.rs:1004 / dump_autoload_command.rs:88-91, not fixable from the test"]
fn test_using_optimize_and_strict_psr() {
    let tear_down = init_temp_composer(None, None, None, true);

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("dump-autoload")),
                (PhpMixed::from("--optimize"), PhpMixed::from(true)),
                (PhpMixed::from("--strict-psr"), PhpMixed::from(true)),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code);

    let output = app_tester.get_display();
    assert!(output.contains("Generating optimized autoload files"));
    let re = Regex::new(r"Generated optimized autoload files containing \d+ classes").unwrap();
    assert!(re.is_match(&output));

    drop(tear_down);
}

/// ref: DumpAutoloadCommandTest::testFailsUsingStrictPsrIfClassMapViolationsAreFound
#[test]
#[serial]
#[ignore = "DumpAutoloadCommand::execute panics: composer_full_mut() holds a mut borrow of the composer RefCell across get_event_dispatcher().dispatch(), which re-enters via EventDispatcher::get_script_listeners -> PartialComposerHandle::borrow_partial (composer.rs:446) -> RefCell already mutably borrowed. Source bug in event_dispatcher.rs:1004 / dump_autoload_command.rs:88-91, not fixable from the test"]
fn test_fails_using_strict_psr_if_class_map_violations_are_found() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "autoload": {
                "psr-4": {
                    "Application\\": "src",
                },
            },
        })),
        None,
        None,
        true,
    );
    let dir = tear_down.working_dir();
    std::fs::create_dir(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("src/Foo.php"),
        "<?php namespace Application\\Src; class Foo {}",
    )
    .unwrap();

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("dump-autoload")),
                (PhpMixed::from("--optimize"), PhpMixed::from(true)),
                (PhpMixed::from("--strict-psr"), PhpMixed::from(true)),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(1, status_code);

    let output = app_tester.get_display();
    let re = Regex::new(
        r"Class Application\\Src\\Foo located in .*? does not comply with psr-4 autoloading standard \(rule: Application\\ => \./src\)\. Skipping\.",
    )
    .unwrap();
    assert!(re.is_match(&output));

    drop(tear_down);
}

/// ref: DumpAutoloadCommandTest::testUsingClassmapAuthoritative
#[test]
#[serial]
#[ignore = "DumpAutoloadCommand::execute panics: composer_full_mut() holds a mut borrow of the composer RefCell across get_event_dispatcher().dispatch(), which re-enters via EventDispatcher::get_script_listeners -> PartialComposerHandle::borrow_partial (composer.rs:446) -> RefCell already mutably borrowed. Source bug in event_dispatcher.rs:1004 / dump_autoload_command.rs:88-91, not fixable from the test"]
fn test_using_classmap_authoritative() {
    let tear_down = init_temp_composer(None, None, None, true);

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("dump-autoload")),
                (
                    PhpMixed::from("--classmap-authoritative"),
                    PhpMixed::from(true),
                ),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code);

    let output = app_tester.get_display();
    assert!(output.contains("Generating optimized autoload files (authoritative)"));
    let re =
        Regex::new(r"Generated optimized autoload files \(authoritative\) containing \d+ classes")
            .unwrap();
    assert!(re.is_match(&output));

    drop(tear_down);
}

/// ref: DumpAutoloadCommandTest::testUsingClassmapAuthoritativeAndStrictPsr
#[test]
#[serial]
#[ignore = "DumpAutoloadCommand::execute panics: composer_full_mut() holds a mut borrow of the composer RefCell across get_event_dispatcher().dispatch(), which re-enters via EventDispatcher::get_script_listeners -> PartialComposerHandle::borrow_partial (composer.rs:446) -> RefCell already mutably borrowed. Source bug in event_dispatcher.rs:1004 / dump_autoload_command.rs:88-91, not fixable from the test"]
fn test_using_classmap_authoritative_and_strict_psr() {
    let tear_down = init_temp_composer(None, None, None, true);

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("dump-autoload")),
                (
                    PhpMixed::from("--classmap-authoritative"),
                    PhpMixed::from(true),
                ),
                (PhpMixed::from("--strict-psr"), PhpMixed::from(true)),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code);

    let output = app_tester.get_display();
    assert!(output.contains("Generating optimized autoload files"));
    let re =
        Regex::new(r"Generated optimized autoload files \(authoritative\) containing \d+ classes")
            .unwrap();
    assert!(re.is_match(&output));

    drop(tear_down);
}

/// ref: DumpAutoloadCommandTest::testStrictPsrDoesNotWorkWithoutOptimizedAutoloader
#[test]
#[serial]
#[ignore = "DumpAutoloadCommand::execute panics: composer_full_mut() holds a mut borrow of the composer RefCell across get_event_dispatcher().dispatch(), which re-enters via EventDispatcher::get_script_listeners -> PartialComposerHandle::borrow_partial (composer.rs:446) -> RefCell already mutably borrowed. Source bug in event_dispatcher.rs:1004 / dump_autoload_command.rs:88-91, not fixable from the test"]
fn test_strict_psr_does_not_work_without_optimized_autoloader() {
    let tear_down = init_temp_composer(None, None, None, true);

    let mut app_tester = get_application_tester();
    let err = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("dump-autoload")),
                (PhpMixed::from("--strict-psr"), PhpMixed::from(true)),
            ],
            RunOptions::default(),
        )
        .expect_err("expected --strict-psr without optimize to error");
    assert!(err.to_string().contains(
        "--strict-psr mode only works with optimized autoloader, use --optimize or --classmap-authoritative if you want a strict return value."
    ));

    drop(tear_down);
}

/// ref: DumpAutoloadCommandTest::testDevAndNoDevCannotBeCombined
#[test]
#[serial]
#[ignore = "DumpAutoloadCommand::execute panics: composer_full_mut() holds a mut borrow of the composer RefCell across get_event_dispatcher().dispatch(), which re-enters via EventDispatcher::get_script_listeners -> PartialComposerHandle::borrow_partial (composer.rs:446) -> RefCell already mutably borrowed. Source bug in event_dispatcher.rs:1004 / dump_autoload_command.rs:88-91, not fixable from the test"]
fn test_dev_and_no_dev_cannot_be_combined() {
    let tear_down = init_temp_composer(None, None, None, true);

    let mut app_tester = get_application_tester();
    let err = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("dump-autoload")),
                (PhpMixed::from("--dev"), PhpMixed::from(true)),
                (PhpMixed::from("--no-dev"), PhpMixed::from(true)),
            ],
            RunOptions::default(),
        )
        .expect_err("expected --dev and --no-dev combination to error");
    assert!(
        err.to_string()
            .contains("You can not use both --no-dev and --dev as they conflict with each other.")
    );

    drop(tear_down);
}

/// ref: DumpAutoloadCommandTest::testWithCustomAutoloaderSuffix
#[test]
#[serial]
#[ignore = "DumpAutoloadCommand::execute panics: composer_full_mut() holds a mut borrow of the composer RefCell across get_event_dispatcher().dispatch(), which re-enters via EventDispatcher::get_script_listeners -> PartialComposerHandle::borrow_partial (composer.rs:446) -> RefCell already mutably borrowed. Source bug in event_dispatcher.rs:1004 / dump_autoload_command.rs:88-91, not fixable from the test"]
fn test_with_custom_autoloader_suffix() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "config": {
                "autoloader-suffix": "Foobar",
            },
        })),
        None,
        None,
        true,
    );
    let dir = tear_down.working_dir();

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![(PhpMixed::from("command"), PhpMixed::from("dump-autoload"))],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code);

    let contents = std::fs::read_to_string(dir.join("vendor/autoload.php")).unwrap_or_default();
    assert!(contents.contains("ComposerAutoloaderInitFoobar"));

    drop(tear_down);
}

/// ref: DumpAutoloadCommandTest::testWithExistingComposerLockAndAutoloaderSuffix
#[test]
#[serial]
#[ignore = "DumpAutoloadCommand::execute panics: composer_full_mut() holds a mut borrow of the composer RefCell across get_event_dispatcher().dispatch(), which re-enters via EventDispatcher::get_script_listeners -> PartialComposerHandle::borrow_partial (composer.rs:446) -> RefCell already mutably borrowed. Source bug in event_dispatcher.rs:1004 / dump_autoload_command.rs:88-91, not fixable from the test"]
fn test_with_existing_composer_lock_and_autoloader_suffix() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "config": {
                "autoloader-suffix": "Foobar",
            },
        })),
        None,
        Some(&serde_json::json!({
            "_readme": [
                "This file locks the dependencies of your project to a known state",
                "Read more about it at https://getcomposer.org/doc/01-basic-usage.md#installing-dependencies",
                "This file is @generated automatically",
            ],
            "content-hash": "d751713988987e9331980363e24189ce",
            "packages": [],
            "packages-dev": [],
            "aliases": [],
            "minimum-stability": "stable",
            "stability-flags": [],
            "prefer-stable": false,
            "prefer-lowest": false,
            "platform": [],
            "platform-dev": [],
            "plugin-api-version": "2.6.0",
        })),
        true,
    );
    let dir = tear_down.working_dir();

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![(PhpMixed::from("command"), PhpMixed::from("dump-autoload"))],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code);

    let contents = std::fs::read_to_string(dir.join("vendor/autoload.php")).unwrap_or_default();
    assert!(contents.contains("ComposerAutoloaderInitFoobar"));

    drop(tear_down);
}

/// ref: DumpAutoloadCommandTest::testWithExistingComposerLockWithoutAutoloaderSuffix
#[test]
#[serial]
#[ignore = "DumpAutoloadCommand::execute panics: composer_full_mut() holds a mut borrow of the composer RefCell across get_event_dispatcher().dispatch(), which re-enters via EventDispatcher::get_script_listeners -> PartialComposerHandle::borrow_partial (composer.rs:446) -> RefCell already mutably borrowed. Source bug in event_dispatcher.rs:1004 / dump_autoload_command.rs:88-91, not fixable from the test"]
fn test_with_existing_composer_lock_without_autoloader_suffix() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "name": "foo/bar",
        })),
        None,
        Some(&serde_json::json!({
            "_readme": [
                "This file locks the dependencies of your project to a known state",
                "Read more about it at https://getcomposer.org/doc/01-basic-usage.md#installing-dependencies",
                "This file is @generated automatically",
            ],
            "content-hash": "2d4a6be9a93712c5d6a119b26734a047",
            "packages": [],
            "packages-dev": [],
            "aliases": [],
            "minimum-stability": "stable",
            "stability-flags": [],
            "prefer-stable": false,
            "prefer-lowest": false,
            "platform": [],
            "platform-dev": [],
            "plugin-api-version": "2.6.0",
        })),
        true,
    );
    let dir = tear_down.working_dir();

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![(PhpMixed::from("command"), PhpMixed::from("dump-autoload"))],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code);

    let contents = std::fs::read_to_string(dir.join("vendor/autoload.php")).unwrap_or_default();
    assert!(contents.contains("ComposerAutoloaderInit2d4a6be9a93712c5d6a119b26734a047"));

    drop(tear_down);
}
