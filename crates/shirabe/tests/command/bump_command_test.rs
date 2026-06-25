//! ref: composer/tests/Composer/Test/Command/BumpCommandTest.php

use crate::test_case::{RunOptions, get_application_tester, init_temp_composer};
use serial_test::serial;
use shirabe_php_shim::PhpMixed;

#[ignore = "needs create_installed_json / create_composer_lock helpers \
            (InstalledFilesystemRepository / Locker::set_lock_data), and BumpCommand::execute calls \
            require_composer -> Factory, reaching ProcessExecutor (git) -> shirabe-php-shim \
            stream_set_blocking (stream.rs todo!(), requires fcntl(2))"]
#[test]
fn test_bump() {
    todo!()
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
#[ignore = "CurlDownloader::new no longer panics (HTTP layer is now reqwest-based), but this path \
            still reaches an unrelated todo!() in shirabe-php-shim stream_set_blocking (stream.rs), \
            which requires fcntl(2). The companion test_bump_fails_on_non_existing_composer_file \
            covers the same error-output capture path without reaching that shim."]
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
