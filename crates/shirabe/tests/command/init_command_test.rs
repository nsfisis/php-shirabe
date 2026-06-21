//! ref: composer/tests/Composer/Test/Command/InitCommandTest.php

// The author/namespace/git-config helpers are protected methods exercised via reflection
// in PHP; the run cases need the ApplicationTester. Neither is available here.

use shirabe_php_shim::server_set;

fn set_up() {
    server_set("COMPOSER_DEFAULT_AUTHOR", "John Smith".to_string());
    server_set("COMPOSER_DEFAULT_EMAIL", "john@example.com".to_string());
}

#[test]
#[ignore = "needs the ApplicationTester harness or reflection into protected InitCommand helpers"]
fn test_parse_valid_author_string() {
    set_up();

    todo!()
}

#[test]
#[ignore = "needs the ApplicationTester harness or reflection into protected InitCommand helpers"]
fn test_parse_empty_author_string() {
    set_up();

    todo!()
}

#[test]
#[ignore = "needs the ApplicationTester harness or reflection into protected InitCommand helpers"]
fn test_parse_author_string_with_invalid_email() {
    set_up();

    todo!()
}

#[test]
#[ignore = "needs the ApplicationTester harness or reflection into protected InitCommand helpers"]
fn test_namespace_from_valid_package_name() {
    set_up();

    todo!()
}

#[test]
#[ignore = "needs the ApplicationTester harness or reflection into protected InitCommand helpers"]
fn test_namespace_from_invalid_package_name() {
    set_up();

    todo!()
}

#[test]
#[ignore = "needs the ApplicationTester harness or reflection into protected InitCommand helpers"]
fn test_namespace_from_missing_package_name() {
    set_up();

    todo!()
}

#[test]
#[ignore = "needs the ApplicationTester harness or reflection into protected InitCommand helpers"]
fn test_run_command() {
    set_up();

    todo!()
}

#[test]
#[ignore = "needs the ApplicationTester harness or reflection into protected InitCommand helpers"]
fn test_run_command_invalid() {
    set_up();

    todo!()
}

#[test]
#[ignore = "needs the ApplicationTester harness or reflection into protected InitCommand helpers"]
fn test_run_guess_name_from_dir_sanitizes_dir() {
    set_up();

    todo!()
}

#[test]
#[ignore = "needs the ApplicationTester harness or reflection into protected InitCommand helpers"]
fn test_interactive_run() {
    set_up();

    todo!()
}

#[test]
#[ignore = "needs the ApplicationTester harness or reflection into protected InitCommand helpers"]
fn test_format_authors() {
    set_up();

    todo!()
}

#[test]
#[ignore = "needs the ApplicationTester harness or reflection into protected InitCommand helpers"]
fn test_get_git_config() {
    set_up();

    todo!()
}

#[test]
#[ignore = "needs the ApplicationTester harness or reflection into protected InitCommand helpers"]
fn test_add_vendor_ignore() {
    set_up();

    todo!()
}

#[test]
#[ignore = "needs the ApplicationTester harness or reflection into protected InitCommand helpers"]
fn test_has_vendor_ignore() {
    set_up();

    todo!()
}
