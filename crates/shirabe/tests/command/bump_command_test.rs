//! ref: composer/tests/Composer/Test/Command/BumpCommandTest.php

use crate::test_case::{
    RunOptions, create_composer_lock, create_installed_json, get_application_tester,
    get_complete_package, init_temp_composer,
};
use serial_test::serial;
use shirabe::json::JsonFile;
use shirabe::package::handle::PackageInterfaceHandle;
use shirabe_php_shim::PhpMixed;

/// ref: BumpCommandTest::testBump (data provider rolled into one body).
fn run_bump_case(
    composer_json: serde_json::Value,
    command: &[(&str, PhpMixed)],
    expected: serde_json::Value,
    lock: bool,
    exit_code: i32,
) {
    let tear_down = init_temp_composer(Some(&composer_json), None, None, true);

    let packages: Vec<PackageInterfaceHandle> = vec![
        get_complete_package("first/pkg", "2.3.4").into(),
        get_complete_package("second/pkg", "3.4.0").into(),
    ];
    let dev_packages: Vec<PackageInterfaceHandle> =
        vec![get_complete_package("dev/pkg", "2.3.4.5").into()];

    create_installed_json(&packages, &dev_packages, true);
    if lock {
        create_composer_lock(&packages, &dev_packages);
    }

    let mut input: Vec<(PhpMixed, PhpMixed)> =
        vec![(PhpMixed::from("command"), PhpMixed::from("bump"))];
    for (k, v) in command {
        input.push((PhpMixed::from(*k), v.clone()));
    }

    let mut app_tester = get_application_tester();
    let status_code = app_tester.run(input, RunOptions::default()).unwrap();
    assert_eq!(exit_code, status_code);

    let mut json = JsonFile::new("./composer.json".to_string(), None, None).unwrap();
    let read = json.read().unwrap();
    let actual: serde_json::Value = serde_json::from_str(&JsonFile::encode(&read)).unwrap();
    assert_eq!(expected, actual);

    drop(tear_down);
}

#[test]
#[serial]
fn test_bump() {
    // 'bump all by default'
    run_bump_case(
        serde_json::json!({
            "require": { "first/pkg": "^v2.0", "second/pkg": "3.*" },
            "require-dev": { "dev/pkg": "~2.0" },
        }),
        &[],
        serde_json::json!({
            "require": { "first/pkg": "^2.3.4", "second/pkg": "^3.4" },
            "require-dev": { "dev/pkg": "^2.3.4.5" },
        }),
        true,
        0,
    );

    // 'bump only dev with --dev-only'
    run_bump_case(
        serde_json::json!({
            "require": { "first/pkg": "^2.0", "second/pkg": "3.*" },
            "require-dev": { "dev/pkg": "~2.0" },
        }),
        &[("--dev-only", PhpMixed::from(true))],
        serde_json::json!({
            "require": { "first/pkg": "^2.0", "second/pkg": "3.*" },
            "require-dev": { "dev/pkg": "^2.3.4.5" },
        }),
        true,
        0,
    );

    // 'bump only non-dev with --no-dev-only'
    run_bump_case(
        serde_json::json!({
            "require": { "first/pkg": "^2.0", "second/pkg": "3.*" },
            "require-dev": { "dev/pkg": "~2.0" },
        }),
        &[("--no-dev-only", PhpMixed::from(true))],
        serde_json::json!({
            "require": { "first/pkg": "^2.3.4", "second/pkg": "^3.4" },
            "require-dev": { "dev/pkg": "~2.0" },
        }),
        true,
        0,
    );

    // 'bump only listed with packages arg'
    run_bump_case(
        serde_json::json!({
            "require": { "first/pkg": "^2.0", "second/pkg": "3.*" },
            "require-dev": { "dev/pkg": "~2.0" },
        }),
        &[(
            "packages",
            PhpMixed::List(vec![
                PhpMixed::from("first/pkg:3.0.1"),
                PhpMixed::from("dev/*"),
            ]),
        )],
        serde_json::json!({
            "require": { "first/pkg": "^2.3.4", "second/pkg": "3.*" },
            "require-dev": { "dev/pkg": "^2.3.4.5" },
        }),
        true,
        0,
    );

    // 'bump works from installed repo without lock file'
    run_bump_case(
        serde_json::json!({
            "require": { "first/pkg": "^2.0", "second/pkg": "3.*" },
        }),
        &[],
        serde_json::json!({
            "require": { "first/pkg": "^2.3.4", "second/pkg": "^3.4" },
        }),
        false,
        0,
    );

    // 'bump with --dry-run with packages to bump'
    run_bump_case(
        serde_json::json!({
            "require": { "first/pkg": "^2.0", "second/pkg": "3.*" },
            "require-dev": { "dev/pkg": "~2.0" },
        }),
        &[("--dry-run", PhpMixed::from(true))],
        serde_json::json!({
            "require": { "first/pkg": "^2.0", "second/pkg": "3.*" },
            "require-dev": { "dev/pkg": "~2.0" },
        }),
        true,
        1,
    );

    // 'bump with --dry-run without packages to bump'
    run_bump_case(
        serde_json::json!({
            "require": { "first/pkg": "^2.3.4", "second/pkg": "^3.4" },
            "require-dev": { "dev/pkg": "^2.3.4.5" },
        }),
        &[("--dry-run", PhpMixed::from(true))],
        serde_json::json!({
            "require": { "first/pkg": "^2.3.4", "second/pkg": "^3.4" },
            "require-dev": { "dev/pkg": "^2.3.4.5" },
        }),
        true,
        0,
    );

    // 'bump works with non-standard package'
    run_bump_case(
        serde_json::json!({
            "require": { "php": ">=5.3", "first/pkg": "^2.3.4", "second/pkg": "^3.4" },
            "require-dev": { "dev/pkg": "^2.3.4.5" },
        }),
        &[],
        serde_json::json!({
            "require": { "php": ">=5.3", "first/pkg": "^2.3.4", "second/pkg": "^3.4" },
            "require-dev": { "dev/pkg": "^2.3.4.5" },
        }),
        true,
        0,
    );

    // 'bump works with unknown package'
    run_bump_case(
        serde_json::json!({
            "require": { "first/pkg": "^2.3.4", "second/pkg": "^3.4", "third/pkg": "^1.2" },
        }),
        &[],
        serde_json::json!({
            "require": { "first/pkg": "^2.3.4", "second/pkg": "^3.4", "third/pkg": "^1.2" },
        }),
        true,
        0,
    );

    // 'bump works with aliased package'
    run_bump_case(
        serde_json::json!({
            "require": { "first/pkg": "^2.3.4", "second/pkg": "dev-bugfix as 3.4.x-dev" },
        }),
        &[],
        serde_json::json!({
            "require": { "first/pkg": "^2.3.4", "second/pkg": "dev-bugfix as 3.4.x-dev" },
        }),
        true,
        0,
    );
}

#[test]
#[serial]
fn test_bump_fails_on_non_existing_composer_file() {
    let tear_down = init_temp_composer(Some(&serde_json::json!({})), None, None, false);
    let composer_json_path = tear_down.working_dir().join("composer.json");
    std::fs::remove_file(&composer_json_path).unwrap();

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![(PhpMixed::from("command"), PhpMixed::from("bump"))],
            RunOptions {
                capture_stderr_separately: true,
                ..RunOptions::default()
            },
        )
        .unwrap();

    assert_eq!(1, status_code);
    let error_output = app_tester.get_error_output();
    assert!(
        error_output.contains("./composer.json is not readable."),
        "expected error output to mention composer.json not readable, got: {:?}",
        error_output,
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_bump_fails_on_write_error_to_composer_file() {
    if shirabe_php_shim::function_exists("posix_getuid") && shirabe_php_shim::posix_getuid() == 0 {
        // ref: $this->markTestSkipped('Cannot run as root');
        return;
    }

    let tear_down = init_temp_composer(Some(&serde_json::json!({})), None, None, false);
    let composer_json_path = tear_down.working_dir().join("composer.json");
    shirabe_php_shim::chmod(&composer_json_path.to_string_lossy(), 0o444);

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![(PhpMixed::from("command"), PhpMixed::from("bump"))],
            RunOptions {
                capture_stderr_separately: true,
                ..RunOptions::default()
            },
        )
        .unwrap();

    assert_eq!(1, status_code);
    let error_output = app_tester.get_error_output();
    assert!(
        error_output.contains("./composer.json is not writable."),
        "expected error output to mention composer.json not writable, got: {:?}",
        error_output,
    );

    drop(tear_down);
}
