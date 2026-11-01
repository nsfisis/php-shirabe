//! ref: composer/tests/Composer/Test/Command/RemoveCommandTest.php

use crate::test_case::{
    RunOptions, create_composer_lock, create_installed_json, get_application_tester, get_package,
    init_temp_composer,
};
use bstr::ByteSlice;
use indexmap::IndexMap;
use serial_test::serial;
use shirabe::json::JsonFile;
use shirabe::package::Link;
use shirabe::package::handle::PackageInterfaceHandle;
use shirabe_php_shim::trim;
use shirabe_semver::constraint::{AnyConstraint, MatchAllConstraint};
use shirabe_symfony_console::input::{InputValue, ParameterName};

// Symfony\Component\Console\Command\Command exit codes.
const SUCCESS: i32 = 0;
const FAILURE: i32 = 1;
const INVALID: i32 = 2;

fn input(pairs: Vec<(&str, InputValue)>) -> Vec<(ParameterName, InputValue)> {
    pairs
        .into_iter()
        .map(|(k, v)| (ParameterName::of(k), v))
        .collect()
}

/// Read a JSON file in the CWD and decode it to a `serde_json::Value`.
fn read_json_file(path: &str) -> serde_json::Value {
    let json = JsonFile::new(path, None, None).unwrap();
    let read = json.read().unwrap();
    serde_json::from_str(&JsonFile::encode(&read).unwrap()).unwrap()
}

/// Mirrors PHPUnit's assertEmpty for a decoded JSON document (empty object/array/null).
fn is_empty_json(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => true,
        serde_json::Value::Object(map) => map.is_empty(),
        serde_json::Value::Array(list) => list.is_empty(),
        _ => false,
    }
}

#[test]
#[serial]
fn test_exception_running_with_no_remove_packages() {
    let mut app_tester = get_application_tester();
    let err = app_tester
        .run(
            input(vec![("command", InputValue::from("remove"))]),
            RunOptions::default(),
        )
        .expect_err("expected InvalidArgumentException for missing packages argument");
    assert!(
        err.to_string()
            .contains(r#"Not enough arguments (missing: "packages")."#),
        "got: {}",
        err
    );
}

#[test]
#[serial]
fn test_exception_when_running_unused_without_lock_file() {
    let tear_down = init_temp_composer(None, None, None, true);

    let mut app_tester = get_application_tester();
    let err = app_tester
        .run(
            input(vec![
                ("command", InputValue::from("remove")),
                ("--unused", InputValue::from(true)),
            ]),
            RunOptions::default(),
        )
        .expect_err("expected UnexpectedValueException when --unused without lock file");
    assert!(
        err.to_string()
            .contains("A valid composer.lock file is required to run this command with --unused"),
        "got: {}",
        err
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_warning_when_removing_non_existent_package() {
    let tear_down = init_temp_composer(None, None, None, true);
    create_installed_json(&[], &[], true);

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            input(vec![
                ("command", InputValue::from("remove")),
                (
                    "packages",
                    InputValue::Array(vec!["vendor1/package1".into()]),
                ),
            ]),
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(SUCCESS, status_code);
    assert!(
        trim(app_tester.get_display()).starts_with_str("<warning>vendor1/package1 is not required in your composer.json and has not been removed</warning>"
        ),
        "got: {}",
        app_tester.get_display().display_lossy()
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_warning_when_removing_package_from_wrong_type() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "require": { "root/req": "1.*" },
        })),
        None,
        None,
        true,
    );

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            input(vec![
                ("command", InputValue::from("remove")),
                ("packages", InputValue::Array(vec!["root/req".into()])),
                ("--dev", InputValue::from(true)),
                ("--no-update", InputValue::from(true)),
                ("--no-interaction", InputValue::from(true)),
            ]),
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(SUCCESS, status_code);
    assert_eq!(
        "<warning>root/req could not be found in require-dev but it is present in require</warning>
./composer.json has been updated",
        trim(app_tester.get_display())
    );
    assert_eq!(
        serde_json::json!({ "require": { "root/req": "1.*" } }),
        read_json_file("./composer.json")
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_warning_when_removing_package_with_deprecated_dependencies_flag() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "require": { "root/req": "1.*" },
        })),
        None,
        None,
        true,
    );

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            input(vec![
                ("command", InputValue::from("remove")),
                ("packages", InputValue::Array(vec!["root/req".into()])),
                ("--update-with-dependencies", InputValue::from(true)),
                ("--no-update", InputValue::from(true)),
                ("--no-interaction", InputValue::from(true)),
            ]),
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(SUCCESS, status_code);
    assert_eq!(
        "<warning>You are using the deprecated option \"update-with-dependencies\". This is now default behaviour. The --no-update-with-dependencies option can be used to remove a package without its dependencies.</warning>
./composer.json has been updated",
        trim(app_tester.get_display())
    );
    assert!(
        is_empty_json(&read_json_file("./composer.json")),
        "expected composer.json to be empty, got: {}",
        read_json_file("./composer.json")
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_message_output_when_no_unused_packages_to_remove() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "repositories": {
                "packages": {
                    "type": "package",
                    "package": [
                        {"name": "root/req", "version": "1.0.0", "require": {"nested/req": "^1"}},
                        {"name": "nested/req", "version": "1.1.0"},
                    ],
                },
            },
            "require": { "root/req": "1.*" },
        })),
        None,
        None,
        true,
    );

    let required_package = get_package("root/req", "1.0.0");
    let mut requires = IndexMap::new();
    requires.insert(
        "nested/req".to_string(),
        Link::new(
            bstr::BString::from("root/req".to_string()),
            bstr::BString::from("nested/req".to_string()),
            AnyConstraint::MatchAll(MatchAllConstraint::new(None)),
            Some(bstr::BString::from(Link::TYPE_REQUIRE.to_string())),
            bstr::BString::from("^1".to_string()),
        ),
    );
    required_package.__set_requires(
        requires
            .into_iter()
            .map(|(k, v)| (bstr::BString::from(k), v))
            .collect::<indexmap::IndexMap<_, _>>(),
    );
    let nested_package = get_package("nested/req", "1.1.0");

    let packages: Vec<PackageInterfaceHandle> = vec![required_package, nested_package];
    create_installed_json(&packages, &[], true);
    create_composer_lock(&packages, &[]);

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            input(vec![
                ("command", InputValue::from("remove")),
                ("--unused", InputValue::from(true)),
                ("--no-audit", InputValue::from(true)),
                ("--no-interaction", InputValue::from(true)),
            ]),
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(SUCCESS, status_code);
    assert_eq!(
        "No unused packages to remove",
        trim(app_tester.get_display())
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_remove_unused_package() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "repositories": {
                "packages": {
                    "type": "package",
                    "package": [
                        {"name": "root/req", "version": "1.0.0"},
                        {"name": "not/req", "version": "1.0.0"},
                    ],
                },
            },
            "require": { "root/req": "1.*" },
        })),
        None,
        None,
        true,
    );

    let required_package = get_package("root/req", "1.0.0");
    let extraneous_package = get_package("not/req", "1.0.0");

    create_installed_json(std::slice::from_ref(&required_package), &[], true);
    create_composer_lock(&[required_package, extraneous_package], &[]);

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            input(vec![
                ("command", InputValue::from("remove")),
                ("--unused", InputValue::from(true)),
                ("--no-audit", InputValue::from(true)),
                ("--no-interaction", InputValue::from(true)),
            ]),
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(SUCCESS, status_code);
    assert!(
        app_tester.get_display().starts_with_str("<warning>not/req is not required in your composer.json and has not been removed</warning>"
        ),
        "got: {}",
        app_tester.get_display().display_lossy()
    );
    assert!(
        app_tester
            .get_display()
            .contains_str("Running shirabe update not/req"),
        "got: {}",
        app_tester.get_display().display_lossy()
    );
    assert!(
        app_tester
            .get_display()
            .contains_str("- Removing not/req (1.0.0)"),
        "got: {}",
        app_tester.get_display().display_lossy()
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_remove_package_by_name() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "repositories": {
                "packages": {
                    "type": "package",
                    "package": [
                        {"name": "root/req", "version": "1.0.0", "type": "metapackage"},
                        {"name": "root/another", "version": "1.0.0", "type": "metapackage"},
                    ],
                },
            },
            "require": { "root/req": "1.*", "root/another": "1.*" },
        })),
        None,
        None,
        true,
    );
    let root_req_package = get_package("root/req", "1.0.0");
    let root_another_package = get_package("root/another", "1.0.0");
    // Set as a metapackage so that we can do the whole post-remove update & install process without
    // Composer trying to download them (DownloadManager::getDownloaderForPackage).
    root_req_package.__set_type(bstr::BString::from("metapackage".to_string()));
    root_another_package.__set_type(bstr::BString::from("metapackage".to_string()));

    let packages: Vec<PackageInterfaceHandle> = vec![root_req_package, root_another_package];
    create_installed_json(&packages, &[], true);
    create_composer_lock(&packages, &[]);

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            input(vec![
                ("command", InputValue::from("remove")),
                ("packages", InputValue::Array(vec!["root/req".into()])),
                ("--no-audit", InputValue::from(true)),
                ("--no-interaction", InputValue::from(true)),
            ]),
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(SUCCESS, status_code);
    let display = app_tester.get_display();
    let trimmed = display.trim();
    assert!(
        trimmed.starts_with_str("./composer.json has been updated"),
        "got: {}",
        display.display_lossy()
    );
    assert!(
        trimmed.contains_str("Running shirabe update root/req"),
        "got: {}",
        display.display_lossy()
    );
    assert!(
        trimmed.contains_str("Lock file operations: 0 installs, 0 updates, 1 removal"),
        "got: {}",
        display.display_lossy()
    );
    assert!(
        trimmed.contains_str("- Removing root/req (1.0.0)"),
        "got: {}",
        display.display_lossy()
    );
    assert!(
        trimmed.contains_str("Package operations: 0 installs, 0 updates, 1 removal"),
        "got: {}",
        display.display_lossy()
    );
    assert_eq!(
        serde_json::json!({ "root/another": "1.*" }),
        read_json_file("./composer.json")["require"]
    );
    assert_eq!(
        serde_json::json!([{"name": "root/another", "version": "1.0.0", "type": "metapackage"}]),
        read_json_file("./composer.lock")["packages"]
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_remove_package_by_name_with_dry_run() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "repositories": {
                "packages": {
                    "type": "package",
                    "package": [
                        {"name": "root/req", "version": "1.0.0", "type": "metapackage"},
                        {"name": "root/another", "version": "1.0.0", "type": "metapackage"},
                    ],
                },
            },
            "require": { "root/req": "1.*", "root/another": "1.*" },
        })),
        None,
        None,
        true,
    );
    let root_req_package = get_package("root/req", "1.0.0");
    let root_another_package = get_package("root/another", "1.0.0");
    root_req_package.__set_type(bstr::BString::from("metapackage".to_string()));
    root_another_package.__set_type(bstr::BString::from("metapackage".to_string()));

    let packages: Vec<PackageInterfaceHandle> = vec![root_req_package, root_another_package];
    create_installed_json(&packages, &[], true);
    create_composer_lock(&packages, &[]);

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            input(vec![
                ("command", InputValue::from("remove")),
                ("packages", InputValue::Array(vec!["root/req".into()])),
                ("--dry-run", InputValue::from(true)),
                ("--no-audit", InputValue::from(true)),
                ("--no-interaction", InputValue::from(true)),
            ]),
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(SUCCESS, status_code);
    let display = app_tester.get_display();
    let trimmed = display.trim();
    assert!(
        trimmed.contains_str("./composer.json has been updated"),
        "got: {}",
        display.display_lossy()
    );
    assert!(
        trimmed.contains_str("Running shirabe update root/req"),
        "got: {}",
        display.display_lossy()
    );
    assert!(
        trimmed.contains_str("Lock file operations: 0 installs, 0 updates, 1 removal"),
        "got: {}",
        display.display_lossy()
    );
    assert!(
        trimmed.contains_str("- Removing root/req (1.0.0)"),
        "got: {}",
        display.display_lossy()
    );
    assert!(
        trimmed.contains_str("Package operations: 0 installs, 0 updates, 1 removal"),
        "got: {}",
        display.display_lossy()
    );
    assert_eq!(
        serde_json::json!({ "root/req": "1.*", "root/another": "1.*" }),
        read_json_file("./composer.json")["require"]
    );
    assert_eq!(
        serde_json::json!([
            {"name": "root/another", "version": "1.0.0", "type": "metapackage"},
            {"name": "root/req", "version": "1.0.0", "type": "metapackage"},
        ]),
        read_json_file("./composer.lock")["packages"]
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_remove_allowed_plugin_package_with_no_other_allowed_plugins() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "repositories": {
                "packages": {
                    "type": "package",
                    "package": [
                        {"name": "root/req", "version": "1.0.0", "type": "metapackage"},
                        {"name": "root/another", "version": "1.0.0", "type": "metapackage"},
                    ],
                },
            },
            "require": { "root/req": "1.*", "root/another": "1.*" },
            "config": { "allow-plugins": { "root/req": true } },
        })),
        None,
        None,
        true,
    );
    let root_req_package = get_package("root/req", "1.0.0");
    let root_another_package = get_package("root/another", "1.0.0");
    root_req_package.__set_type(bstr::BString::from("metapackage".to_string()));
    root_another_package.__set_type(bstr::BString::from("metapackage".to_string()));

    let packages: Vec<PackageInterfaceHandle> = vec![root_req_package, root_another_package];
    create_installed_json(&packages, &[], true);
    create_composer_lock(&packages, &[]);

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            input(vec![
                ("command", InputValue::from("remove")),
                ("packages", InputValue::Array(vec!["root/req".into()])),
                ("--no-audit", InputValue::from(true)),
                ("--no-interaction", InputValue::from(true)),
            ]),
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(SUCCESS, status_code);
    assert_eq!(
        serde_json::json!({ "root/another": "1.*" }),
        read_json_file("./composer.json")["require"]
    );
    assert!(
        is_empty_json(&read_json_file("./composer.json")["config"]),
        "expected config to be empty, got: {}",
        read_json_file("./composer.json")["config"]
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_remove_allowed_plugin_package_with_other_allowed_plugins() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "repositories": {
                "packages": {
                    "type": "package",
                    "package": [
                        {"name": "root/req", "version": "1.0.0", "type": "metapackage"},
                        {"name": "root/another", "version": "1.0.0", "type": "metapackage"},
                    ],
                },
            },
            "require": { "root/req": "1.*", "root/another": "1.*" },
            "config": { "allow-plugins": { "root/another": true, "root/req": true } },
        })),
        None,
        None,
        true,
    );
    let root_req_package = get_package("root/req", "1.0.0");
    let root_another_package = get_package("root/another", "1.0.0");
    root_req_package.__set_type(bstr::BString::from("metapackage".to_string()));
    root_another_package.__set_type(bstr::BString::from("metapackage".to_string()));

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            input(vec![
                ("command", InputValue::from("remove")),
                ("packages", InputValue::Array(vec!["root/req".into()])),
                ("--no-audit", InputValue::from(true)),
                ("--no-interaction", InputValue::from(true)),
            ]),
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(SUCCESS, status_code);
    assert_eq!(
        serde_json::json!({ "root/another": "1.*" }),
        read_json_file("./composer.json")["require"]
    );
    assert_eq!(
        serde_json::json!({ "allow-plugins": { "root/another": true } }),
        read_json_file("./composer.json")["config"]
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_remove_packages_by_vendor() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "repositories": {
                "packages": {
                    "type": "package",
                    "package": [
                        {"name": "root/req", "version": "1.0.0"},
                        {"name": "root/another", "version": "1.0.0"},
                        {"name": "another/req", "version": "1.0.0"},
                    ],
                },
            },
            "require": { "root/req": "1.*", "root/another": "1.*", "another/req": "1.*" },
        })),
        None,
        None,
        true,
    );
    let root_req_package = get_package("root/req", "1.0.0");
    let root_another_package = get_package("root/another", "1.0.0");
    let another_req_package = get_package("another/req", "1.0.0");

    let packages: Vec<PackageInterfaceHandle> =
        vec![root_req_package, root_another_package, another_req_package];
    create_installed_json(&packages, &[], true);
    create_composer_lock(&packages, &[]);

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            input(vec![
                ("command", InputValue::from("remove")),
                ("packages", InputValue::Array(vec!["root/*".into()])),
                ("--no-install", InputValue::from(true)),
                ("--no-audit", InputValue::from(true)),
                ("--no-interaction", InputValue::from(true)),
            ]),
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(SUCCESS, status_code);
    let display = app_tester.get_display();
    assert!(
        trim(&display).starts_with_str("./composer.json has been updated"),
        "got: {}",
        display.display_lossy()
    );
    assert!(
        display.contains_str("Running shirabe update root/*"),
        "got: {}",
        display.display_lossy()
    );
    assert!(
        display.contains_str("- Removing root/another (1.0.0)"),
        "got: {}",
        display.display_lossy()
    );
    assert!(
        display.contains_str("- Removing root/req (1.0.0)"),
        "got: {}",
        display.display_lossy()
    );
    assert!(
        display.contains_str("Writing lock file"),
        "got: {}",
        display.display_lossy()
    );
    assert_eq!(
        serde_json::json!({ "another/req": "1.*" }),
        read_json_file("./composer.json")["require"]
    );
    assert_eq!(
        serde_json::json!([{"name": "another/req", "version": "1.0.0", "type": "library"}]),
        read_json_file("./composer.lock")["packages"]
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_remove_packages_by_vendor_with_dry_run() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "repositories": {
                "packages": {
                    "type": "package",
                    "package": [
                        {"name": "root/req", "version": "1.0.0"},
                        {"name": "root/another", "version": "1.0.0"},
                        {"name": "another/req", "version": "1.0.0"},
                    ],
                },
            },
            "require": { "root/req": "1.*", "root/another": "1.*", "another/req": "1.*" },
        })),
        None,
        None,
        true,
    );
    let root_req_package = get_package("root/req", "1.0.0");
    let root_another_package = get_package("root/another", "1.0.0");
    let another_req_package = get_package("another/req", "1.0.0");

    let packages: Vec<PackageInterfaceHandle> =
        vec![root_req_package, root_another_package, another_req_package];
    create_installed_json(&packages, &[], true);
    create_composer_lock(&packages, &[]);

    let mut app_tester = get_application_tester();
    app_tester
        .run(
            input(vec![
                ("command", InputValue::from("remove")),
                ("packages", InputValue::Array(vec!["root/*".into()])),
                ("--dry-run", InputValue::from(true)),
                ("--no-install", InputValue::from(true)),
                ("--no-audit", InputValue::from(true)),
                ("--no-interaction", InputValue::from(true)),
            ]),
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(SUCCESS, app_tester.get_status_code());
    assert_eq!(
        "./composer.json has been updated
Running shirabe update root/*
Loading composer repositories with package information
Updating dependencies
Lock file operations: 0 installs, 0 updates, 2 removals
  - Removing root/another (1.0.0)
  - Removing root/req (1.0.0)",
        trim(app_tester.get_display())
    );
    assert!(
        !app_tester.get_display().contains_str("Writing lock file"),
        "got: {}",
        app_tester.get_display().display_lossy()
    );
    assert_eq!(
        serde_json::json!({ "root/req": "1.*", "root/another": "1.*", "another/req": "1.*" }),
        read_json_file("./composer.json")["require"]
    );
    assert_eq!(
        serde_json::json!([
            {"name": "another/req", "version": "1.0.0", "type": "library"},
            {"name": "root/another", "version": "1.0.0", "type": "library"},
            {"name": "root/req", "version": "1.0.0", "type": "library"},
        ]),
        read_json_file("./composer.lock")["packages"]
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_warning_when_removing_packages_by_vendor_from_wrong_type() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "require": { "root/req": "1.*", "root/another": "1.*", "another/req": "1.*" },
        })),
        None,
        None,
        true,
    );

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            input(vec![
                ("command", InputValue::from("remove")),
                ("packages", InputValue::Array(vec!["root/*".into()])),
                ("--dev", InputValue::from(true)),
                ("--no-interaction", InputValue::from(true)),
                ("--no-update", InputValue::from(true)),
            ]),
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(SUCCESS, status_code);
    assert_eq!(
        "<warning>root/req could not be found in require-dev but it is present in require</warning>
<warning>root/another could not be found in require-dev but it is present in require</warning>
./composer.json has been updated",
        trim(app_tester.get_display())
    );
    assert_eq!(
        serde_json::json!({
            "require": { "root/req": "1.*", "root/another": "1.*", "another/req": "1.*" }
        }),
        read_json_file("./composer.json")
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_package_still_present_error_when_no_install_flag_used() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "require": { "root/req": "1.*" },
        })),
        None,
        None,
        true,
    );
    let root_req_package = get_package("root/req", "1.0.0");

    create_installed_json(std::slice::from_ref(&root_req_package), &[], true);
    create_composer_lock(std::slice::from_ref(&root_req_package), &[]);

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            input(vec![
                ("command", InputValue::from("remove")),
                ("packages", InputValue::Array(vec!["root/req".into()])),
                ("--no-install", InputValue::from(true)),
                ("--no-audit", InputValue::from(true)),
                ("--no-interaction", InputValue::from(true)),
            ]),
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(INVALID, status_code);
    let display = app_tester.get_display();
    assert!(
        display.contains_str("./composer.json has been updated"),
        "got: {}",
        display.display_lossy()
    );
    assert!(
        display.contains_str("Lock file operations: 0 installs, 0 updates, 1 removal"),
        "got: {}",
        display.display_lossy()
    );
    assert!(
        display.contains_str("- Removing root/req (1.0.0)"),
        "got: {}",
        display.display_lossy()
    );
    assert!(
        display.contains_str("Writing lock file"),
        "got: {}",
        display.display_lossy()
    );
    assert!(
        display.contains_str(
            "Removal failed, root/req is still present, it may be required by another package. See `shirabe why root/req`"
        ),
        "got: {}",
        display.display_lossy()
    );
    assert!(
        is_empty_json(&read_json_file("./composer.json")),
        "expected composer.json to be empty, got: {}",
        read_json_file("./composer.json")
    );
    assert!(
        is_empty_json(&read_json_file("./composer.lock")["packages"]),
        "expected composer.lock packages to be empty, got: {}",
        read_json_file("./composer.lock")["packages"]
    );
    assert_eq!(
        serde_json::json!([{
            "name": "root/req",
            "version": "1.0.0",
            "version_normalized": "1.0.0.0",
            "type": "library",
            "install-path": "../root/req"
        }]),
        read_json_file("./vendor/composer/installed.json")["packages"]
    );

    drop(tear_down);
}

/// ref: RemoveCommandTest::testUpdateInheritedDependenciesFlagIsPassedToPostRemoveInstaller
/// (data provider provideInheritedDependenciesUpdateFlag).
fn run_update_inherited_dependencies_flag_case(
    install_flag_name: &str,
    expected_composer_update_command: &str,
) {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "repositories": {
                "packages": {
                    "type": "package",
                    "package": [
                        {"name": "root/req", "version": "1.0.0", "type": "metapackage"},
                    ],
                },
            },
            "require": { "root/req": "1.*" },
        })),
        None,
        None,
        true,
    );
    let root_req_package = get_package("root/req", "1.0.0");
    root_req_package.__set_type(bstr::BString::from("metapackage".to_string()));

    create_installed_json(std::slice::from_ref(&root_req_package), &[], true);
    create_composer_lock(std::slice::from_ref(&root_req_package), &[]);

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            input(vec![
                ("command", InputValue::from("remove")),
                ("packages", InputValue::Array(vec!["root/req".into()])),
                (install_flag_name, InputValue::from(true)),
                ("--no-audit", InputValue::from(true)),
                ("--no-interaction", InputValue::from(true)),
            ]),
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(SUCCESS, status_code);
    let display = app_tester.get_display();
    assert!(
        display.contains_str("./composer.json has been updated"),
        "got: {}",
        display.display_lossy()
    );
    assert!(
        display.contains_str(expected_composer_update_command),
        "got: {}",
        display.display_lossy()
    );
    assert!(
        display.contains_str("Package operations: 0 installs, 0 updates, 1 removal"),
        "got: {}",
        display.display_lossy()
    );
    assert!(
        display.contains_str("- Removing root/req (1.0.0)"),
        "got: {}",
        display.display_lossy()
    );
    assert!(
        display.contains_str("Writing lock file"),
        "got: {}",
        display.display_lossy()
    );
    assert!(
        display.contains_str("Lock file operations: 0 installs, 0 updates, 1 removal"),
        "got: {}",
        display.display_lossy()
    );
    assert!(
        is_empty_json(&read_json_file("./composer.lock")["packages"]),
        "expected composer.lock packages to be empty, got: {}",
        read_json_file("./composer.lock")["packages"]
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_update_inherited_dependencies_flag_is_passed_to_post_remove_installer() {
    // 'update with all dependencies'
    run_update_inherited_dependencies_flag_case(
        "--update-with-all-dependencies",
        "Running shirabe update root/req --with-all-dependencies",
    );
    // 'with all dependencies'
    run_update_inherited_dependencies_flag_case(
        "--with-all-dependencies",
        "Running shirabe update root/req --with-all-dependencies",
    );
    // 'no update with dependencies'
    run_update_inherited_dependencies_flag_case(
        "--no-update-with-dependencies",
        "Running shirabe update root/req --with-dependencies",
    );
}
