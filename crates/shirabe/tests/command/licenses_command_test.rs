//! ref: composer/tests/Composer/Test/Command/LicensesCommandTest.php

use crate::test_case::{
    RunOptions, TearDown, create_composer_lock, create_installed_json, get_application_tester,
    get_complete_package, init_temp_composer,
};
use serial_test::serial;
use shirabe::package::handle::PackageInterfaceHandle;
use shirabe_php_shim::PhpMixed;

/// ref: LicensesCommandTest::setUp
fn set_up() -> TearDown {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "name": "test/pkg",
            "version": "1.2.3",
            "license": "MIT",
            "require": {
                "first/pkg": "^2.0",
                "second/pkg": "3.*",
                "third/pkg": "^1.3",
            },
            "require-dev": {
                "dev/pkg": "~2.0",
            },
        })),
        None,
        None,
        true,
    );

    let first = get_complete_package("first/pkg", "2.3.4");
    first.set_license(vec!["MIT".to_string()]);

    let second = get_complete_package("second/pkg", "3.4.0");
    second.set_license(vec!["LGPL-2.0-only".to_string()]);
    second.set_homepage("https://example.org".to_string());

    let third = get_complete_package("third/pkg", "1.5.4");

    let dev = get_complete_package("dev/pkg", "2.3.4.5");
    dev.set_license(vec!["MIT".to_string()]);

    let packages: Vec<PackageInterfaceHandle> = vec![first.into(), second.into(), third.into()];
    let dev_packages: Vec<PackageInterfaceHandle> = vec![dev.into()];

    create_installed_json(&packages, &dev_packages, true);
    create_composer_lock(&packages, &dev_packages);

    tear_down
}

/// Asserts each non-blank line of `display` matches the regex built from `expected[i]`
/// (`implode("\s+", expected[i])` in PHP).
fn assert_lines(display: &str, expected: &[Vec<&str>]) {
    let mut i = 0usize;
    for line in display.split('\n') {
        if line.trim().is_empty() {
            i += 1;
            continue;
        }
        assert!(i < expected.len(), "Got more output lines than expected");
        let pattern = expected[i]
            .iter()
            .map(|s| regex::escape(s))
            .collect::<Vec<_>>()
            .join(r"\s+");
        let re = regex::Regex::new(&pattern).unwrap();
        assert!(
            re.is_match(line),
            "line {} {:?} did not match {:?}",
            i,
            line,
            pattern
        );
        i += 1;
    }
}

#[test]
#[serial]
fn test_basic_run() {
    let _tear_down = set_up();

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![(PhpMixed::from("command"), PhpMixed::from("license"))],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code);

    let expected = vec![
        vec!["Name:", "test/pkg"],
        vec!["Version:", "1.2.3"],
        vec!["Licenses:", "MIT"],
        vec!["Dependencies:"],
        vec![],
        vec!["Name", "Version", "Licenses"],
        vec!["dev/pkg", "2.3.4.5", "MIT"],
        vec!["first/pkg", "2.3.4", "MIT"],
        vec!["second/pkg", "3.4.0", "LGPL-2.0-only"],
        vec!["third/pkg", "1.5.4", "none"],
    ];
    assert_lines(&app_tester.get_display(), &expected);
}

#[test]
#[serial]
fn test_no_dev() {
    let _tear_down = set_up();

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("license")),
                (PhpMixed::from("--no-dev"), PhpMixed::from(true)),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code);

    let expected = vec![
        vec!["Name:", "test/pkg"],
        vec!["Version:", "1.2.3"],
        vec!["Licenses:", "MIT"],
        vec!["Dependencies:"],
        vec![],
        vec!["Name", "Version", "Licenses"],
        vec!["first/pkg", "2.3.4", "MIT"],
        vec!["second/pkg", "3.4.0", "LGPL-2.0-only"],
        vec!["third/pkg", "1.5.4", "none"],
    ];
    assert_lines(&app_tester.get_display(), &expected);
}

#[test]
#[serial]
fn test_format_json() {
    let _tear_down = set_up();

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("license")),
                (PhpMixed::from("--format"), PhpMixed::from("json")),
            ],
            RunOptions {
                capture_stderr_separately: true,
                ..RunOptions::default()
            },
        )
        .unwrap();
    assert_eq!(0, status_code);

    let expected = serde_json::json!({
        "name": "test/pkg",
        "version": "1.2.3",
        "license": ["MIT"],
        "dependencies": {
            "dev/pkg": { "version": "2.3.4.5", "license": ["MIT"] },
            "first/pkg": { "version": "2.3.4", "license": ["MIT"] },
            "second/pkg": { "version": "3.4.0", "license": ["LGPL-2.0-only"] },
            "third/pkg": { "version": "1.5.4", "license": [] },
        },
    });

    let actual: serde_json::Value = serde_json::from_str(&app_tester.get_display()).unwrap();
    assert_eq!(expected, actual);
}

#[test]
#[serial]
fn test_format_summary() {
    let _tear_down = set_up();

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("license")),
                (PhpMixed::from("--format"), PhpMixed::from("summary")),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code);

    let expected = [
        ("-", "-"),
        ("License", "Number of dependencies"),
        ("-", "-"),
        ("MIT", "2"),
        ("LGPL-2.0-only", "1"),
        ("none", "1"),
        ("-", "-"),
    ];

    let display = app_tester.get_display();
    let lines: Vec<&str> = display.split('\n').collect();
    for (i, (key, value)) in expected.iter().enumerate() {
        let pattern = format!(r"{}\s+{}", regex::escape(key), regex::escape(value));
        let re = regex::Regex::new(&pattern).unwrap();
        assert!(
            re.is_match(lines[i]),
            "line {} {:?} did not match {:?}",
            i,
            lines[i],
            pattern
        );
    }
}

#[test]
#[serial]
fn test_format_unknown() {
    let _tear_down = set_up();

    let mut app_tester = get_application_tester();
    let result = app_tester.run(
        vec![
            (PhpMixed::from("command"), PhpMixed::from("license")),
            (PhpMixed::from("--format"), PhpMixed::from("unknown")),
        ],
        RunOptions::default(),
    );
    assert!(
        result.is_err(),
        "expected a RuntimeException, got {:?}",
        result
    );
}

#[test]
#[serial]
fn test_locked() {
    let _tear_down = set_up();

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("license")),
                (PhpMixed::from("--locked"), PhpMixed::from(true)),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code);

    let expected = vec![
        vec!["Name:", "test/pkg"],
        vec!["Version:", "1.2.3"],
        vec!["Licenses:", "MIT"],
        vec!["Dependencies:"],
        vec![],
        vec!["Name", "Version", "Licenses"],
        vec!["dev/pkg", "2.3.4.5", "MIT"],
        vec!["first/pkg", "2.3.4", "MIT"],
        vec!["second/pkg", "3.4.0", "LGPL-2.0-only"],
        vec!["third/pkg", "1.5.4", "none"],
    ];
    assert_lines(&app_tester.get_display(), &expected);
}

#[test]
#[serial]
fn test_locked_no_dev() {
    let _tear_down = set_up();

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("license")),
                (PhpMixed::from("--locked"), PhpMixed::from(true)),
                (PhpMixed::from("--no-dev"), PhpMixed::from(true)),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code);

    let expected = vec![
        vec!["Name:", "test/pkg"],
        vec!["Version:", "1.2.3"],
        vec!["Licenses:", "MIT"],
        vec!["Dependencies:"],
        vec![],
        vec!["Name", "Version", "Licenses"],
        vec!["first/pkg", "2.3.4", "MIT"],
        vec!["second/pkg", "3.4.0", "LGPL-2.0-only"],
        vec!["third/pkg", "1.5.4", "none"],
    ];
    assert_lines(&app_tester.get_display(), &expected);
}

#[test]
#[serial]
fn test_locked_without_lock_file() {
    let _tear_down = set_up();

    // Remove the lock file
    let _ = std::fs::remove_file("./composer.lock");

    let mut app_tester = get_application_tester();
    let result = app_tester.run(
        vec![
            (PhpMixed::from("command"), PhpMixed::from("license")),
            (PhpMixed::from("--locked"), PhpMixed::from(true)),
        ],
        RunOptions::default(),
    );
    let err = result.expect_err("expected an UnexpectedValueException");
    assert!(
        err.to_string()
            .contains("Valid composer.json and composer.lock files are required to run this command with --locked"),
        "unexpected error: {}",
        err
    );
}
