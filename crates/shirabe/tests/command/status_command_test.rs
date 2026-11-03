//! ref: composer/tests/Composer/Test/Command/StatusCommandTest.php

use crate::test_case::{
    RunOptions, create_composer_lock, create_installed_json, get_application_tester,
    get_complete_package, get_package, init_temp_composer,
};
use bstr::{BStr, BString, ByteSlice};
use serial_test::serial;
use shirabe::package::handle::PackageInterfaceHandle;
use shirabe_php_shim::{bconcat, path_from_bytes, trim};
use shirabe_symfony_console::input::{InputValue, ParameterName};

fn input(pairs: Vec<(&BStr, InputValue)>) -> Vec<(ParameterName, InputValue)> {
    pairs
        .into_iter()
        .map(|(k, v)| (ParameterName::of(k), v))
        .collect()
}

#[test]
#[serial]
fn test_no_local_changes() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({ "require": { "root/req": "1.*" } })),
        None,
        None,
        true,
    );

    let package = get_complete_package("root/req", "1.0.0");
    package.__set_type(BString::from("metapackage"));

    let packages: Vec<PackageInterfaceHandle> = vec![package.into()];

    create_composer_lock(&packages, &[]);
    create_installed_json(&packages, &[], true);

    let mut app_tester = get_application_tester();
    app_tester
        .run(
            vec![(ParameterName::of("command"), InputValue::from("status"))],
            RunOptions::default(),
        )
        .unwrap();

    assert_eq!("No local changes", trim(app_tester.get_display()));

    drop(tear_down);
}

/// ref: StatusCommandTest::locallyModifiedPackagesUseCaseProvider entry.
struct LocallyModifiedPackageData {
    name: &'static BStr,
    version: &'static BStr,
    installation_source: &'static BStr,
    r#type: &'static BStr,
    url: &'static BStr,
    reference: Option<&'static BStr>,
}

/// ref: StatusCommandTest::testLocallyModifiedPackages (data provider rolled into a helper).
fn run_locally_modified_packages_case(
    composer_json: serde_json::Value,
    command_flags: Vec<(&BStr, InputValue)>,
    package_data: LocallyModifiedPackageData,
) {
    let _tear_down = init_temp_composer(Some(&composer_json), None, None, true);

    let package = get_package(package_data.name, package_data.version);
    package.set_installation_source(Some(bconcat!(package_data.installation_source)));

    if package_data.installation_source == "source" {
        package.__set_source_type(Some(bconcat!(package_data.r#type)));
        package.set_source_url(Some(bconcat!(package_data.url)));
        package.set_source_reference(package_data.reference.map(|reference| reference.to_owned()));
    }

    if package_data.installation_source == "dist" {
        package.set_dist_type(Some(bconcat!(package_data.r#type)));
        package.set_dist_url(Some(bconcat!(package_data.url)));
        package.set_dist_reference(package_data.reference.map(|reference| reference.to_owned()));
    }

    create_composer_lock(&[package], &[]);

    let mut app_tester = get_application_tester();
    app_tester
        .run(
            input(vec![(BStr::new("command"), InputValue::from("install"))]),
            RunOptions::default(),
        )
        .unwrap();

    std::fs::write(
        std::env::current_dir()
            .unwrap()
            .join("vendor")
            .join(path_from_bytes(&package_data.name))
            .join("composer.json"),
        "{}",
    )
    .unwrap();

    let mut status_input = vec![(BStr::new("command"), InputValue::from("status"))];
    status_input.extend(command_flags);
    app_tester
        .run(input(status_input), RunOptions::default())
        .unwrap();

    let expected = "You have changes in the following dependencies:";
    let actual = app_tester.get_display();
    let actual = trim(&actual);

    assert!(actual.contains_str(expected));
    assert!(actual.contains_str(package_data.name));
}

#[test]
#[serial]
fn test_locally_modified_packages_from_source() {
    run_locally_modified_packages_case(
        serde_json::json!({ "require": { "composer/class-map-generator": "^1.0" } }),
        vec![],
        LocallyModifiedPackageData {
            name: BStr::new("composer/class-map-generator"),
            version: BStr::new("1.1"),
            installation_source: BStr::new("source"),
            r#type: BStr::new("git"),
            url: BStr::new("https://github.com/composer/class-map-generator.git"),
            reference: Some(BStr::new("953cc4ea32e0c31f2185549c7d216d7921f03da9")),
        },
    );
}

#[test]
#[serial]
fn test_locally_modified_packages_from_dist() {
    run_locally_modified_packages_case(
        serde_json::json!({ "require": { "smarty/smarty": "^3.1" } }),
        vec![(BStr::new("--verbose"), InputValue::from(true))],
        LocallyModifiedPackageData {
            name: BStr::new("smarty/smarty"),
            version: BStr::new("3.1.7"),
            installation_source: BStr::new("dist"),
            r#type: BStr::new("zip"),
            url: BStr::new("https://www.smarty.net/files/Smarty-3.1.7.zip"),
            reference: None,
        },
    );
}
