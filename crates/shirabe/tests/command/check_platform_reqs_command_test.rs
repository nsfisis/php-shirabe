//! ref: composer/tests/Composer/Test/Command/CheckPlatformReqsCommandTest.php

use crate::test_case::{
    RunOptions, create_composer_lock, create_installed_json, get_application_tester,
    get_complete_package, init_temp_composer,
};
use serial_test::serial;
use shirabe::package::handle::PackageInterfaceHandle;
use shirabe_symfony_console::input::InputValue;
use shirabe_symfony_console::input::ParameterName;

/// ref: CheckPlatformReqsCommandTest::testPlatformReqsAreSatisfied (data provider rolled into one body).
fn run_platform_reqs_are_satisfied_case(
    composer_json: serde_json::Value,
    command: &[(&str, InputValue)],
    expected: &str,
    lock: bool,
) {
    let tear_down = init_temp_composer(Some(&composer_json), None, None, false);

    let packages: Vec<PackageInterfaceHandle> =
        vec![get_complete_package("ext-foobar", "2.3.4").into()];
    let dev_packages: Vec<PackageInterfaceHandle> =
        vec![get_complete_package("ext-barbaz", "2.3.4.5").into()];

    create_installed_json(&packages, &dev_packages, true);

    if lock {
        create_composer_lock(&packages, &dev_packages);
    }

    let mut input: Vec<(ParameterName, InputValue)> = vec![(
        ParameterName::of("command"),
        InputValue::from("check-platform-reqs"),
    )];
    for (k, v) in command {
        input.push((ParameterName::of(k), v.clone()));
    }

    let mut app_tester = get_application_tester();
    let status_code = app_tester.run(input, RunOptions::default()).unwrap();

    // assertCommandIsSuccessful
    assert_eq!(0, status_code, "{}", app_tester.get_display());
    assert_eq!(expected.trim(), app_tester.get_display().trim());

    drop(tear_down);
}

#[test]
#[serial]
fn test_platform_reqs_are_satisfied() {
    // 'Disables checking of require-dev packages requirements.'
    run_platform_reqs_are_satisfied_case(
        serde_json::json!({
            "require": { "ext-foobar": "^2.0" },
            "require-dev": { "ext-barbaz": "~4.0" },
        }),
        &[("--no-dev", InputValue::from(true))],
        "Checking non-dev platform requirements for packages in the vendor dir
ext-foobar 2.3.4   success",
        true,
    );

    // 'Checks requirements only from the lock file, not from installed packages.'
    run_platform_reqs_are_satisfied_case(
        serde_json::json!({
            "require": { "ext-foobar": "^2.3" },
            "require-dev": { "ext-barbaz": "~2.0" },
        }),
        &[("--lock", InputValue::from(true))],
        "Checking platform requirements using the lock file\next-barbaz 2.3.4.5   success \next-foobar 2.3.4     success",
        true,
    );
}

#[test]
#[serial]
fn test_exception_thrown_if_no_lockfile_found() {
    let tear_down = init_temp_composer(Some(&serde_json::json!({})), None, None, false);

    let mut app_tester = get_application_tester();
    let err = app_tester
        .run(
            vec![(
                ParameterName::of("command"),
                InputValue::from("check-platform-reqs"),
            )],
            RunOptions::default(),
        )
        .expect_err("expected LogicException for missing lockfile");
    assert!(
        err.to_string()
            .contains("No lockfile found. Unable to read locked packages"),
        "got: {}",
        err
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_failed_platform_requirement() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "require": { "ext-foobar": "^0.3" },
            "require-dev": { "ext-barbaz": "^2.3" },
        })),
        None,
        None,
        false,
    );

    let packages: Vec<PackageInterfaceHandle> =
        vec![get_complete_package("ext-foobar", "2.3.4").into()];
    let dev_packages: Vec<PackageInterfaceHandle> =
        vec![get_complete_package("ext-barbaz", "2.3.4.5").into()];

    create_installed_json(&packages, &dev_packages, true);

    create_composer_lock(&packages, &dev_packages);

    let mut app_tester = get_application_tester();
    app_tester
        .run(
            vec![
                (
                    ParameterName::of("command"),
                    InputValue::from("check-platform-reqs"),
                ),
                (ParameterName::of("--format"), InputValue::from("json")),
            ],
            RunOptions::default(),
        )
        .unwrap();

    let expected = r#"Checking platform requirements for packages in the vendor dir
[
    {
        "name": "ext-barbaz",
        "version": "2.3.4.5",
        "status": "success",
        "failed_requirement": null,
        "provider": null
    },
    {
        "name": "ext-foobar",
        "version": "2.3.4",
        "status": "failed",
        "failed_requirement": {
            "source": "__root__",
            "type": "requires",
            "target": "ext-foobar",
            "constraint": "^0.3"
        },
        "provider": null
    }
]"#;

    assert_eq!(expected.trim(), app_tester.get_display().trim());

    drop(tear_down);
}
