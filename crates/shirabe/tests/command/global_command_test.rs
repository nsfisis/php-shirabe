//! ref: composer/tests/Composer/Test/Command/GlobalCommandTest.php sha1:54897c80f4687529818c2caf72d311dbd69947f5

use crate::test_case::{
    RunOptions, create_composer_lock, create_installed_json, get_application_tester,
    get_complete_package, get_package, init_temp_composer,
};
use bstr::{BString, ByteSlice};
use serial_test::serial;
use shirabe::util::platform::Platform;
use shirabe_php_shim::{bconcat, error_message, path_to_bytes};
use shirabe_symfony_console::input::{InputValue, ParameterName};
use std::path::PathBuf;
use tempfile::TempDir;

/// ref: GlobalCommandTest::tearDown (the COMPOSER part; COMPOSER_HOME is restored by the
/// `init_temp_composer` guard).
struct EnvGuard;

impl Drop for EnvGuard {
    fn drop(&mut self) {
        Platform::clear_env("COMPOSER").unwrap();
    }
}

/// ref: TestCase::getUniqueTmpDirectory followed by `chdir($dir)`.
///
/// Returns the temp dir guard (kept alive by the caller so it is removed at end of scope).
fn chdir_unique_tmp_directory() -> TempDir {
    let test_dir = TempDir::new().unwrap();
    std::env::set_current_dir(test_dir.path()).unwrap();
    test_dir
}

#[test]
#[serial]
fn test_global() {
    let _env_guard = EnvGuard;

    let script = r#"@php -r "echo 'COMPOSER SCRIPT OUTPUT: '.getenv('COMPOSER') . PHP_EOL;""#;
    let fake_composer = "TMP_COMPOSER.JSON";
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "scripts": {
                "test-script": script,
            },
        })),
        None,
        None,
        true,
    );
    let composer_home: PathBuf = tear_down.working_dir();
    let composer_home_str = composer_home.to_path_buf();

    Platform::put_env("COMPOSER_HOME", path_to_bytes(&composer_home_str)).unwrap();
    Platform::put_env("COMPOSER", fake_composer).unwrap();

    let _test_dir = chdir_unique_tmp_directory();

    let mut app_tester = get_application_tester();
    let _ = app_tester.run(
        vec![
            (ParameterName::of("command"), InputValue::from("global")),
            (
                ParameterName::of("command-name"),
                InputValue::from("test-script"),
            ),
            (
                ParameterName::of("--no-interaction"),
                InputValue::from(true),
            ),
        ],
        RunOptions::default(),
    );

    let display = app_tester.get_display();

    assert_eq!(
        bconcat!(
            "Changed current directory to ",
            composer_home_str,
            "\nCOMPOSER SCRIPT OUTPUT: \n"
        ),
        display
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_cannot_create_home() {
    let _env_guard = EnvGuard;

    let dir = TempDir::new().unwrap();
    let filename = dir.path().join("file");
    std::fs::write(&filename, "").unwrap();
    let filename_str = filename.to_path_buf();

    Platform::put_env("COMPOSER_HOME", path_to_bytes(&filename_str)).unwrap();

    let mut app_tester = get_application_tester();
    let err = app_tester
        .run(
            vec![
                (ParameterName::of("command"), InputValue::from("global")),
                (
                    ParameterName::of("command-name"),
                    InputValue::from("test-script"),
                ),
                (
                    ParameterName::of("--no-interaction"),
                    InputValue::from(true),
                ),
            ],
            RunOptions::default(),
        )
        .expect_err("expected RuntimeException for a non-directory COMPOSER_HOME");

    assert!(
        error_message(&err).contains_str(bconcat!(filename_str, " exists and is not a directory.")),
        "got: {}",
        err
    );

    Platform::clear_env("COMPOSER_HOME").unwrap();
}

#[test]
#[serial]
fn test_global_show() {
    let _env_guard = EnvGuard;

    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "repositories": {
                "packages": {
                    "type": "package",
                    "package": [
                        {"name": "vendor/global-tool", "version": "1.0.0"},
                    ],
                },
            },
            "require": {
                "vendor/global-tool": "1.0.0",
            },
        })),
        None,
        None,
        true,
    );
    let composer_home_str = tear_down.working_dir().to_path_buf();

    let pkg = get_complete_package("vendor/global-tool", "1.0.0");
    pkg.set_description(BString::from("A globally installed tool"));
    create_installed_json(&[pkg.into()], &[], true);

    Platform::put_env("COMPOSER_HOME", path_to_bytes(&composer_home_str)).unwrap();

    let _test_dir = chdir_unique_tmp_directory();

    let mut app_tester = get_application_tester();
    app_tester.set_inputs(vec![BString::from("")]);
    let _ = app_tester.run(
        vec![
            (ParameterName::of("command"), InputValue::from("global")),
            (ParameterName::of("command-name"), InputValue::from("show")),
        ],
        RunOptions::default(),
    );

    let output = app_tester.get_display();
    assert!(
        output.contains_str("vendor/global-tool"),
        "got: {}",
        output.display_lossy()
    );
    assert!(
        output.contains_str("1.0.0"),
        "got: {}",
        output.display_lossy()
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_global_show_without_packages() {
    let _env_guard = EnvGuard;

    let tear_down = init_temp_composer(None, None, None, true);
    let composer_home_str = tear_down.working_dir().to_path_buf();

    create_installed_json(&[], &[], true);

    Platform::put_env("COMPOSER_HOME", path_to_bytes(&composer_home_str)).unwrap();

    let _test_dir = chdir_unique_tmp_directory();

    let mut app_tester = get_application_tester();
    app_tester.set_inputs(vec![BString::from("")]);
    let status_code = app_tester
        .run(
            vec![
                (ParameterName::of("command"), InputValue::from("global")),
                (ParameterName::of("command-name"), InputValue::from("show")),
            ],
            RunOptions::default(),
        )
        .unwrap();

    assert_eq!(0, status_code);

    drop(tear_down);
}

#[test]
#[serial]
#[ignore = "installs a package via the global command's require sub-dispatch, which panics inside the event dispatcher: \"Subscriber ?::? for event pre-operations-exec is not callable\""]
fn test_global_require() {
    let _env_guard = EnvGuard;

    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "repositories": {
                "packages": {
                    "type": "package",
                    "package": [
                        {
                            "name": "vendor/required-pkg",
                            "version": "2.0.0",
                            "dist": {"type": "file", "url": file!()},
                        },
                    ],
                },
            },
        })),
        None,
        None,
        true,
    );
    let composer_home_str = tear_down.working_dir().to_path_buf();

    Platform::put_env("COMPOSER_HOME", path_to_bytes(&composer_home_str)).unwrap();

    let _test_dir = chdir_unique_tmp_directory();

    let mut app_tester = get_application_tester();
    app_tester.set_inputs(vec![BString::from("")]);
    let status_code = app_tester
        .run(
            vec![
                (ParameterName::of("command"), InputValue::from("global")),
                (
                    ParameterName::of("command-name"),
                    InputValue::from("require"),
                ),
                (
                    ParameterName::of("packages"),
                    InputValue::Array(vec!["vendor/required-pkg:2.0.0".into()]),
                ),
            ],
            RunOptions::default(),
        )
        .unwrap();

    assert_eq!(0, status_code);
    assert!(
        app_tester
            .get_display()
            .contains_str("Installing vendor/required-pkg"),
        "got: {}",
        app_tester.get_display().display_lossy()
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_global_update() {
    let _env_guard = EnvGuard;

    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "repositories": {
                "packages": {
                    "type": "package",
                    "package": [
                        {"name": "vendor/pkg", "version": "1.0.0"},
                    ],
                },
            },
            "require": {
                "vendor/pkg": "1.0.0",
            },
        })),
        None,
        None,
        true,
    );
    let composer_home_str = tear_down.working_dir().to_path_buf();

    let pkg = get_package("vendor/pkg", "1.0.0");
    create_installed_json(std::slice::from_ref(&pkg), &[], true);
    create_composer_lock(&[pkg], &[]);

    Platform::put_env("COMPOSER_HOME", path_to_bytes(&composer_home_str)).unwrap();

    let _test_dir = chdir_unique_tmp_directory();

    let mut app_tester = get_application_tester();
    app_tester.set_inputs(vec![BString::from("")]);
    let status_code = app_tester
        .run(
            vec![
                (ParameterName::of("command"), InputValue::from("global")),
                (
                    ParameterName::of("command-name"),
                    InputValue::from("update"),
                ),
            ],
            RunOptions::default(),
        )
        .unwrap();

    assert_eq!(0, status_code);

    drop(tear_down);
}

#[test]
#[serial]
fn test_global_changes_directory() {
    let _env_guard = EnvGuard;

    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "name": "test/global",
        })),
        None,
        None,
        true,
    );
    let composer_home_str = tear_down.working_dir().to_path_buf();

    Platform::put_env("COMPOSER_HOME", path_to_bytes(&composer_home_str)).unwrap();

    let _test_dir = chdir_unique_tmp_directory();

    let mut app_tester = get_application_tester();
    app_tester.set_inputs(vec![BString::from("")]);
    let _ = app_tester.run(
        vec![
            (ParameterName::of("command"), InputValue::from("global")),
            (
                ParameterName::of("command-name"),
                InputValue::from("config"),
            ),
            (ParameterName::of("setting-key"), InputValue::from("name")),
        ],
        RunOptions::default(),
    );

    let output = app_tester.get_display();
    assert!(
        output.contains_str(bconcat!("Changed current directory to ", composer_home_str)),
        "got: {}",
        output.display_lossy()
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_global_missing_command_name() {
    let _env_guard = EnvGuard;

    let tear_down = init_temp_composer(None, None, None, true);
    let composer_home_str = tear_down.working_dir().to_path_buf();

    Platform::put_env("COMPOSER_HOME", path_to_bytes(&composer_home_str)).unwrap();

    let mut app_tester = get_application_tester();
    app_tester.set_inputs(vec![BString::from("")]);
    let err = app_tester
        .run(
            vec![(ParameterName::of("command"), InputValue::from("global"))],
            RunOptions::default(),
        )
        .expect_err("expected a RuntimeException for the missing command-name argument");

    assert!(
        error_message(&err).contains_str("Not enough arguments (missing: \"command-name\")"),
        "got: {}",
        err
    );

    drop(tear_down);
}
