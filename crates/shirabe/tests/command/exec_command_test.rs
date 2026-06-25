//! ref: composer/tests/Composer/Test/Command/ExecCommandTest.php

use crate::test_case::{RunOptions, get_application_tester, init_temp_composer};
use serial_test::serial;
use shirabe_php_shim::PhpMixed;

/// ref: ExecCommandTest::testListThrowsIfNoBinariesExist
#[test]
#[serial]
fn test_list_throws_if_no_binaries_exist() {
    let tear_down = init_temp_composer(Some(&serde_json::json!({})), None, None, false);
    let composer_dir = tear_down.working_dir();

    let composer_bin_dir = format!("{}/vendor/bin", composer_dir.display());

    let mut app_tester = get_application_tester();
    let err = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("exec")),
                (PhpMixed::from("--list"), PhpMixed::from(true)),
            ],
            RunOptions::default(),
        )
        .expect_err("exec --list with no binaries should raise a RuntimeException");

    assert!(
        err.to_string().contains(&format!(
            "No binaries found in composer.json or in bin-dir ({})",
            composer_bin_dir
        )),
        "expected RuntimeException about no binaries, got: {:?}",
        err.to_string(),
    );

    drop(tear_down);
}

/// ref: ExecCommandTest::testList
#[test]
#[serial]
fn test_list() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "bin": [
                "a",
            ],
        })),
        None,
        None,
        false,
    );
    let composer_dir = tear_down.working_dir();

    let composer_bin_dir = format!("{}/vendor/bin", composer_dir.display());
    std::fs::create_dir_all(&composer_bin_dir).unwrap();
    std::fs::write(format!("{}/b", composer_bin_dir), "").unwrap();
    std::fs::write(format!("{}/b.bat", composer_bin_dir), "").unwrap();
    std::fs::write(format!("{}/c", composer_bin_dir), "").unwrap();

    let mut app_tester = get_application_tester();
    app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("exec")),
                (PhpMixed::from("--list"), PhpMixed::from(true)),
            ],
            RunOptions::default(),
        )
        .unwrap();

    let output = app_tester.get_display();

    assert_eq!("Available binaries:\n- b\n- c\n- a (local)", output.trim(),);

    drop(tear_down);
}
