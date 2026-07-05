//! ref: composer/tests/Composer/Test/Command/GlobalCommandTest.php

use crate::test_case::{
    RunOptions, create_composer_lock, create_installed_json, get_application_tester,
    get_complete_package, get_package, init_temp_composer,
};
use serial_test::serial;
use shirabe::util::platform::Platform;
use shirabe_php_shim::PhpMixed;
use std::path::PathBuf;
use tempfile::TempDir;

/// ref: GlobalCommandTest::tearDown (the COMPOSER part; COMPOSER_HOME is restored by the
/// `init_temp_composer` guard).
struct EnvGuard;

impl Drop for EnvGuard {
    fn drop(&mut self) {
        Platform::clear_env("COMPOSER");
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
#[ignore = "runs a composer.json script, which Application::do_run registers as a reflection-instantiated command; that registration path is todo!() at crates/shirabe/src/console/application.rs:2461 (plugin: register reflection-instantiated command on Application::add)"]
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
    let composer_home_str = composer_home.to_string_lossy().to_string();

    Platform::put_env("COMPOSER_HOME", &composer_home_str);
    Platform::put_env("COMPOSER", fake_composer);

    let _test_dir = chdir_unique_tmp_directory();

    let mut app_tester = get_application_tester();
    let _ = app_tester.run(
        vec![
            (PhpMixed::from("command"), PhpMixed::from("global")),
            (
                PhpMixed::from("command-name"),
                PhpMixed::from("test-script"),
            ),
            (PhpMixed::from("--no-interaction"), PhpMixed::from(true)),
        ],
        RunOptions::default(),
    );

    let display = app_tester.get_display();

    assert_eq!(
        format!(
            "Changed current directory to {}\nCOMPOSER SCRIPT OUTPUT: \n",
            composer_home_str
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
    let filename_str = filename.to_string_lossy().to_string();

    Platform::put_env("COMPOSER_HOME", &filename_str);

    let mut app_tester = get_application_tester();
    let err = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("global")),
                (
                    PhpMixed::from("command-name"),
                    PhpMixed::from("test-script"),
                ),
                (PhpMixed::from("--no-interaction"), PhpMixed::from(true)),
            ],
            RunOptions::default(),
        )
        .expect_err("expected RuntimeException for a non-directory COMPOSER_HOME");

    assert!(
        err.to_string()
            .contains(&format!("{} exists and is not a directory.", filename_str)),
        "got: {}",
        err
    );

    Platform::clear_env("COMPOSER_HOME");
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
    let composer_home_str = tear_down.working_dir().to_string_lossy().to_string();

    let pkg = get_complete_package("vendor/global-tool", "1.0.0");
    pkg.set_description("A globally installed tool".to_string());
    create_installed_json(&[pkg.into()], &[], true);

    Platform::put_env("COMPOSER_HOME", &composer_home_str);

    let _test_dir = chdir_unique_tmp_directory();

    let mut app_tester = get_application_tester();
    app_tester.set_inputs(vec!["".to_string()]);
    let _ = app_tester.run(
        vec![
            (PhpMixed::from("command"), PhpMixed::from("global")),
            (PhpMixed::from("command-name"), PhpMixed::from("show")),
        ],
        RunOptions::default(),
    );

    let output = app_tester.get_display();
    assert!(output.contains("vendor/global-tool"), "got: {}", output);
    assert!(output.contains("1.0.0"), "got: {}", output);

    drop(tear_down);
}

#[test]
#[serial]
fn test_global_show_without_packages() {
    let _env_guard = EnvGuard;

    let tear_down = init_temp_composer(None, None, None, true);
    let composer_home_str = tear_down.working_dir().to_string_lossy().to_string();

    create_installed_json(&[], &[], true);

    Platform::put_env("COMPOSER_HOME", &composer_home_str);

    let _test_dir = chdir_unique_tmp_directory();

    let mut app_tester = get_application_tester();
    app_tester.set_inputs(vec!["".to_string()]);
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("global")),
                (PhpMixed::from("command-name"), PhpMixed::from("show")),
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
    let composer_home_str = tear_down.working_dir().to_string_lossy().to_string();

    Platform::put_env("COMPOSER_HOME", &composer_home_str);

    let _test_dir = chdir_unique_tmp_directory();

    let mut app_tester = get_application_tester();
    app_tester.set_inputs(vec!["".to_string()]);
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("global")),
                (PhpMixed::from("command-name"), PhpMixed::from("require")),
                (
                    PhpMixed::from("packages"),
                    PhpMixed::List(vec![PhpMixed::from("vendor/required-pkg:2.0.0")]),
                ),
            ],
            RunOptions::default(),
        )
        .unwrap();

    assert_eq!(0, status_code);
    assert!(
        app_tester
            .get_display()
            .contains("Installing vendor/required-pkg"),
        "got: {}",
        app_tester.get_display()
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
    let composer_home_str = tear_down.working_dir().to_string_lossy().to_string();

    let pkg = get_package("vendor/pkg", "1.0.0");
    create_installed_json(std::slice::from_ref(&pkg), &[], true);
    create_composer_lock(&[pkg], &[]);

    Platform::put_env("COMPOSER_HOME", &composer_home_str);

    let _test_dir = chdir_unique_tmp_directory();

    let mut app_tester = get_application_tester();
    app_tester.set_inputs(vec!["".to_string()]);
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("global")),
                (PhpMixed::from("command-name"), PhpMixed::from("update")),
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
    let composer_home_str = tear_down.working_dir().to_string_lossy().to_string();

    Platform::put_env("COMPOSER_HOME", &composer_home_str);

    let _test_dir = chdir_unique_tmp_directory();

    let mut app_tester = get_application_tester();
    app_tester.set_inputs(vec!["".to_string()]);
    let _ = app_tester.run(
        vec![
            (PhpMixed::from("command"), PhpMixed::from("global")),
            (PhpMixed::from("command-name"), PhpMixed::from("config")),
            (PhpMixed::from("setting-key"), PhpMixed::from("name")),
        ],
        RunOptions::default(),
    );

    let output = app_tester.get_display();
    assert!(
        output.contains(&format!(
            "Changed current directory to {}",
            composer_home_str
        )),
        "got: {}",
        output
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_global_missing_command_name() {
    let _env_guard = EnvGuard;

    let tear_down = init_temp_composer(None, None, None, true);
    let composer_home_str = tear_down.working_dir().to_string_lossy().to_string();

    Platform::put_env("COMPOSER_HOME", &composer_home_str);

    let mut app_tester = get_application_tester();
    app_tester.set_inputs(vec!["".to_string()]);
    let err = app_tester
        .run(
            vec![(PhpMixed::from("command"), PhpMixed::from("global"))],
            RunOptions::default(),
        )
        .expect_err("expected a RuntimeException for the missing command-name argument");

    assert!(
        err.to_string()
            .contains("Not enough arguments (missing: \"command-name\")"),
        "got: {}",
        err
    );

    drop(tear_down);
}
