//! ref: composer/tests/Composer/Test/Command/InitCommandTest.php

// The author/namespace/git-config helpers are protected methods exercised via reflection
// in PHP; the run cases need the ApplicationTester. Neither is available here.

use shirabe::command::init_command::InitCommand;
use shirabe_php_shim::server_set;

fn set_up() {
    server_set("COMPOSER_DEFAULT_AUTHOR", "John Smith".to_string());
    server_set("COMPOSER_DEFAULT_EMAIL", "john@example.com".to_string());
}

#[ignore = "InitCommand::parse_author_string is private; integration tests cannot reach it (PHP uses reflection)"]
#[test]
fn test_parse_valid_author_string() {
    set_up();

    todo!()
}

#[ignore = "InitCommand::parse_author_string is private; integration tests cannot reach it (PHP uses reflection)"]
#[test]
fn test_parse_empty_author_string() {
    set_up();

    todo!()
}

#[ignore = "InitCommand::parse_author_string is private; integration tests cannot reach it (PHP uses reflection)"]
#[test]
fn test_parse_author_string_with_invalid_email() {
    set_up();

    todo!()
}

#[test]
fn test_namespace_from_valid_package_name() {
    set_up();

    let command = InitCommand::new();
    let namespace = command.namespace_from_package_name("new_projects.acme-extra/package-name");
    assert_eq!(
        Some("NewProjectsAcmeExtra\\PackageName".to_string()),
        namespace
    );
}

#[test]
fn test_namespace_from_invalid_package_name() {
    set_up();

    let command = InitCommand::new();
    let namespace = command.namespace_from_package_name("invalid-package-name");
    assert_eq!(None, namespace);
}

#[test]
fn test_namespace_from_missing_package_name() {
    set_up();

    let command = InitCommand::new();
    let namespace = command.namespace_from_package_name("");
    assert_eq!(None, namespace);
}

#[ignore = "needs TestCase::init_temp_composer and get_application_tester (ApplicationTester) infrastructure, not implemented"]
#[test]
fn test_run_command() {
    set_up();

    todo!()
}

#[ignore = "needs TestCase::init_temp_composer and get_application_tester (ApplicationTester) infrastructure, not implemented"]
#[test]
fn test_run_command_invalid() {
    set_up();

    todo!()
}

#[ignore = "needs TestCase::init_temp_composer and get_application_tester (ApplicationTester) infrastructure, not implemented"]
#[test]
fn test_run_guess_name_from_dir_sanitizes_dir() {
    set_up();

    todo!()
}

#[ignore = "needs TestCase::init_temp_composer and get_application_tester (ApplicationTester with set_inputs) infrastructure, not implemented"]
#[test]
fn test_interactive_run() {
    set_up();

    todo!()
}

#[ignore = "InitCommand::format_authors is pub(crate); integration tests cannot reach it (PHP subclasses via DummyInitCommand)"]
#[test]
fn test_format_authors() {
    set_up();

    todo!()
}

#[ignore = "InitCommand::get_git_config is pub(crate); integration tests cannot reach it (PHP subclasses via DummyInitCommand)"]
#[test]
fn test_get_git_config() {
    set_up();

    todo!()
}

#[ignore = "InitCommand::add_vendor_ignore is pub(crate) and test needs TestCase::get_unique_tmp_directory; neither reachable from integration tests"]
#[test]
fn test_add_vendor_ignore() {
    set_up();

    todo!()
}

#[ignore = "InitCommand::has_vendor_ignore/add_vendor_ignore are pub(crate) and test needs TestCase::get_unique_tmp_directory; neither reachable from integration tests"]
#[test]
fn test_has_vendor_ignore() {
    set_up();

    todo!()
}
