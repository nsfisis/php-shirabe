//! ref: composer/tests/Composer/Test/Command/InstallCommandTest.php sha1:674debfae1f3df3f994d48d125b0304f70f47c28

use crate::test_case::{
    RunOptions, create_composer_lock, create_installed_json, get_application_tester, get_package,
    init_temp_composer,
};
use bstr::{BStr, BString};
use shirabe_php_shim::trim;
use shirabe_symfony_console::input::{InputValue, ParameterName};

fn input(pairs: Vec<(&BStr, InputValue)>) -> Vec<(ParameterName, InputValue)> {
    pairs
        .into_iter()
        .map(|(k, v)| (ParameterName::of(k), v))
        .collect()
}

/// ref: InstallCommandTest::errorCaseProvider
fn error_cases() -> Vec<(
    &'static BStr,
    serde_json::Value,
    Vec<(&'static BStr, InputValue)>,
    &'static BStr,
)> {
    vec![
        (
            BStr::new("it writes an error when the dev flag is passed"),
            serde_json::json!({ "repositories": [] }),
            vec![(BStr::new("--dev"), InputValue::from(true))],
            BStr::new(
                r#"<warning>You are using the deprecated option "--dev". It has no effect and will break in Composer 3.</warning>
Installing dependencies from lock file (including require-dev)
Verifying lock file contents can be installed on current platform.
Nothing to install, update or remove
Generating autoload files"#,
            ),
        ),
        (
            BStr::new("it writes an error when no-suggest flag passed"),
            serde_json::json!({ "repositories": [] }),
            vec![(BStr::new("--no-suggest"), InputValue::from(true))],
            BStr::new(
                r#"<warning>You are using the deprecated option "--no-suggest". It has no effect and will break in Composer 3.</warning>
Installing dependencies from lock file (including require-dev)
Verifying lock file contents can be installed on current platform.
Nothing to install, update or remove
Generating autoload files"#,
            ),
        ),
        (
            BStr::new("it writes an error when packages passed"),
            serde_json::json!({ "repositories": [] }),
            vec![(
                BStr::new("packages"),
                InputValue::Array(vec!["vendor/package".into()]),
            )],
            BStr::new(
                r#"Invalid argument vendor/package. Use "shirabe require vendor/package" instead to add packages to your composer.json."#,
            ),
        ),
        (
            BStr::new("it writes an error when no-install flag is passed"),
            serde_json::json!({ "repositories": [] }),
            vec![(BStr::new("--no-install"), InputValue::from(true))],
            BStr::new(
                r#"Invalid option "--no-install". Use "shirabe update --no-install" instead if you are trying to update the composer.lock file."#,
            ),
        ),
    ]
}

#[test]
#[serial_test::serial]
fn test_install_command_errors() {
    for (label, composer_json, command, expected) in error_cases() {
        let _tear_down = init_temp_composer(Some(&composer_json), None, None, true);

        let packages = [get_package("vendor/package", "1.2.3")];
        let dev_packages = [get_package("vendor/devpackage", "2.3.4")];

        create_composer_lock(&packages, &dev_packages);
        create_installed_json(&packages, &dev_packages, true);

        let mut app_tester = get_application_tester();
        let mut args = vec![(BStr::new("command"), InputValue::from("install"))];
        args.extend(command);
        let _ = app_tester.run(input(args), RunOptions::default());

        assert_eq!(
            trim(expected),
            trim(app_tester.get_display()),
            "case: {label}",
            label = label.display_lossy()
        );
    }
}

#[test]
#[serial_test::serial]
fn test_install_from_empty_vendor() {
    let composer_json = serde_json::json!({
        "require": { "root/req": "1.*" },
        "require-dev": { "root/another": "1.*" },
    });
    let _tear_down = init_temp_composer(Some(&composer_json), None, None, true);

    let root_req_package = get_package("root/req", "1.0.0");
    let another_package = get_package("root/another", "1.0.0");
    // Set as a metapackage so that we can do the whole post-remove update & install process without
    // Composer trying to download them (DownloadManager::getDownloaderForPackage).
    root_req_package.__set_type(BString::from("metapackage"));
    another_package.__set_type(BString::from("metapackage"));

    create_composer_lock(&[root_req_package], &[another_package]);

    let mut app_tester = get_application_tester();
    app_tester
        .run(
            input(vec![
                (BStr::new("command"), InputValue::from("install")),
                (BStr::new("--no-progress"), InputValue::from(true)),
            ]),
            RunOptions::default(),
        )
        .unwrap();

    assert_eq!(
        "Installing dependencies from lock file (including require-dev)
Verifying lock file contents can be installed on current platform.
Package operations: 2 installs, 0 updates, 0 removals
  - Installing root/another (1.0.0)
  - Installing root/req (1.0.0)
Generating autoload files",
        trim(app_tester.get_display())
    );
}

#[test]
#[serial_test::serial]
fn test_install_from_empty_vendor_no_dev() {
    let composer_json = serde_json::json!({
        "require": { "root/req": "1.*" },
        "require-dev": { "root/another": "1.*" },
    });
    let _tear_down = init_temp_composer(Some(&composer_json), None, None, true);

    let root_req_package = get_package("root/req", "1.0.0");
    let another_package = get_package("root/another", "1.0.0");
    // Set as a metapackage so that we can do the whole post-remove update & install process without
    // Composer trying to download them (DownloadManager::getDownloaderForPackage).
    root_req_package.__set_type(BString::from("metapackage"));
    another_package.__set_type(BString::from("metapackage"));

    create_composer_lock(&[root_req_package], &[another_package]);

    let mut app_tester = get_application_tester();
    app_tester
        .run(
            input(vec![
                (BStr::new("command"), InputValue::from("install")),
                (BStr::new("--no-progress"), InputValue::from(true)),
                (BStr::new("--no-dev"), InputValue::from(true)),
            ]),
            RunOptions::default(),
        )
        .unwrap();

    assert_eq!(
        "Installing dependencies from lock file
Verifying lock file contents can be installed on current platform.
Package operations: 1 install, 0 updates, 0 removals
  - Installing root/req (1.0.0)
Generating autoload files",
        trim(app_tester.get_display())
    );
}

#[test]
#[serial_test::serial]
fn test_install_new_packages_with_existing_partial_vendor() {
    let composer_json = serde_json::json!({
        "require": {
            "root/req": "1.*",
            "root/another": "1.*",
        },
    });
    let _tear_down = init_temp_composer(Some(&composer_json), None, None, true);

    let root_req_package = get_package("root/req", "1.0.0");
    let another_package = get_package("root/another", "1.0.0");
    // Set as a metapackage so that we can do the whole post-remove update & install process without
    // Composer trying to download them (DownloadManager::getDownloaderForPackage).
    root_req_package.__set_type(BString::from("metapackage"));
    another_package.__set_type(BString::from("metapackage"));

    create_composer_lock(&[root_req_package.clone(), another_package], &[]);
    create_installed_json(&[root_req_package], &[], true);

    let mut app_tester = get_application_tester();
    app_tester
        .run(
            input(vec![
                (BStr::new("command"), InputValue::from("install")),
                (BStr::new("--no-progress"), InputValue::from(true)),
            ]),
            RunOptions::default(),
        )
        .unwrap();

    assert_eq!(
        "Installing dependencies from lock file (including require-dev)
Verifying lock file contents can be installed on current platform.
Package operations: 1 install, 0 updates, 0 removals
  - Installing root/another (1.0.0)
Generating autoload files",
        trim(app_tester.get_display())
    );
}
