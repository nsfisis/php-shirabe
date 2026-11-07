//! ref: composer/tests/Composer/Test/Command/HomeCommandTest.php sha1:4145ff15b54d8d5408ab88173ec6047a353416d6

use crate::test_case::{
    RunOptions, create_installed_json, get_application_tester, get_complete_package,
    init_temp_composer,
};
use bstr::BStr;
use serial_test::serial;
use shirabe::package::PackageInterfaceHandle;
use shirabe_php_shim::{bformat, trim};
use shirabe_symfony_console::input::{InputValue, ParameterName};

/// ref: HomeCommandTest::testHomeCommandWithShowFlag
fn run_use_case(
    composer_json: serde_json::Value,
    command: Vec<(ParameterName, InputValue)>,
    expected: &BStr,
    urls: &[(&BStr, &BStr)],
) {
    let _tear_down = init_temp_composer(Some(&composer_json), None, None, true);

    let package = get_complete_package("vendor/package", "1.2.3");
    let dev_package = get_complete_package("vendor/devpackage", "2.3.4");

    for (pkg, url) in urls {
        if *pkg == "vendor/package" {
            package.set_homepage(bformat!("{url}"));
        }
        if *pkg == "vendor/devpackage" {
            dev_package.set_homepage(bformat!("{url}"));
        }
    }

    let packages: Vec<PackageInterfaceHandle> = vec![package.into()];
    let dev_packages: Vec<PackageInterfaceHandle> = vec![dev_package.into()];

    create_installed_json(&packages, &dev_packages, true);

    let mut app_tester = get_application_tester();
    let mut input = vec![
        (ParameterName::of("command"), InputValue::from("home")),
        (ParameterName::of("--show"), InputValue::from(true)),
    ];
    input.extend(command);
    app_tester.run(input, RunOptions::default()).unwrap();

    assert_eq!(trim(expected), trim(app_tester.get_display()));
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
            ParameterName::of("packages"),
            InputValue::Array(vec!["vendor/package".into()]),
        )],
        BStr::new("<warning>Invalid or missing repository URL for vendor/package</warning>"),
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
        BStr::new(
            "No package specified, opening homepage for the root package\n\
         <warning>Invalid or missing repository URL for __root__</warning>",
        ),
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
            ParameterName::of("packages"),
            InputValue::Array(vec!["vendor/anotherpackage".into()]),
        )],
        BStr::new(
            "<warning>Package vendor/anotherpackage not found</warning>\n\
         <warning>Invalid or missing repository URL for vendor/anotherpackage</warning>",
        ),
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
            ParameterName::of("packages"),
            InputValue::Array(vec!["vendor/package".into()]),
        )],
        BStr::new("https://example.org"),
        &[(
            BStr::new("vendor/package"),
            BStr::new("https://example.org"),
        )],
    );
}

/// ref: useCaseProvider 'A valid dev package URL'
#[test]
#[serial]
fn test_home_command_with_show_flag_a_valid_dev_package_url() {
    run_use_case(
        serde_json::json!({ "repositories": [] }),
        vec![(
            ParameterName::of("packages"),
            InputValue::Array(vec!["vendor/devpackage".into()]),
        )],
        BStr::new("https://example.org/dev"),
        &[(
            BStr::new("vendor/devpackage"),
            BStr::new("https://example.org/dev"),
        )],
    );
}
