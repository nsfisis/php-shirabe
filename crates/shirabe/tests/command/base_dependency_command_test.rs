//! ref: composer/tests/Composer/Test/Command/BaseDependencyCommandTest.php

use crate::test_case::{
    RunOptions, create_composer_lock, create_installed_json, get_application_tester, get_package,
    get_version_constraint, init_temp_composer,
};
use bstr::{BStr, BString, ByteSlice};
use serial_test::serial;
use shirabe::package::Link;
use shirabe::package::handle::PackageInterfaceHandle;
use shirabe_php_shim::{bconcat, error_message, trim};
use shirabe_semver::constraint::{AnyConstraint, MatchAllConstraint, MultiConstraint};
use shirabe_symfony_console::input::{InputValue, ParameterName};

/// ref: TestCase::trimLines — strip trailing spaces from each line, then trim the whole string.
fn trim_lines(s: &[u8]) -> bstr::BString {
    let stripped = s
        .split_str("\n")
        .map(|line| line.trim_end_with(|c| c == ' '))
        .collect::<Vec<_>>();
    trim(bstr::join("\n", stripped))
}

fn match_all() -> AnyConstraint {
    AnyConstraint::MatchAll(MatchAllConstraint::new(None))
}

/// ref: BaseDependencyCommandTest::testExceptionWhenNoRequiredParameters (data provider rolled in).
#[test]
#[serial]
fn test_exception_when_no_required_parameters() {
    // noParametersCaseProvider
    let cases: Vec<(&BStr, Vec<(&BStr, InputValue)>, &BStr)> = vec![
        (
            BStr::new("why"),
            vec![],
            BStr::new("Not enough arguments (missing: \"package\")."),
        ),
        (
            BStr::new("why-not"),
            vec![],
            BStr::new("Not enough arguments (missing: \"package, version\")."),
        ),
        (
            BStr::new("why-not"),
            vec![(BStr::new("version"), InputValue::from("*"))],
            BStr::new("Not enough arguments (missing: \"package\")."),
        ),
        (
            BStr::new("why-not"),
            vec![(BStr::new("package"), InputValue::from("vendor1/package1"))],
            BStr::new("Not enough arguments (missing: \"version\")."),
        ),
    ];

    for (command, parameters, expected_message) in cases {
        let mut input: Vec<(ParameterName, InputValue)> =
            vec![(ParameterName::of("command"), InputValue::from(command))];
        for (k, v) in parameters {
            input.push((ParameterName::of(k), v));
        }

        let mut app_tester = get_application_tester();
        let err = app_tester
            .run(input, RunOptions::default())
            .expect_err("expected RuntimeException");
        assert!(
            error_message(&err).contains_str(expected_message),
            "expected error to contain {:?}, got: {:?}",
            expected_message,
            error_message(&err),
        );
    }
}

/// ref: BaseDependencyCommandTest::testExceptionWhenRunningLockedWithoutLockFile.
#[test]
#[serial]
fn test_exception_when_running_locked_without_lock_file() {
    // caseProvider
    let cases: Vec<(&BStr, Vec<(&BStr, InputValue)>)> = vec![
        (
            BStr::new("why"),
            vec![(BStr::new("package"), InputValue::from("vendor1/package1"))],
        ),
        (
            BStr::new("why-not"),
            vec![
                (BStr::new("package"), InputValue::from("vendor1/package1")),
                (BStr::new("version"), InputValue::from("1.*")),
            ],
        ),
    ];

    for (command, parameters) in cases {
        let tear_down = init_temp_composer(None, None, None, true);

        let mut input: Vec<(ParameterName, InputValue)> =
            vec![(ParameterName::of("command"), InputValue::from(command))];
        for (k, v) in parameters {
            input.push((ParameterName::of(k), v));
        }
        input.push((ParameterName::of("--locked"), InputValue::from(true)));

        let mut app_tester = get_application_tester();
        let err = app_tester
            .run(input, RunOptions::default())
            .expect_err("expected UnexpectedValueException");
        assert!(
            error_message(&err).contains_str(
                "A valid composer.lock file is required to run this command with --locked"
            ),
            "got: {:?}",
            error_message(&err),
        );

        drop(tear_down);
    }
}

/// ref: BaseDependencyCommandTest::testExceptionWhenItCouldNotFoundThePackage.
#[test]
#[serial]
fn test_exception_when_it_could_not_found_the_package() {
    // caseProvider
    let cases: Vec<(&BStr, Vec<(&BStr, InputValue)>)> = vec![
        (
            BStr::new("why"),
            vec![(BStr::new("package"), InputValue::from("vendor1/package1"))],
        ),
        (
            BStr::new("why-not"),
            vec![
                (BStr::new("package"), InputValue::from("vendor1/package1")),
                (BStr::new("version"), InputValue::from("1.*")),
            ],
        ),
    ];

    for (command, parameters) in cases {
        let package_to_be_inspected = "vendor1/package1";

        let tear_down = init_temp_composer(None, None, None, true);

        let mut input: Vec<(ParameterName, InputValue)> =
            vec![(ParameterName::of("command"), InputValue::from(command))];
        for (k, v) in parameters {
            input.push((ParameterName::of(k), v));
        }

        let mut app_tester = get_application_tester();
        let err = app_tester
            .run(input, RunOptions::default())
            .expect_err("expected InvalidArgumentException");
        let expected = bconcat!(
            "Could not find package \"",
            package_to_be_inspected,
            "\" in your project"
        );
        assert!(
            error_message(&err).contains_str(&expected),
            "expected error to contain {:?}, got: {:?}",
            expected,
            error_message(&err),
        );

        drop(tear_down);
    }
}

/// ref: BaseDependencyCommandTest::testExceptionWhenPackageWasNotFoundInProject.
#[test]
#[serial]
fn test_exception_when_package_was_not_found_in_project() {
    // caseProvider
    let cases: Vec<(&BStr, Vec<(&BStr, InputValue)>)> = vec![
        (
            BStr::new("why"),
            vec![(BStr::new("package"), InputValue::from("vendor1/package1"))],
        ),
        (
            BStr::new("why-not"),
            vec![
                (BStr::new("package"), InputValue::from("vendor1/package1")),
                (BStr::new("version"), InputValue::from("1.*")),
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

        let mut input: Vec<(ParameterName, InputValue)> =
            vec![(ParameterName::of("command"), InputValue::from(command))];
        for (k, v) in parameters {
            input.push((ParameterName::of(k), v));
        }

        let mut app_tester = get_application_tester();
        let err = app_tester
            .run(input, RunOptions::default())
            .expect_err("expected InvalidArgumentException");
        let expected = bconcat!(
            "Could not find package \"",
            package_to_be_inspected,
            "\" in your project"
        );
        assert!(
            error_message(&err).contains_str(&expected),
            "expected error to contain {:?}, got: {:?}",
            expected,
            error_message(&err),
        );

        drop(tear_down);
    }
}

/// ref: BaseDependencyCommandTest::testWarningWhenDependenciesAreNotInstalled.
#[test]
#[serial]
fn test_warning_when_dependencies_are_not_installed() {
    let expected_warning_message = "<warning>No dependencies installed. Try running shirabe install or update, or use --locked.</warning>";

    // caseProvider
    let cases: Vec<(&BStr, Vec<(&BStr, InputValue)>)> = vec![
        (
            BStr::new("why"),
            vec![(BStr::new("package"), InputValue::from("vendor1/package1"))],
        ),
        (
            BStr::new("why-not"),
            vec![
                (BStr::new("package"), InputValue::from("vendor1/package1")),
                (BStr::new("version"), InputValue::from("1.*")),
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

        let mut input: Vec<(ParameterName, InputValue)> =
            vec![(ParameterName::of("command"), InputValue::from(command))];
        for (k, v) in parameters {
            input.push((ParameterName::of(k), v));
        }

        let mut app_tester = get_application_tester();
        app_tester.run(input, RunOptions::default()).unwrap();

        assert_eq!(expected_warning_message, trim(app_tester.get_display()));

        drop(tear_down);
    }
}

/// ref: BaseDependencyCommandTest::testWhyCommandOutputs (caseWhyProvider rolled in).
#[test]
#[serial]
fn test_why_command_outputs() {
    // caseWhyProvider: (package, --tree, --recursive, expected_output, expected_status_code)
    let cases: Vec<(&BStr, bool, bool, &BStr, i32)> = vec![
        (
            BStr::new("vendor1/package1"),
            false,
            false,
            BStr::new("There is no installed package depending on \"vendor1/package1\""),
            1,
        ),
        (
            BStr::new("vendor1/package3"),
            false,
            false,
            BStr::new(
                "__root__         -     requires vendor1/package3 (2.3.0)\n\
             vendor1/package2 2.3.0 requires vendor1/package3 (^1)",
            ),
            0,
        ),
        (
            BStr::new("vendor1/package3"),
            true,
            false,
            BStr::new(
                "vendor1/package3 2.1.0\n\
             |--__root__ (requires vendor1/package3 2.3.0)\n\
             `--vendor1/package2 2.3.0 (requires vendor1/package3 ^1)\n   \
             |--__root__ (requires vendor1/package2 1.3.0)\n   \
             `--vendor1/package1 1.3.0 (requires vendor1/package2 ^2)",
            ),
            0,
        ),
        (
            BStr::new("vendor1/package3"),
            false,
            true,
            BStr::new(
                "__root__         -     requires vendor1/package2 (1.3.0)\n\
             vendor1/package1 1.3.0 requires vendor1/package2 (^2)\n\
             __root__         -     requires vendor1/package3 (2.3.0)\n\
             vendor1/package2 2.3.0 requires vendor1/package3 (^1)",
            ),
            0,
        ),
        (
            BStr::new("vendor2/package1"),
            false,
            false,
            BStr::new("__root__ - requires (for development) vendor2/package1 (2.*)"),
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
            BString::from("vendor1/package2"),
            Link::new(
                BString::from("vendor1/package1"),
                BString::from("vendor1/package2"),
                match_all(),
                Some(Link::TYPE_REQUIRE.to_owned()),
                BString::from("^2"),
            ),
        );
        first_required_package
            .__set_requires(req1.into_iter().collect::<indexmap::IndexMap<_, _>>());

        let second_required_package = get_package("vendor1/package2", "2.3.0");
        let mut req2 = indexmap::IndexMap::new();
        req2.insert(
            BString::from("vendor1/package3"),
            Link::new(
                BString::from("vendor1/package2"),
                BString::from("vendor1/package3"),
                match_all(),
                Some(Link::TYPE_REQUIRE.to_owned()),
                BString::from("^1"),
            ),
        );
        second_required_package
            .__set_requires(req2.into_iter().collect::<indexmap::IndexMap<_, _>>());

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

        let input: Vec<(ParameterName, InputValue)> = vec![
            (ParameterName::of("command"), InputValue::from("why")),
            (
                ParameterName::of("package"),
                InputValue::from(package_to_be_inspected),
            ),
            (
                ParameterName::of("--tree"),
                InputValue::from(render_as_tree),
            ),
            (
                ParameterName::of("--recursive"),
                InputValue::from(render_recursively),
            ),
            (ParameterName::of("--locked"), InputValue::from(true)),
        ];

        let mut app_tester = get_application_tester();
        app_tester.run(input, RunOptions::default()).unwrap();

        assert_eq!(expected_status_code, app_tester.get_status_code());
        assert_eq!(trim(expected_output), trim_lines(&app_tester.get_display()));

        drop(tear_down);
    }
}

/// ref: BaseDependencyCommandTest::testWhyNotCommandOutputs (caseWhyNotProvider rolled in).
#[test]
#[serial]
fn test_why_not_command_outputs() {
    // caseWhyNotProvider: (package, version, expected_output, expected_status_code)
    let cases: Vec<(&BStr, &BStr, &BStr, i32)> = vec![
        (
            BStr::new("vendor1/package1"),
            BStr::new("3.*"),
            BStr::new(
                "Package \"vendor1/package1\" could not be found with constraint \"3.*\", results below will most likely be incomplete.\n\
             __root__ - requires vendor1/package1 (1.*)\n\
             Not finding what you were looking for? Try calling `shirabe require \"vendor1/package1:3.*\" --dry-run` to get another view on the problem.",
            ),
            1,
        ),
        (
            BStr::new("vendor1/package1"),
            BStr::new("^1.4"),
            BStr::new(
                "Package \"vendor1/package1\" could not be found with constraint \"^1.4\", results below will most likely be incomplete.\n\
             There is no installed package depending on \"vendor1/package1\" in versions not matching ^1.4\n\
             Not finding what you were looking for? Try calling `shirabe require \"vendor1/package1:^1.4\" --dry-run` to get another view on the problem.",
            ),
            0,
        ),
        (
            BStr::new("vendor1/package1"),
            BStr::new("^1.3"),
            BStr::new(
                "Package \"vendor1/package1\" 1.3.0 is already installed! To find out why, run `shirabe why vendor1/package1`",
            ),
            0,
        ),
        (
            BStr::new("vendor2/package3"),
            BStr::new("1.5.0"),
            BStr::new(
                "vendor2/package2 1.0.0 requires vendor2/package3 (1.4.*)\n\
             Not finding what you were looking for? Try calling `shirabe update \"vendor2/package3:1.5.0\" --dry-run` to get another view on the problem.",
            ),
            1,
        ),
        (
            BStr::new("php"),
            BStr::new("^8"),
            BStr::new(
                "Package \"php ^8\" found in version \"8.3.2\" (version provided by config.platform).\n\
             There is no installed package depending on \"php\" in versions not matching ^8",
            ),
            0,
        ),
        (
            BStr::new("php"),
            BStr::new("9.1.0"),
            BStr::new(
                "__root__         -     requires php (^8)\n\
             vendor2/package2 1.0.0 requires php (^8.2)",
            ),
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
            BString::from("vendor2/package3"),
            Link::new(
                BString::from("vendor2/package2"),
                BString::from("vendor2/package3"),
                match_all(),
                Some(Link::TYPE_REQUIRE.to_owned()),
                BString::from("1.4.*"),
            ),
        );
        req.insert(
            BString::from("php"),
            Link::new(
                BString::from("vendor2/package2"),
                BString::from("php"),
                AnyConstraint::Multi(MultiConstraint::new(
                    vec![
                        get_version_constraint(">=", "8.2.0.0"),
                        get_version_constraint("<", "9.0.0.0-dev"),
                    ],
                    true,
                    None,
                )),
                Some(Link::TYPE_REQUIRE.to_owned()),
                BString::from("^8.2"),
            ),
        );
        second_dev_required_package
            .__set_requires(req.into_iter().collect::<indexmap::IndexMap<_, _>>());

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

        let input: Vec<(ParameterName, InputValue)> = vec![
            (ParameterName::of("command"), InputValue::from("why-not")),
            (
                ParameterName::of("package"),
                InputValue::from(package_to_be_inspected),
            ),
            (
                ParameterName::of("version"),
                InputValue::from(package_version_to_be_inspected),
            ),
        ];

        let mut app_tester = get_application_tester();
        app_tester.run(input, RunOptions::default()).unwrap();

        assert_eq!(expected_status_code, app_tester.get_status_code());
        assert_eq!(trim(expected_output), trim_lines(&app_tester.get_display()));

        drop(tear_down);
    }
}
