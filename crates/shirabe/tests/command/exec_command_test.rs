//! ref: composer/tests/Composer/Test/Command/ExecCommandTest.php sha1:3c86f24a58d2338489cac207610707609782bea9

use crate::test_case::{RunOptions, get_application_tester, init_temp_composer};
use bstr::ByteSlice as _;
use shirabe_php_shim::{bformat, error_message, trim};
use shirabe_symfony_console::input::{InputValue, ParameterName};

/// ref: ExecCommandTest::testListThrowsIfNoBinariesExist
#[test]
#[serial_test::serial]
fn test_list_throws_if_no_binaries_exist() {
    let tear_down = init_temp_composer(Some(&serde_json::json!({})), None, None, false);
    let composer_dir = tear_down.working_dir();

    let composer_bin_dir = composer_dir.join("vendor/bin");

    let mut app_tester = get_application_tester();
    let err = app_tester
        .run(
            vec![
                (ParameterName::of("command"), InputValue::from("exec")),
                (ParameterName::of("--list"), InputValue::from(true)),
            ],
            RunOptions::default(),
        )
        .expect_err("exec --list with no binaries should raise a RuntimeException");

    assert!(
        error_message(&err).contains_str(bformat!(
            "No binaries found in composer.json or in bin-dir ({composer_bin_dir})"
        )),
        "expected RuntimeException about no binaries, got: {:?}",
        error_message(&err),
    );

    drop(tear_down);
}

/// ref: ExecCommandTest::testList
#[test]
#[serial_test::serial]
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

    let composer_bin_dir = composer_dir.join("vendor/bin");
    std::fs::create_dir_all(&composer_bin_dir).unwrap();
    std::fs::write(composer_bin_dir.join("b"), "").unwrap();
    std::fs::write(composer_bin_dir.join("b.bat"), "").unwrap();
    std::fs::write(composer_bin_dir.join("c"), "").unwrap();

    let mut app_tester = get_application_tester();
    app_tester
        .run(
            vec![
                (ParameterName::of("command"), InputValue::from("exec")),
                (ParameterName::of("--list"), InputValue::from(true)),
            ],
            RunOptions::default(),
        )
        .unwrap();

    let output = app_tester.get_display();

    assert_eq!("Available binaries:\n- b\n- c\n- a (local)", trim(&output));

    drop(tear_down);
}
