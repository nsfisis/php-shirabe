//! ref: composer/tests/Composer/Test/Command/AuditCommandTest.php

use crate::test_case::{
    RunOptions, create_composer_lock, create_installed_json, get_application_tester, get_package,
    init_temp_composer,
};
use serial_test::serial;
use shirabe::package::handle::PackageInterfaceHandle;
use shirabe_php_shim::PhpMixed;

#[test]
#[serial]
fn test_successful_response_code_when_no_packages_are_required() {
    let tear_down = init_temp_composer(None, None, None, true);

    let mut app_tester = get_application_tester();
    app_tester
        .run(
            vec![(PhpMixed::from("command"), PhpMixed::from("audit"))],
            RunOptions::default(),
        )
        .unwrap();

    assert_eq!(0, app_tester.get_status_code());
    assert_eq!(
        "No packages - skipping audit.",
        app_tester.get_display().trim()
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_error_auditing_lock_file_when_it_is_missing() {
    let tear_down = init_temp_composer(None, None, None, true);
    let packages: Vec<PackageInterfaceHandle> = vec![get_package("dummy/pkg", "1.0.0")];
    create_installed_json(&packages, &[], true);

    let mut app_tester = get_application_tester();
    let err = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("audit")),
                (PhpMixed::from("--locked"), PhpMixed::from(true)),
            ],
            RunOptions::default(),
        )
        .expect_err("expected UnexpectedValueException");
    assert!(
        err.to_string().contains(
            "Valid composer.json and composer.lock files are required to run this command with --locked"
        ),
        "unexpected error: {err}"
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_audit_package_with_no_security_vulnerabilities() {
    let tear_down = init_temp_composer(None, None, None, true);
    let packages: Vec<PackageInterfaceHandle> = vec![get_package("dummy/pkg", "1.0.0")];
    create_installed_json(&packages, &[], true);
    create_composer_lock(&packages, &[]);

    let mut app_tester = get_application_tester();
    app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("audit")),
                (PhpMixed::from("--locked"), PhpMixed::from(true)),
            ],
            RunOptions::default(),
        )
        .unwrap();

    assert!(
        app_tester
            .get_display()
            .trim()
            .contains("No security vulnerability advisories found."),
        "unexpected display: {}",
        app_tester.get_display()
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_audit_package_with_no_dev_option_passed() {
    let tear_down = init_temp_composer(None, None, None, true);
    let dev_package: Vec<PackageInterfaceHandle> = vec![get_package("dummy/pkg", "1.0.0")];
    create_installed_json(&[], &dev_package, true);
    create_composer_lock(&[], &dev_package);

    let mut app_tester = get_application_tester();
    app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("audit")),
                (PhpMixed::from("--no-dev"), PhpMixed::from(true)),
            ],
            RunOptions::default(),
        )
        .unwrap();

    assert!(
        app_tester
            .get_display()
            .trim()
            .contains("No packages - skipping audit."),
        "unexpected display: {}",
        app_tester.get_display()
    );

    drop(tear_down);
}
