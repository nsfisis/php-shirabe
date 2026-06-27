//! ref: composer/tests/Composer/Test/Command/StatusCommandTest.php

use crate::test_case::{
    RunOptions, create_composer_lock, create_installed_json, get_application_tester,
    get_complete_package, get_package, init_temp_composer,
};
use serial_test::serial;
use shirabe::package::handle::PackageInterfaceHandle;
use shirabe_php_shim::PhpMixed;

fn input(pairs: Vec<(&str, PhpMixed)>) -> Vec<(PhpMixed, PhpMixed)> {
    pairs
        .into_iter()
        .map(|(k, v)| (PhpMixed::from(k), v))
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
    package.__set_type("metapackage".to_string());

    let packages: Vec<PackageInterfaceHandle> = vec![package.into()];

    create_composer_lock(&packages, &[]);
    create_installed_json(&packages, &[], true);

    let mut app_tester = get_application_tester();
    app_tester
        .run(
            vec![(PhpMixed::from("command"), PhpMixed::from("status"))],
            RunOptions::default(),
        )
        .unwrap();

    assert_eq!("No local changes", app_tester.get_display().trim());

    drop(tear_down);
}

/// ref: StatusCommandTest::locallyModifiedPackagesUseCaseProvider entry.
struct LocallyModifiedPackageData {
    name: &'static str,
    version: &'static str,
    installation_source: &'static str,
    r#type: &'static str,
    url: &'static str,
    reference: Option<&'static str>,
}

/// ref: StatusCommandTest::testLocallyModifiedPackages (data provider rolled into a helper).
fn run_locally_modified_packages_case(
    composer_json: serde_json::Value,
    command_flags: Vec<(&str, PhpMixed)>,
    package_data: LocallyModifiedPackageData,
) {
    let _tear_down = init_temp_composer(Some(&composer_json), None, None, true);

    let package = get_package(package_data.name, package_data.version);
    package.set_installation_source(Some(package_data.installation_source.to_string()));

    if package_data.installation_source == "source" {
        package.__set_source_type(Some(package_data.r#type.to_string()));
        package.set_source_url(Some(package_data.url.to_string()));
        package.set_source_reference(package_data.reference.map(str::to_string));
    }

    if package_data.installation_source == "dist" {
        package.set_dist_type(Some(package_data.r#type.to_string()));
        package.set_dist_url(Some(package_data.url.to_string()));
        package.set_dist_reference(package_data.reference.map(str::to_string));
    }

    create_composer_lock(&[package], &[]);

    let mut app_tester = get_application_tester();
    app_tester
        .run(
            input(vec![("command", PhpMixed::from("install"))]),
            RunOptions::default(),
        )
        .unwrap();

    std::fs::write(
        std::env::current_dir()
            .unwrap()
            .join("vendor")
            .join(package_data.name)
            .join("composer.json"),
        "{}",
    )
    .unwrap();

    let mut status_input = vec![("command", PhpMixed::from("status"))];
    status_input.extend(command_flags);
    app_tester
        .run(input(status_input), RunOptions::default())
        .unwrap();

    let expected = "You have changes in the following dependencies:";
    let actual = app_tester.get_display();
    let actual = actual.trim();

    assert!(actual.contains(expected));
    assert!(actual.contains(package_data.name));
}

#[test]
#[serial]
#[ignore = "runs `install` to download the package (git source / dist zip) before `status`, but \
            InstallationManager::execute is still a todo!() stub so install never populates vendor/, \
            and init_temp_composer disables packagist.org — running this needs a real VCS/zip fixture"]
fn test_locally_modified_packages_from_source() {
    run_locally_modified_packages_case(
        serde_json::json!({ "require": { "composer/class-map-generator": "^1.0" } }),
        vec![],
        LocallyModifiedPackageData {
            name: "composer/class-map-generator",
            version: "1.1",
            installation_source: "source",
            r#type: "git",
            url: "https://github.com/composer/class-map-generator.git",
            reference: Some("953cc4ea32e0c31f2185549c7d216d7921f03da9"),
        },
    );
}

#[test]
#[serial]
#[ignore = "runs `install` to download the package (git source / dist zip) before `status`, but \
            InstallationManager::execute is still a todo!() stub so install never populates vendor/, \
            and init_temp_composer disables packagist.org — running this needs a real VCS/zip fixture"]
fn test_locally_modified_packages_from_dist() {
    run_locally_modified_packages_case(
        serde_json::json!({ "require": { "smarty/smarty": "^3.1" } }),
        vec![("--verbose", PhpMixed::from(true))],
        LocallyModifiedPackageData {
            name: "smarty/smarty",
            version: "3.1.7",
            installation_source: "dist",
            r#type: "zip",
            url: "https://www.smarty.net/files/Smarty-3.1.7.zip",
            reference: None,
        },
    );
}
