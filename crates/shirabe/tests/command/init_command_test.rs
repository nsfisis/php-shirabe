//! ref: composer/tests/Composer/Test/Command/InitCommandTest.php

use crate::test_case::{RunOptions, get_application_tester, init_temp_composer};
use serial_test::serial;
use shirabe::command::init_command::InitCommand;
use shirabe::json::JsonFile;
use shirabe_php_shim::{PHP_SERVER, PhpMixed};
use tempfile::TempDir;

fn set_up() {
    let mut server = PHP_SERVER.lock().unwrap();
    server.put("COMPOSER_DEFAULT_AUTHOR".into(), "John Smith".into());
    server.put("COMPOSER_DEFAULT_EMAIL".into(), "john@example.com".into());
}

/// const DEFAULT_AUTHORS in PHP.
fn default_authors() -> serde_json::Value {
    serde_json::json!({ "name": "John Smith", "email": "john@example.com" })
}

/// Reads CWD's `composer.json` like PHP's `(new JsonFile(...))->read()`, projected onto a
/// `serde_json::Value` so the comparison ignores object key order (matching PHPUnit's `assertEquals`
/// on arrays) while staying order-sensitive for lists.
fn read_composer_json(dir: &std::path::Path) -> serde_json::Value {
    let mut file = JsonFile::new(
        dir.join("composer.json").to_string_lossy().to_string(),
        None,
        None,
    )
    .unwrap();
    serde_json::to_value(file.read().unwrap()).unwrap()
}

/// `['command' => 'init', '--no-interaction' => true] + $arguments`.
fn non_interactive_input(arguments: Vec<(PhpMixed, PhpMixed)>) -> Vec<(PhpMixed, PhpMixed)> {
    let mut input = vec![
        (PhpMixed::from("command"), PhpMixed::from("init")),
        (PhpMixed::from("--no-interaction"), PhpMixed::Bool(true)),
    ];
    input.extend(arguments);
    input
}

fn opt(name: &str, value: &str) -> (PhpMixed, PhpMixed) {
    (PhpMixed::from(name), PhpMixed::from(value))
}

fn opt_list(name: &str, values: &[&str]) -> (PhpMixed, PhpMixed) {
    (
        PhpMixed::from(name),
        PhpMixed::List(values.iter().map(|v| PhpMixed::from(*v)).collect()),
    )
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

#[test]
fn test_parse_empty_author_string() {
    set_up();

    let command = InitCommand::new();
    let result = command.__parse_author_string("");
    assert!(result.is_err());
}

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

/// @return iterable<string, array{0: array<string, mixed>, 1: array<string, mixed>}>
fn run_data_provider() -> Vec<(serde_json::Value, Vec<(PhpMixed, PhpMixed)>)> {
    vec![
        // name argument
        (
            serde_json::json!({
                "name": "test/pkg",
                "authors": [default_authors()],
                "require": [],
            }),
            vec![opt("--name", "test/pkg")],
        ),
        // name and author arguments
        (
            serde_json::json!({
                "name": "test/pkg",
                "require": [],
                "authors": [{ "name": "Mr. Test", "email": "test@example.org" }],
            }),
            vec![
                opt("--name", "test/pkg"),
                opt("--author", "Mr. Test <test@example.org>"),
            ],
        ),
        // name and author arguments without email
        (
            serde_json::json!({
                "name": "test/pkg",
                "require": [],
                "authors": [{ "name": "Mr. Test" }],
            }),
            vec![opt("--name", "test/pkg"), opt("--author", "Mr. Test")],
        ),
        // single repository argument
        (
            serde_json::json!({
                "name": "test/pkg",
                "authors": [default_authors()],
                "require": [],
                "repositories": [{ "type": "vcs", "url": "http://packages.example.com" }],
            }),
            vec![
                opt("--name", "test/pkg"),
                opt_list(
                    "--repository",
                    &["{\"type\":\"vcs\",\"url\":\"http://packages.example.com\"}"],
                ),
            ],
        ),
        // multiple repository arguments
        (
            serde_json::json!({
                "name": "test/pkg",
                "authors": [default_authors()],
                "require": [],
                "repositories": [
                    { "type": "vcs", "url": "http://vcs.example.com" },
                    { "type": "composer", "url": "http://composer.example.com" },
                    {
                        "type": "composer",
                        "url": "http://composer2.example.com",
                        "options": { "ssl": { "verify_peer": "true" } },
                    },
                ],
            }),
            vec![
                opt("--name", "test/pkg"),
                opt_list(
                    "--repository",
                    &[
                        "{\"type\":\"vcs\",\"url\":\"http://vcs.example.com\"}",
                        "{\"type\":\"composer\",\"url\":\"http://composer.example.com\"}",
                        "{\"type\":\"composer\",\"url\":\"http://composer2.example.com\",\"options\":{\"ssl\":{\"verify_peer\":\"true\"}}}",
                    ],
                ),
            ],
        ),
        // stability argument
        (
            serde_json::json!({
                "name": "test/pkg",
                "authors": [default_authors()],
                "require": [],
                "minimum-stability": "dev",
            }),
            vec![opt("--name", "test/pkg"), opt("--stability", "dev")],
        ),
        // require one argument
        (
            serde_json::json!({
                "name": "test/pkg",
                "authors": [default_authors()],
                "require": { "first/pkg": "1.0.0" },
            }),
            vec![
                opt("--name", "test/pkg"),
                opt_list("--require", &["first/pkg:1.0.0"]),
            ],
        ),
        // require multiple arguments
        (
            serde_json::json!({
                "name": "test/pkg",
                "authors": [default_authors()],
                "require": { "first/pkg": "1.0.0", "second/pkg": "^3.4" },
            }),
            vec![
                opt("--name", "test/pkg"),
                opt_list("--require", &["first/pkg:1.0.0", "second/pkg:^3.4"]),
            ],
        ),
        // require-dev one argument
        (
            serde_json::json!({
                "name": "test/pkg",
                "authors": [default_authors()],
                "require": [],
                "require-dev": { "first/pkg": "1.0.0" },
            }),
            vec![
                opt("--name", "test/pkg"),
                opt_list("--require-dev", &["first/pkg:1.0.0"]),
            ],
        ),
        // require-dev multiple arguments
        (
            serde_json::json!({
                "name": "test/pkg",
                "authors": [default_authors()],
                "require": [],
                "require-dev": { "first/pkg": "1.0.0", "second/pkg": "^3.4" },
            }),
            vec![
                opt("--name", "test/pkg"),
                opt_list("--require-dev", &["first/pkg:1.0.0", "second/pkg:^3.4"]),
            ],
        ),
        // autoload argument
        (
            serde_json::json!({
                "name": "test/pkg",
                "authors": [default_authors()],
                "require": [],
                "autoload": { "psr-4": { "Test\\Pkg\\": "testMapping/" } },
            }),
            vec![opt("--name", "test/pkg"), opt("--autoload", "testMapping/")],
        ),
        // homepage argument
        (
            serde_json::json!({
                "name": "test/pkg",
                "authors": [default_authors()],
                "require": [],
                "homepage": "https://example.org/",
            }),
            vec![
                opt("--name", "test/pkg"),
                opt("--homepage", "https://example.org/"),
            ],
        ),
        // description argument
        (
            serde_json::json!({
                "name": "test/pkg",
                "authors": [default_authors()],
                "require": [],
                "description": "My first example package",
            }),
            vec![
                opt("--name", "test/pkg"),
                opt("--description", "My first example package"),
            ],
        ),
        // type argument
        (
            serde_json::json!({
                "name": "test/pkg",
                "authors": [default_authors()],
                "require": [],
                "type": "project",
            }),
            vec![opt("--name", "test/pkg"), opt("--type", "project")],
        ),
        // license argument
        (
            serde_json::json!({
                "name": "test/pkg",
                "authors": [default_authors()],
                "require": [],
                "license": "MIT",
            }),
            vec![opt("--name", "test/pkg"), opt("--license", "MIT")],
        ),
    ]
}

#[test]
#[serial]
fn test_run_command() {
    set_up();

    for (expected, arguments) in run_data_provider() {
        let tear_down = init_temp_composer(None, None, None, true);
        let dir = tear_down.working_dir();
        std::fs::remove_file(dir.join("composer.json")).unwrap();
        std::fs::remove_file(dir.join("auth.json")).unwrap();

        let mut app_tester = get_application_tester();
        app_tester
            .run(non_interactive_input(arguments), RunOptions::default())
            .unwrap();

        assert_eq!(0, app_tester.get_status_code());

        assert_eq!(expected, read_composer_json(&dir));
    }
}

/// Either the run throws (optionally carrying a message), or it returns exit code 1 and writes a
/// message matching the regex to stderr.
enum InvalidExpectation {
    Throws(Option<&'static str>),
    StderrMatches(&'static str),
}

/// @return iterable<string, array{0: class-string<\Throwable>|null, 1: string|null, 2: array<string, mixed>}>
fn run_invalid_data_provider() -> Vec<(InvalidExpectation, Vec<(PhpMixed, PhpMixed)>)> {
    vec![
        // invalid name argument
        (
            InvalidExpectation::Throws(None),
            vec![opt("--name", "test")],
        ),
        // invalid author argument
        (
            InvalidExpectation::Throws(None),
            vec![
                opt("--name", "test/pkg"),
                opt("--author", "Mr. Test <test>"),
            ],
        ),
        // invalid stability argument
        (
            InvalidExpectation::StderrMatches(
                r"minimum-stability\s+:\s+Does not have a value in the enumeration",
            ),
            vec![opt("--name", "test/pkg"), opt("--stability", "bogus")],
        ),
        // invalid require argument
        (
            InvalidExpectation::Throws(Some(
                "Option first is missing a version constraint, use e.g. first:^1.0",
            )),
            vec![opt("--name", "test/pkg"), opt_list("--require", &["first"])],
        ),
        // invalid require-dev argument
        (
            InvalidExpectation::Throws(Some(
                "Option first is missing a version constraint, use e.g. first:^1.0",
            )),
            vec![
                opt("--name", "test/pkg"),
                opt_list("--require-dev", &["first"]),
            ],
        ),
        // invalid homepage argument
        (
            InvalidExpectation::StderrMatches(r"homepage\s*:\s*Invalid URL format"),
            vec![opt("--name", "test/pkg"), opt("--homepage", "not-a-url")],
        ),
    ]
}

#[test]
#[serial]
#[ignore = "drives InitCommand, which calls get_git_config -> ProcessExecutor::run_process; \
            that path is not ported (shim is_callable and Process::start/run are todo!())"]
fn test_run_command_invalid() {
    set_up();

    for (expectation, arguments) in run_invalid_data_provider() {
        let tear_down = init_temp_composer(None, None, None, true);
        let dir = tear_down.working_dir();
        std::fs::remove_file(dir.join("composer.json")).unwrap();
        std::fs::remove_file(dir.join("auth.json")).unwrap();

        let mut app_tester = get_application_tester();
        let options = RunOptions {
            capture_stderr_separately: true,
            ..Default::default()
        };
        let result = app_tester.run(non_interactive_input(arguments), options);

        match expectation {
            InvalidExpectation::Throws(message) => {
                let error = result.expect_err("expected the command to surface an exception");
                if let Some(message) = message {
                    let rendered = format!("{:#}", error);
                    assert!(
                        rendered.contains(message),
                        "error {:?} did not contain {:?}",
                        rendered,
                        message
                    );
                }
            }
            InvalidExpectation::StderrMatches(pattern) => {
                result.unwrap();
                assert_eq!(1, app_tester.get_status_code());
                let regex = regex::Regex::new(pattern).unwrap();
                let stderr = app_tester.get_error_output();
                assert!(
                    regex.is_match(&stderr),
                    "stderr {:?} did not match {:?}",
                    stderr,
                    pattern
                );
            }
        }
    }
}

#[test]
#[serial]
fn test_run_guess_name_from_dir_sanitizes_dir() {
    set_up();

    let tear_down = init_temp_composer(None, None, None, true);

    let dir_name = "_foo_--bar__baz.--..qux__";
    std::fs::create_dir(dir_name).unwrap();
    std::env::set_current_dir(dir_name).unwrap();

    PHP_SERVER
        .lock()
        .unwrap()
        .put("COMPOSER_DEFAULT_VENDOR".into(), ".vendorName".into());

    let mut app_tester = get_application_tester();
    let result = app_tester.run(non_interactive_input(vec![]), RunOptions::default());

    PHP_SERVER.lock().unwrap().clear("COMPOSER_DEFAULT_VENDOR");
    result.unwrap();

    assert_eq!(0, app_tester.get_status_code());

    let expected = serde_json::json!({
        "name": "vendor-name/foo-bar_baz.qux",
        "authors": [default_authors()],
        "require": [],
    });
    assert_eq!(
        expected,
        read_composer_json(&std::env::current_dir().unwrap())
    );

    drop(tear_down);
}

#[test]
#[serial]
#[ignore = "drives InitCommand, which calls get_git_config -> ProcessExecutor::run_process; \
            that path is not ported (shim is_callable and Process::start/run are todo!())"]
fn test_interactive_run() {
    set_up();

    let tear_down = init_temp_composer(None, None, None, true);
    let dir = tear_down.working_dir();
    std::fs::remove_file(dir.join("composer.json")).unwrap();
    std::fs::remove_file(dir.join("auth.json")).unwrap();

    let mut app_tester = get_application_tester();
    app_tester.set_inputs(vec![
        "vendor/pkg".to_string(),                  // Pkg name
        "my description".to_string(),              // Description
        "Mr. Test <test@example.org>".to_string(), // Author
        "stable".to_string(),                      // Minimum stability
        "library".to_string(),                     // Type
        "AGPL-3.0-only".to_string(),               // License
        "no".to_string(),                          // Define dependencies
        "no".to_string(),                          // Define dev dependencies
        "n".to_string(),                           // Add PSR-4 autoload mapping
        "".to_string(),                            // Confirm generation
    ]);

    app_tester
        .run(
            vec![(PhpMixed::from("command"), PhpMixed::from("init"))],
            RunOptions::default(),
        )
        .unwrap();

    assert_eq!(0, app_tester.get_status_code());

    let expected = serde_json::json!({
        "name": "vendor/pkg",
        "description": "my description",
        "type": "library",
        "license": "AGPL-3.0-only",
        "authors": [{ "name": "Mr. Test", "email": "test@example.org" }],
        "minimum-stability": "stable",
        "require": [],
    });
    assert_eq!(expected, read_composer_json(&dir));
}

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

#[test]
#[ignore = "calls get_git_config -> ProcessExecutor::run_process, which is not ported \
            (shim is_callable and Process::start/run are todo!())"]
fn test_get_git_config() {
    set_up();

    let command = InitCommand::new();
    let git_config = command.__get_git_config();
    assert!(git_config.contains_key("user.name"));
    assert!(git_config.contains_key("user.email"));
}

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
