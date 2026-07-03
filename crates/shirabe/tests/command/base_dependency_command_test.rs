//! ref: composer/tests/Composer/Test/Command/BaseDependencyCommandTest.php

use crate::test_case::{
    RunOptions, create_composer_lock, create_installed_json, get_application_tester, get_package,
    get_version_constraint, init_temp_composer,
};
use serial_test::serial;
use shirabe::package::Link;
use shirabe::package::handle::PackageInterfaceHandle;
use shirabe_php_shim::PhpMixed;
use shirabe_semver::constraint::{AnyConstraint, MatchAllConstraint, MultiConstraint};

/// ref: TestCase::trimLines — strip trailing spaces from each line, then trim the whole string.
fn trim_lines(s: &str) -> String {
    let stripped = s
        .lines()
        .map(|line| line.trim_end_matches(' '))
        .collect::<Vec<_>>()
        .join("\n");
    stripped.trim().to_string()
}

fn match_all() -> AnyConstraint {
    AnyConstraint::MatchAll(MatchAllConstraint::new(None))
}

/// ref: BaseDependencyCommandTest::testExceptionWhenNoRequiredParameters (data provider rolled in).
#[test]
#[serial]
fn test_exception_when_no_required_parameters() {
    // noParametersCaseProvider
    let cases: Vec<(&str, Vec<(&str, PhpMixed)>, &str)> = vec![
        (
            "why",
            vec![],
            "Not enough arguments (missing: \"package\").",
        ),
        (
            "why-not",
            vec![],
            "Not enough arguments (missing: \"package, version\").",
        ),
        (
            "why-not",
            vec![("version", PhpMixed::from("*"))],
            "Not enough arguments (missing: \"package\").",
        ),
        (
            "why-not",
            vec![("package", PhpMixed::from("vendor1/package1"))],
            "Not enough arguments (missing: \"version\").",
        ),
    ];

    for (command, parameters, expected_message) in cases {
        let mut input: Vec<(PhpMixed, PhpMixed)> =
            vec![(PhpMixed::from("command"), PhpMixed::from(command))];
        for (k, v) in parameters {
            input.push((PhpMixed::from(k), v));
        }

        let mut app_tester = get_application_tester();
        let err = app_tester
            .run(input, RunOptions::default())
            .expect_err("expected RuntimeException");
        assert!(
            err.to_string().contains(expected_message),
            "expected error to contain {:?}, got: {:?}",
            expected_message,
            err.to_string(),
        );
    }
}

/// ref: BaseDependencyCommandTest::testExceptionWhenRunningLockedWithoutLockFile.
#[test]
#[serial]
fn test_exception_when_running_locked_without_lock_file() {
    // caseProvider
    let cases: Vec<(&str, Vec<(&str, PhpMixed)>)> = vec![
        ("why", vec![("package", PhpMixed::from("vendor1/package1"))]),
        (
            "why-not",
            vec![
                ("package", PhpMixed::from("vendor1/package1")),
                ("version", PhpMixed::from("1.*")),
            ],
        ),
    ];

    for (command, parameters) in cases {
        let tear_down = init_temp_composer(None, None, None, true);

        let mut input: Vec<(PhpMixed, PhpMixed)> =
            vec![(PhpMixed::from("command"), PhpMixed::from(command))];
        for (k, v) in parameters {
            input.push((PhpMixed::from(k), v));
        }
        input.push((PhpMixed::from("--locked"), PhpMixed::from(true)));

        let mut app_tester = get_application_tester();
        let err = app_tester
            .run(input, RunOptions::default())
            .expect_err("expected UnexpectedValueException");
        assert!(
            err.to_string().contains(
                "A valid composer.lock file is required to run this command with --locked"
            ),
            "got: {:?}",
            err.to_string(),
        );

        drop(tear_down);
    }
}

/// ref: BaseDependencyCommandTest::testExceptionWhenItCouldNotFoundThePackage.
#[test]
#[serial]
#[ignore]
fn test_exception_when_it_could_not_found_the_package() {
    // caseProvider
    let cases: Vec<(&str, Vec<(&str, PhpMixed)>)> = vec![
        ("why", vec![("package", PhpMixed::from("vendor1/package1"))]),
        (
            "why-not",
            vec![
                ("package", PhpMixed::from("vendor1/package1")),
                ("version", PhpMixed::from("1.*")),
            ],
        ),
    ];

    for (command, parameters) in cases {
        let package_to_be_inspected = "vendor1/package1";

        let tear_down = init_temp_composer(None, None, None, true);

        let mut input: Vec<(PhpMixed, PhpMixed)> =
            vec![(PhpMixed::from("command"), PhpMixed::from(command))];
        for (k, v) in parameters {
            input.push((PhpMixed::from(k), v));
        }

        let mut app_tester = get_application_tester();
        let err = app_tester
            .run(input, RunOptions::default())
            .expect_err("expected InvalidArgumentException");
        let expected = format!(
            "Could not find package \"{}\" in your project",
            package_to_be_inspected
        );
        assert!(
            err.to_string().contains(&expected),
            "expected error to contain {:?}, got: {:?}",
            expected,
            err.to_string(),
        );

        drop(tear_down);
    }
}

/// ref: BaseDependencyCommandTest::testExceptionWhenPackageWasNotFoundInProject.
#[test]
#[serial]
#[ignore]
fn test_exception_when_package_was_not_found_in_project() {
    // caseProvider
    let cases: Vec<(&str, Vec<(&str, PhpMixed)>)> = vec![
        ("why", vec![("package", PhpMixed::from("vendor1/package1"))]),
        (
            "why-not",
            vec![
                ("package", PhpMixed::from("vendor1/package1")),
                ("version", PhpMixed::from("1.*")),
            ],
        ),
    ];

    for (command, parameters) in cases {
        let package_to_be_inspected = "vendor1/package1";

        let tear_down = init_temp_composer(
            Some(&serde_json::json!({
                "require": {
                    "vendor1/package2": "1.*",
                    "vendor2/package1": "2.*",
                },
            })),
            None,
            None,
            true,
        );

        let first_required_package = get_package("vendor1/package2", "1.0.0");
        let second_required_package = get_package("vendor2/package1", "1.0.0");

        let packages: Vec<PackageInterfaceHandle> = vec![
            first_required_package.clone(),
            second_required_package.clone(),
        ];
        create_installed_json(&packages, &[], false);
        create_composer_lock(&packages, &[]);

        let mut input: Vec<(PhpMixed, PhpMixed)> =
            vec![(PhpMixed::from("command"), PhpMixed::from(command))];
        for (k, v) in parameters {
            input.push((PhpMixed::from(k), v));
        }

        let mut app_tester = get_application_tester();
        let err = app_tester
            .run(input, RunOptions::default())
            .expect_err("expected InvalidArgumentException");
        let expected = format!(
            "Could not find package \"{}\" in your project",
            package_to_be_inspected
        );
        assert!(
            err.to_string().contains(&expected),
            "expected error to contain {:?}, got: {:?}",
            expected,
            err.to_string(),
        );

        drop(tear_down);
    }
}

/// ref: BaseDependencyCommandTest::testWarningWhenDependenciesAreNotInstalled.
#[test]
#[serial]
fn test_warning_when_dependencies_are_not_installed() {
    let expected_warning_message = "<warning>No dependencies installed. Try running composer install or update, or use --locked.</warning>";

    // caseProvider
    let cases: Vec<(&str, Vec<(&str, PhpMixed)>)> = vec![
        ("why", vec![("package", PhpMixed::from("vendor1/package1"))]),
        (
            "why-not",
            vec![
                ("package", PhpMixed::from("vendor1/package1")),
                ("version", PhpMixed::from("1.*")),
            ],
        ),
    ];

    for (command, parameters) in cases {
        let tear_down = init_temp_composer(
            Some(&serde_json::json!({
                "require": {
                    "vendor1/package1": "1.*",
                },
                "require-dev": {
                    "vendor2/package1": "2.*",
                },
            })),
            None,
            None,
            true,
        );

        let some_required_package = get_package("vendor1/package1", "1.0.0");
        let some_dev_required_package = get_package("vendor2/package1", "1.0.0");

        create_composer_lock(
            std::slice::from_ref(&some_required_package),
            std::slice::from_ref(&some_dev_required_package),
        );

        let mut input: Vec<(PhpMixed, PhpMixed)> =
            vec![(PhpMixed::from("command"), PhpMixed::from(command))];
        for (k, v) in parameters {
            input.push((PhpMixed::from(k), v));
        }

        let mut app_tester = get_application_tester();
        app_tester.run(input, RunOptions::default()).unwrap();

        assert_eq!(expected_warning_message, app_tester.get_display().trim());

        drop(tear_down);
    }
}

/// ref: BaseDependencyCommandTest::testWhyCommandOutputs (caseWhyProvider rolled in).
#[test]
#[serial]
#[ignore]
fn test_why_command_outputs() {
    // caseWhyProvider: (package, --tree, --recursive, expected_output, expected_status_code)
    let cases: Vec<(&str, bool, bool, &str, i32)> = vec![
        (
            "vendor1/package1",
            false,
            false,
            "There is no installed package depending on \"vendor1/package1\"",
            1,
        ),
        (
            "vendor1/package3",
            false,
            false,
            "__root__         -     requires vendor1/package3 (2.3.0)\n\
             vendor1/package2 2.3.0 requires vendor1/package3 (^1)",
            0,
        ),
        (
            "vendor1/package3",
            true,
            false,
            "vendor1/package3 2.1.0\n\
             |--__root__ (requires vendor1/package3 2.3.0)\n\
             `--vendor1/package2 2.3.0 (requires vendor1/package3 ^1)\n   \
             |--__root__ (requires vendor1/package2 1.3.0)\n   \
             `--vendor1/package1 1.3.0 (requires vendor1/package2 ^2)",
            0,
        ),
        (
            "vendor1/package3",
            false,
            true,
            "__root__         -     requires vendor1/package2 (1.3.0)\n\
             vendor1/package1 1.3.0 requires vendor1/package2 (^2)\n\
             __root__         -     requires vendor1/package3 (2.3.0)\n\
             vendor1/package2 2.3.0 requires vendor1/package3 (^1)",
            0,
        ),
        (
            "vendor2/package1",
            false,
            false,
            "__root__ - requires (for development) vendor2/package1 (2.*)",
            0,
        ),
    ];

    for (
        package_to_be_inspected,
        render_as_tree,
        render_recursively,
        expected_output,
        expected_status_code,
    ) in cases
    {
        let tear_down = init_temp_composer(
            Some(&serde_json::json!({
                "repositories": {
                    "packages": {
                        "type": "package",
                        "package": [
                            {"name": "vendor1/package1", "version": "1.3.0", "require": {"vendor1/package2": "^2"}},
                            {"name": "vendor1/package2", "version": "2.3.0", "require": {"vendor1/package3": "^1"}},
                            {"name": "vendor1/package3", "version": "2.1.0"},
                        ],
                    },
                },
                "require": {
                    "vendor1/package2": "1.3.0",
                    "vendor1/package3": "2.3.0",
                },
                "require-dev": {
                    "vendor2/package1": "2.*",
                },
            })),
            None,
            None,
            true,
        );

        let first_required_package = get_package("vendor1/package1", "1.3.0");
        let mut req1 = indexmap::IndexMap::new();
        req1.insert(
            "vendor1/package2".to_string(),
            Link::new(
                "vendor1/package1".to_string(),
                "vendor1/package2".to_string(),
                match_all(),
                Some(Link::TYPE_REQUIRE.to_string()),
                "^2".to_string(),
            ),
        );
        first_required_package.__set_requires(req1);

        let second_required_package = get_package("vendor1/package2", "2.3.0");
        let mut req2 = indexmap::IndexMap::new();
        req2.insert(
            "vendor1/package3".to_string(),
            Link::new(
                "vendor1/package2".to_string(),
                "vendor1/package3".to_string(),
                match_all(),
                Some(Link::TYPE_REQUIRE.to_string()),
                "^1".to_string(),
            ),
        );
        second_required_package.__set_requires(req2);

        let third_required_package = get_package("vendor1/package3", "2.1.0");
        let some_dev_required_package = get_package("vendor2/package1", "1.0.0");

        create_composer_lock(
            &[
                first_required_package.clone(),
                second_required_package.clone(),
                third_required_package.clone(),
            ],
            std::slice::from_ref(&some_dev_required_package),
        );
        create_installed_json(
            &[
                first_required_package.clone(),
                second_required_package.clone(),
                third_required_package.clone(),
            ],
            std::slice::from_ref(&some_dev_required_package),
            true,
        );

        let input: Vec<(PhpMixed, PhpMixed)> = vec![
            (PhpMixed::from("command"), PhpMixed::from("why")),
            (
                PhpMixed::from("package"),
                PhpMixed::from(package_to_be_inspected),
            ),
            (PhpMixed::from("--tree"), PhpMixed::from(render_as_tree)),
            (
                PhpMixed::from("--recursive"),
                PhpMixed::from(render_recursively),
            ),
            (PhpMixed::from("--locked"), PhpMixed::from(true)),
        ];

        let mut app_tester = get_application_tester();
        app_tester.run(input, RunOptions::default()).unwrap();

        assert_eq!(expected_status_code, app_tester.get_status_code());
        assert_eq!(
            expected_output.trim(),
            trim_lines(&app_tester.get_display()),
        );

        drop(tear_down);
    }
}

/// ref: BaseDependencyCommandTest::testWhyNotCommandOutputs (caseWhyNotProvider rolled in).
#[test]
#[serial]
#[ignore = "panics in shirabe_php_shim::runtime::trigger_error (crates/shirabe-php-shim/src/runtime.rs:435 todo!()): emitting a PHP error obeys error_reporting and the installed error handler (runtime state not modeled), reached on the `why-not` command execution path"]
fn test_why_not_command_outputs() {
    // caseWhyNotProvider: (package, version, expected_output, expected_status_code)
    let cases: Vec<(&str, &str, &str, i32)> = vec![
        (
            "vendor1/package1",
            "3.*",
            "Package \"vendor1/package1\" could not be found with constraint \"3.*\", results below will most likely be incomplete.\n\
             __root__ - requires vendor1/package1 (1.*)\n\
             Not finding what you were looking for? Try calling `composer require \"vendor1/package1:3.*\" --dry-run` to get another view on the problem.",
            1,
        ),
        (
            "vendor1/package1",
            "^1.4",
            "Package \"vendor1/package1\" could not be found with constraint \"^1.4\", results below will most likely be incomplete.\n\
             There is no installed package depending on \"vendor1/package1\" in versions not matching ^1.4\n\
             Not finding what you were looking for? Try calling `composer require \"vendor1/package1:^1.4\" --dry-run` to get another view on the problem.",
            0,
        ),
        (
            "vendor1/package1",
            "^1.3",
            "Package \"vendor1/package1\" 1.3.0 is already installed! To find out why, run `composer why vendor1/package1`",
            0,
        ),
        (
            "vendor2/package3",
            "1.5.0",
            "vendor2/package2 1.0.0 requires vendor2/package3 (1.4.*)\n\
             Not finding what you were looking for? Try calling `composer update \"vendor2/package3:1.5.0\" --dry-run` to get another view on the problem.",
            1,
        ),
        (
            "php",
            "^8",
            "Package \"php ^8\" found in version \"8.3.2\" (version provided by config.platform).\n\
             There is no installed package depending on \"php\" in versions not matching ^8",
            0,
        ),
        (
            "php",
            "9.1.0",
            "__root__         -     requires php (^8)\n\
             vendor2/package2 1.0.0 requires php (^8.2)",
            1,
        ),
    ];

    for (
        package_to_be_inspected,
        package_version_to_be_inspected,
        expected_output,
        expected_status_code,
    ) in cases
    {
        let tear_down = init_temp_composer(
            Some(&serde_json::json!({
                "repositories": {
                    "packages": {
                        "type": "package",
                        "package": [
                            {"name": "vendor1/package1", "version": "1.3.0"},
                            {"name": "vendor2/package1", "version": "2.0.0"},
                            {"name": "vendor2/package2", "version": "1.0.0", "require": {"vendor2/package3": "1.4.*", "php": "^8.2"}},
                            {"name": "vendor2/package3", "version": "1.4.0"},
                            {"name": "vendor2/package3", "version": "1.5.0"},
                        ],
                    },
                },
                "require": {
                    "vendor1/package1": "1.*",
                    "php": "^8",
                },
                "require-dev": {
                    "vendor2/package1": "2.*",
                    "vendor2/package2": "^1",
                },
                "config": {
                    "platform": {
                        "php": "8.3.2",
                    },
                },
            })),
            None,
            None,
            true,
        );

        let some_required_package = get_package("vendor1/package1", "1.3.0");
        let first_dev_required_package = get_package("vendor2/package1", "2.0.0");
        let second_dev_required_package = get_package("vendor2/package2", "1.0.0");

        let mut req = indexmap::IndexMap::new();
        req.insert(
            "vendor2/package3".to_string(),
            Link::new(
                "vendor2/package2".to_string(),
                "vendor2/package3".to_string(),
                match_all(),
                Some(Link::TYPE_REQUIRE.to_string()),
                "1.4.*".to_string(),
            ),
        );
        req.insert(
            "php".to_string(),
            Link::new(
                "vendor2/package2".to_string(),
                "php".to_string(),
                AnyConstraint::Multi(MultiConstraint::new(
                    vec![
                        get_version_constraint(">=", "8.2.0.0"),
                        get_version_constraint("<", "9.0.0.0-dev"),
                    ],
                    true,
                    None,
                )),
                Some(Link::TYPE_REQUIRE.to_string()),
                "^8.2".to_string(),
            ),
        );
        second_dev_required_package.__set_requires(req);

        let second_dev_nested_required_package = get_package("vendor2/package3", "1.4.0");

        create_composer_lock(
            std::slice::from_ref(&some_required_package),
            &[
                first_dev_required_package.clone(),
                second_dev_required_package.clone(),
            ],
        );
        create_installed_json(
            std::slice::from_ref(&some_required_package),
            &[
                first_dev_required_package.clone(),
                second_dev_required_package.clone(),
                second_dev_nested_required_package.clone(),
            ],
            true,
        );

        let input: Vec<(PhpMixed, PhpMixed)> = vec![
            (PhpMixed::from("command"), PhpMixed::from("why-not")),
            (
                PhpMixed::from("package"),
                PhpMixed::from(package_to_be_inspected),
            ),
            (
                PhpMixed::from("version"),
                PhpMixed::from(package_version_to_be_inspected),
            ),
        ];

        let mut app_tester = get_application_tester();
        app_tester.run(input, RunOptions::default()).unwrap();

        assert_eq!(expected_status_code, app_tester.get_status_code());
        assert_eq!(
            expected_output.trim(),
            trim_lines(&app_tester.get_display()),
        );

        drop(tear_down);
    }
}
