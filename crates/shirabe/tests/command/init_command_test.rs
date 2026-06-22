//! ref: composer/tests/Composer/Test/Command/InitCommandTest.php

// The run cases (testRunCommand, testRunCommandInvalid, testRunGuessNameFromDirSanitizesDir,
// testInteractiveRun) drive the full command via ApplicationTester / initTempComposer, which
// does not exist here; they remain reason'd-ignore. The unit-style cases call the helper
// methods directly via `__`-prefixed test-only wrappers.

use shirabe::command::init_command::InitCommand;
use shirabe_php_shim::{PhpMixed, server_set};
use tempfile::TempDir;

fn set_up() {
    server_set("COMPOSER_DEFAULT_AUTHOR", "John Smith".to_string());
    server_set("COMPOSER_DEFAULT_EMAIL", "john@example.com".to_string());
}

/// @return iterable<string, array{0: string, 1: string|null, 2: string}>
fn valid_author_string_provider() -> Vec<(&'static str, Option<&'static str>, &'static str)> {
    vec![
        // simple
        (
            "John Smith",
            Some("john@example.com"),
            "John Smith <john@example.com>",
        ),
        // without email
        ("John Smith", None, "John Smith"),
        // UTF-8
        (
            "Matti Meikäläinen",
            Some("matti@example.com"),
            "Matti Meikäläinen <matti@example.com>",
        ),
        // UTF-8 with non-spacing marks (\xCC\x88 is U+0308 combining diaeresis)
        (
            "Matti Meika\u{0308}la\u{0308}inen",
            Some("matti@example.com"),
            "Matti Meika\u{0308}la\u{0308}inen <matti@example.com>",
        ),
        // numeric author name
        ("h4x0r", Some("h4x@example.com"), "h4x0r <h4x@example.com>"),
        // alias 1 (issue #5631)
        (
            "Johnathon \"Johnny\" Smith",
            Some("john@example.com"),
            "Johnathon \"Johnny\" Smith <john@example.com>",
        ),
        // alias 2 (issue #5631)
        (
            "Johnathon (Johnny) Smith",
            Some("john@example.com"),
            "Johnathon (Johnny) Smith <john@example.com>",
        ),
    ]
}

#[ignore]
#[test]
fn test_parse_valid_author_string() {
    set_up();

    for (name, email, input) in valid_author_string_provider() {
        let command = InitCommand::new();
        let author = command.__parse_author_string(input).unwrap();
        assert_eq!(
            Some(name.to_string()),
            author.get("name").cloned().flatten()
        );
        assert_eq!(
            email.map(|e| e.to_string()),
            author.get("email").cloned().flatten()
        );
    }
}

#[ignore]
#[test]
fn test_parse_empty_author_string() {
    set_up();

    let command = InitCommand::new();
    let result = command.__parse_author_string("");
    assert!(result.is_err());
}

#[ignore]
#[test]
fn test_parse_author_string_with_invalid_email() {
    set_up();

    let command = InitCommand::new();
    let result = command.__parse_author_string("John Smith <john>");
    assert!(result.is_err());
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

#[ignore]
#[test]
fn test_format_authors() {
    set_up();

    let author_with_email = "John Smith <john@example.com>";
    let author_without_email = "John Smith";
    let command = InitCommand::new();

    let authors = command.__format_authors(author_with_email).unwrap();
    let mut expected: indexmap::IndexMap<String, PhpMixed> = indexmap::IndexMap::new();
    expected.insert(
        "name".to_string(),
        PhpMixed::String("John Smith".to_string()),
    );
    expected.insert(
        "email".to_string(),
        PhpMixed::String("john@example.com".to_string()),
    );
    assert_eq!(expected, authors[0]);

    let authors = command.__format_authors(author_without_email).unwrap();
    let mut expected: indexmap::IndexMap<String, PhpMixed> = indexmap::IndexMap::new();
    expected.insert(
        "name".to_string(),
        PhpMixed::String("John Smith".to_string()),
    );
    assert_eq!(expected, authors[0]);
}

#[ignore]
#[test]
fn test_get_git_config() {
    set_up();

    let mut command = InitCommand::new();
    let git_config = command.__get_git_config();
    assert!(git_config.contains_key("user.name"));
    assert!(git_config.contains_key("user.email"));
}

#[ignore]
#[test]
fn test_add_vendor_ignore() {
    set_up();

    let tmp = TempDir::new().unwrap();
    let ignore_file = tmp.path().join("ignore");
    let ignore_file = ignore_file.to_str().unwrap();

    let command = InitCommand::new();
    command.__add_vendor_ignore(ignore_file, "/vendor/");
    assert!(std::path::Path::new(ignore_file).exists());
    let content = std::fs::read_to_string(ignore_file).unwrap();
    assert!(content.contains("/vendor/"));
}

#[ignore]
#[test]
fn test_has_vendor_ignore() {
    set_up();

    let tmp = TempDir::new().unwrap();
    let ignore_file = tmp.path().join("ignore");
    let ignore_file = ignore_file.to_str().unwrap();

    let command = InitCommand::new();
    assert!(!command.__has_vendor_ignore(ignore_file, "vendor"));
    command.__add_vendor_ignore(ignore_file, "/vendor/");
    assert!(command.__has_vendor_ignore(ignore_file, "vendor"));
}
