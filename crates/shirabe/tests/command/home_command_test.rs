//! ref: composer/tests/Composer/Test/Command/HomeCommandTest.php

use crate::test_case::{
    RunOptions, create_installed_json, get_application_tester, get_complete_package,
    init_temp_composer,
};
use serial_test::serial;
use shirabe::package::handle::PackageInterfaceHandle;
use shirabe_php_shim::PhpMixed;

/// ref: HomeCommandTest::testHomeCommandWithShowFlag
fn run_use_case(
    composer_json: serde_json::Value,
    command: Vec<(PhpMixed, PhpMixed)>,
    expected: &str,
    urls: &[(&str, &str)],
) {
    let _tear_down = init_temp_composer(Some(&composer_json), None, None, true);

    let package = get_complete_package("vendor/package", "1.2.3");
    let dev_package = get_complete_package("vendor/devpackage", "2.3.4");

    for (pkg, url) in urls {
        if *pkg == "vendor/package" {
            package.set_homepage(url.to_string());
        }
        if *pkg == "vendor/devpackage" {
            dev_package.set_homepage(url.to_string());
        }
    }

    let packages: Vec<PackageInterfaceHandle> = vec![package.into()];
    let dev_packages: Vec<PackageInterfaceHandle> = vec![dev_package.into()];

    create_installed_json(&packages, &dev_packages, true);

    let mut app_tester = get_application_tester();
    let mut input = vec![
        (PhpMixed::from("command"), PhpMixed::from("home")),
        (PhpMixed::from("--show"), PhpMixed::from(true)),
    ];
    input.extend(command);
    app_tester.run(input, RunOptions::default()).unwrap();

    assert_eq!(expected.trim(), app_tester.get_display().trim());
}

/// ref: useCaseProvider 'Invalid or missing repository URL'
#[test]
#[serial]
fn test_home_command_with_show_flag_invalid_or_missing_repository_url() {
    run_use_case(
        serde_json::json!({
            "repositories": {
                "packages": {
                    "type": "package",
                    "package": [
                        {"name": "vendor/package", "description": "generic description", "version": "1.0.0"},
                    ],
                },
            },
            "require": {
                "vendor/package": "^1.0",
            },
        }),
        vec![(
            PhpMixed::from("packages"),
            PhpMixed::List(vec![PhpMixed::from("vendor/package")]),
        )],
        "<warning>Invalid or missing repository URL for vendor/package</warning>",
        &[],
    );
}

/// ref: useCaseProvider 'No Packages Provided'
#[test]
#[serial]
fn test_home_command_with_show_flag_no_packages_provided() {
    run_use_case(
        serde_json::json!({ "repositories": [] }),
        vec![],
        "No package specified, opening homepage for the root package\n\
         <warning>Invalid or missing repository URL for __root__</warning>",
        &[],
    );
}

/// ref: useCaseProvider 'Package not found'
#[test]
#[serial]
fn test_home_command_with_show_flag_package_not_found() {
    run_use_case(
        serde_json::json!({ "repositories": [] }),
        vec![(
            PhpMixed::from("packages"),
            PhpMixed::List(vec![PhpMixed::from("vendor/anotherpackage")]),
        )],
        "<warning>Package vendor/anotherpackage not found</warning>\n\
         <warning>Invalid or missing repository URL for vendor/anotherpackage</warning>",
        &[],
    );
}

/// ref: useCaseProvider 'A valid package URL'
#[test]
#[serial]
fn test_home_command_with_show_flag_a_valid_package_url() {
    run_use_case(
        serde_json::json!({ "repositories": [] }),
        vec![(
            PhpMixed::from("packages"),
            PhpMixed::List(vec![PhpMixed::from("vendor/package")]),
        )],
        "https://example.org",
        &[("vendor/package", "https://example.org")],
    );
}

/// ref: useCaseProvider 'A valid dev package URL'
#[test]
#[serial]
fn test_home_command_with_show_flag_a_valid_dev_package_url() {
    run_use_case(
        serde_json::json!({ "repositories": [] }),
        vec![(
            PhpMixed::from("packages"),
            PhpMixed::List(vec![PhpMixed::from("vendor/devpackage")]),
        )],
        "https://example.org/dev",
        &[("vendor/devpackage", "https://example.org/dev")],
    );
}
