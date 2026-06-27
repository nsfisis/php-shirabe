//! ref: composer/tests/Composer/Test/Command/RepositoryCommandTest.php

use crate::test_case::{RunOptions, get_application_tester, init_temp_composer};
use serial_test::serial;
use shirabe::json::JsonFile;
use shirabe_php_shim::PhpMixed;

/// Read the composer.json in the CWD and decode it.
fn read_composer_json() -> serde_json::Value {
    let mut json = JsonFile::new("./composer.json".to_string(), None, None).unwrap();
    let read = json.read().unwrap();
    serde_json::from_str(&JsonFile::encode(&read)).unwrap()
}

#[test]
#[serial]
fn test_list_with_no_repositories() {
    let tear_down = init_temp_composer(Some(&serde_json::json!({})), None, None, true);

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("repo")),
                (PhpMixed::from("action"), PhpMixed::from("list")),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code);

    assert_eq!(
        "[packagist.org] composer https://repo.packagist.org",
        app_tester.get_display().trim()
    );
    // composer.json should remain unchanged
    assert_eq!(serde_json::json!([]), read_composer_json());

    drop(tear_down);
}

#[test]
#[serial]
fn test_list_with_repositories_as_list() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "repositories": [
                {"type": "composer", "url": "https://first.test"},
                {"name": "foo", "type": "vcs", "url": "https://old.example.org"},
                {"name": "bar", "type": "vcs", "url": "https://other.example.org"},
            ],
        })),
        None,
        None,
        true,
    );

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("repo")),
                (PhpMixed::from("action"), PhpMixed::from("list")),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code);

    assert_eq!(
        "[0] composer https://first.test
[foo] vcs https://old.example.org
[bar] vcs https://other.example.org
[packagist.org] disabled",
        app_tester.get_display().trim()
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_list_with_repositories_as_assoc() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "repositories": {
                "0": {"type": "composer", "url": "https://first.test"},
                "foo": {"type": "vcs", "url": "https://old.example.org"},
                "bar": {"type": "vcs", "url": "https://other.example.org"},
            },
        })),
        None,
        None,
        true,
    );

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("repo")),
                (PhpMixed::from("action"), PhpMixed::from("list")),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code);

    assert_eq!(
        "[0] composer https://first.test
[foo] vcs https://old.example.org
[bar] vcs https://other.example.org
[packagist.org] disabled",
        app_tester.get_display().trim()
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_add_repository_with_type_and_url() {
    let tear_down = init_temp_composer(Some(&serde_json::json!({})), None, None, true);

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("repo")),
                (PhpMixed::from("action"), PhpMixed::from("add")),
                (PhpMixed::from("name"), PhpMixed::from("foo")),
                (PhpMixed::from("arg1"), PhpMixed::from("vcs")),
                (
                    PhpMixed::from("arg2"),
                    PhpMixed::from("https://example.org/foo.git"),
                ),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code, "{}", app_tester.get_display());

    assert_eq!(
        serde_json::json!({
            "repositories": [
                {"name": "foo", "type": "vcs", "url": "https://example.org/foo.git"},
            ],
        }),
        read_composer_json()
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_add_repository_with_json() {
    let tear_down = init_temp_composer(Some(&serde_json::json!({})), None, None, true);

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("repo")),
                (PhpMixed::from("action"), PhpMixed::from("add")),
                (PhpMixed::from("name"), PhpMixed::from("bar")),
                (
                    PhpMixed::from("arg1"),
                    PhpMixed::from(r#"{"type":"composer","url":"https://repo.example.org"}"#),
                ),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code);

    assert_eq!(
        serde_json::json!({
            "repositories": [
                {"name": "bar", "type": "composer", "url": "https://repo.example.org"},
            ],
        }),
        read_composer_json()
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_remove_repository() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "repositories": {"foo": {"type": "vcs", "url": "https://example.org"}},
        })),
        None,
        None,
        false,
    );

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("repo")),
                (PhpMixed::from("action"), PhpMixed::from("remove")),
                (PhpMixed::from("name"), PhpMixed::from("foo")),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code);

    let json = read_composer_json();
    // repositories key may still exist as empty array depending on manipulator, accept either
    if let Some(repositories) = json.get("repositories") {
        assert_eq!(&serde_json::json!([]), repositories);
    } else {
        assert_eq!(serde_json::json!([]), json);
    }

    drop(tear_down);
}

/// ref: RepositoryCommandTest::testSetAndGetUrlInRepositoryAssoc (data provider).
fn run_set_and_get_url_assoc_case(name: &str, index: &str, new_url: &str) {
    let repositories = serde_json::json!({
        "first": {"type": "composer", "url": "https://first.test"},
        "foo": {"type": "vcs", "url": "https://old.example.org"},
        "bar": {"type": "vcs", "url": "https://other.example.org"},
    });
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({ "repositories": repositories })),
        None,
        None,
        false,
    );

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("repo")),
                (PhpMixed::from("action"), PhpMixed::from("set-url")),
                (PhpMixed::from("name"), PhpMixed::from(name)),
                (PhpMixed::from("arg1"), PhpMixed::from(new_url)),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code, "{}", app_tester.get_display());

    let json = read_composer_json();
    // calling it still in assoc means, the repository has not been converted, which is good
    assert_eq!(
        Some(&serde_json::Value::String(new_url.to_string())),
        json.get("repositories")
            .and_then(|r| r.get(index))
            .and_then(|r| r.get("url"))
    );

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("repo")),
                (PhpMixed::from("action"), PhpMixed::from("get-url")),
                (PhpMixed::from("name"), PhpMixed::from(name)),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code);
    assert_eq!(new_url, app_tester.get_display().trim());

    drop(tear_down);
}

#[test]
#[serial]
fn test_set_and_get_url_in_repository_assoc() {
    // change first of three
    run_set_and_get_url_assoc_case("first", "first", "https://new.example.org");
    // change middle of three
    run_set_and_get_url_assoc_case("foo", "foo", "https://new.example.org");
    // change last of three
    run_set_and_get_url_assoc_case("bar", "bar", "https://new.example.org");
}

/// ref: RepositoryCommandTest::testSetAndGetUrlInRepositoryList (data provider).
fn run_set_and_get_url_list_case(name: &str, index: usize, new_url: &str) {
    let repositories = serde_json::json!([
        {"name": "first", "type": "composer", "url": "https://first.test"},
        {"name": "foo", "type": "vcs", "url": "https://old.example.org"},
        {"name": "bar", "type": "vcs", "url": "https://other.example.org"},
    ]);
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({ "repositories": repositories })),
        None,
        None,
        false,
    );

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("repo")),
                (PhpMixed::from("action"), PhpMixed::from("set-url")),
                (PhpMixed::from("name"), PhpMixed::from(name)),
                (PhpMixed::from("arg1"), PhpMixed::from(new_url)),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code, "{}", app_tester.get_display());

    let json = read_composer_json();
    assert_eq!(
        Some(&serde_json::Value::String(name.to_string())),
        json.get("repositories")
            .and_then(|r| r.get(index))
            .and_then(|r| r.get("name"))
    );
    assert_eq!(
        Some(&serde_json::Value::String(new_url.to_string())),
        json.get("repositories")
            .and_then(|r| r.get(index))
            .and_then(|r| r.get("url"))
    );

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("repo")),
                (PhpMixed::from("action"), PhpMixed::from("get-url")),
                (PhpMixed::from("name"), PhpMixed::from(name)),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code);
    assert_eq!(new_url, app_tester.get_display().trim());

    drop(tear_down);
}

#[test]
#[serial]
fn test_set_and_get_url_in_repository_list() {
    // change first of three
    run_set_and_get_url_list_case("first", 0, "https://new.example.org");
    // change middle of three
    run_set_and_get_url_list_case("foo", 1, "https://new.example.org");
    // change last of three
    run_set_and_get_url_list_case("bar", 2, "https://new.example.org");
}

#[test]
#[serial]
fn test_disable_and_enable_packagist() {
    let tear_down = init_temp_composer(Some(&serde_json::json!({})), None, None, true);

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("repo")),
                (PhpMixed::from("action"), PhpMixed::from("disable")),
                (PhpMixed::from("name"), PhpMixed::from("packagist")),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code);
    assert_eq!(
        serde_json::json!({"repositories": [{"packagist.org": false}]}),
        read_composer_json()
    );

    // enable packagist should remove the override
    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("repo")),
                (PhpMixed::from("action"), PhpMixed::from("enable")),
                (PhpMixed::from("name"), PhpMixed::from("packagist")),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code);
    assert_eq!(serde_json::json!([]), read_composer_json());

    drop(tear_down);
}

#[test]
#[serial]
fn test_invalid_arg_combination_throws() {
    let mut app_tester = get_application_tester();
    let err = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("repo")),
                (
                    PhpMixed::from("--file"),
                    PhpMixed::from("alt.composer.json"),
                ),
                (PhpMixed::from("--global"), PhpMixed::from(true)),
            ],
            RunOptions::default(),
        )
        .expect_err("expected RuntimeException for --file and --global combination");
    assert!(
        err.to_string()
            .contains("--file and --global can not be combined"),
        "got: {}",
        err
    );
}

#[test]
#[serial]
fn test_prepend_repository_by_name_list_to_assoc() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "repositories": [{"type": "git", "url": "example.tld"}],
        })),
        None,
        None,
        false,
    );

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("repo")),
                (PhpMixed::from("action"), PhpMixed::from("add")),
                (PhpMixed::from("name"), PhpMixed::from("foo")),
                (PhpMixed::from("arg1"), PhpMixed::from("path")),
                (PhpMixed::from("arg2"), PhpMixed::from("foo/bar")),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code, "{}", app_tester.get_display());

    assert_eq!(
        serde_json::json!({
            "repositories": [
                {"name": "foo", "type": "path", "url": "foo/bar"},
                {"type": "git", "url": "example.tld"},
            ],
        }),
        read_composer_json()
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_append_repository_by_name_list_to_assoc() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "repositories": [{"type": "git", "url": "example.tld"}],
        })),
        None,
        None,
        false,
    );

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("repo")),
                (PhpMixed::from("action"), PhpMixed::from("add")),
                (PhpMixed::from("name"), PhpMixed::from("foo")),
                (PhpMixed::from("arg1"), PhpMixed::from("path")),
                (PhpMixed::from("arg2"), PhpMixed::from("foo/bar")),
                (PhpMixed::from("--append"), PhpMixed::from(true)),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code, "{}", app_tester.get_display());

    assert_eq!(
        serde_json::json!({
            "repositories": [
                {"type": "git", "url": "example.tld"},
                {"name": "foo", "type": "path", "url": "foo/bar"},
            ],
        }),
        read_composer_json()
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_prepend_repository_assoc_with_packagist_disabled() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "repositories": [{"type": "git", "url": "example.tld"}, {"packagist.org": false}],
        })),
        None,
        None,
        true,
    );

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("repo")),
                (PhpMixed::from("action"), PhpMixed::from("add")),
                (PhpMixed::from("name"), PhpMixed::from("foo")),
                (PhpMixed::from("arg1"), PhpMixed::from("path")),
                (PhpMixed::from("arg2"), PhpMixed::from("foo/bar")),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code, "{}", app_tester.get_display());

    assert_eq!(
        serde_json::json!({
            "repositories": [
                {"name": "foo", "type": "path", "url": "foo/bar"},
                {"type": "git", "url": "example.tld"},
                {"packagist.org": false},
            ],
        }),
        read_composer_json()
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_append_repository_assoc_with_packagist_disabled() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "repositories": [{"type": "git", "url": "example.tld"}, {"packagist.org": false}],
        })),
        None,
        None,
        true,
    );

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("repo")),
                (PhpMixed::from("action"), PhpMixed::from("add")),
                (PhpMixed::from("name"), PhpMixed::from("foo")),
                (PhpMixed::from("arg1"), PhpMixed::from("path")),
                (PhpMixed::from("arg2"), PhpMixed::from("foo/bar")),
                (PhpMixed::from("--append"), PhpMixed::from(true)),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code, "{}", app_tester.get_display());

    assert_eq!(
        serde_json::json!({
            "repositories": [
                {"type": "git", "url": "example.tld"},
                {"packagist.org": false},
                {"name": "foo", "type": "path", "url": "foo/bar"},
            ],
        }),
        read_composer_json()
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_add_before_and_after_by_name() {
    // Start with two repos as named-list and a disabled packagist boolean
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "repositories": [
                {"name": "alpha", "type": "vcs", "url": "https://example.org/a"},
                {"name": "omega", "type": "vcs", "url": "https://example.org/o"},
                {"packagist.org": false},
            ],
        })),
        None,
        None,
        true,
    );

    // Insert before omega
    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("repo")),
                (PhpMixed::from("action"), PhpMixed::from("add")),
                (PhpMixed::from("name"), PhpMixed::from("beta")),
                (PhpMixed::from("arg1"), PhpMixed::from("vcs")),
                (
                    PhpMixed::from("arg2"),
                    PhpMixed::from("https://example.org/b"),
                ),
                (PhpMixed::from("--before"), PhpMixed::from("omega")),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code, "{}", app_tester.get_display());

    // Insert after alpha
    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("repo")),
                (PhpMixed::from("action"), PhpMixed::from("add")),
                (PhpMixed::from("name"), PhpMixed::from("gamma")),
                (PhpMixed::from("arg1"), PhpMixed::from("vcs")),
                (
                    PhpMixed::from("arg2"),
                    PhpMixed::from("https://example.org/g"),
                ),
                (PhpMixed::from("--after"), PhpMixed::from("alpha")),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code, "{}", app_tester.get_display());

    // Expect order: alpha, gamma, beta, omega, then packagist.org boolean preserved
    assert_eq!(
        serde_json::json!({
            "repositories": [
                {"name": "alpha", "type": "vcs", "url": "https://example.org/a"},
                {"name": "gamma", "type": "vcs", "url": "https://example.org/g"},
                {"name": "beta", "type": "vcs", "url": "https://example.org/b"},
                {"name": "omega", "type": "vcs", "url": "https://example.org/o"},
                {"packagist.org": false},
            ],
        }),
        read_composer_json()
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_add_same_name_replaces_existing() {
    let tear_down = init_temp_composer(Some(&serde_json::json!({})), None, None, true);

    // first add
    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("repo")),
                (PhpMixed::from("action"), PhpMixed::from("add")),
                (PhpMixed::from("name"), PhpMixed::from("foo")),
                (PhpMixed::from("arg1"), PhpMixed::from("vcs")),
                (
                    PhpMixed::from("arg2"),
                    PhpMixed::from("https://example.org/old"),
                ),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code, "{}", app_tester.get_display());

    // second add with same name but different url
    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("repo")),
                (PhpMixed::from("action"), PhpMixed::from("add")),
                (PhpMixed::from("name"), PhpMixed::from("foo")),
                (PhpMixed::from("arg1"), PhpMixed::from("vcs")),
                (
                    PhpMixed::from("arg2"),
                    PhpMixed::from("https://example.org/new"),
                ),
                (PhpMixed::from("--append"), PhpMixed::from(true)),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code, "{}", app_tester.get_display());

    let json = read_composer_json();

    // repositories can be stored as assoc or named-list depending on manipulator fallbacks
    // Validate there is only one "foo" and its url is the latest
    let mut count_foo = 0;
    let mut url: Option<&serde_json::Value> = None;
    match json.get("repositories") {
        Some(serde_json::Value::Object(map)) => {
            for (k, repo) in map {
                if k == "foo" && repo.is_object() {
                    count_foo += 1;
                    url = repo.get("url");
                } else if repo.is_object()
                    && repo.get("name") == Some(&serde_json::Value::String("foo".to_string()))
                {
                    count_foo += 1;
                    url = repo.get("url");
                }
            }
        }
        Some(serde_json::Value::Array(list)) => {
            for repo in list {
                if repo.is_object()
                    && repo.get("name") == Some(&serde_json::Value::String("foo".to_string()))
                {
                    count_foo += 1;
                    url = repo.get("url");
                }
            }
        }
        _ => {}
    }
    assert_eq!(
        1, count_foo,
        "Exactly one repository entry with name foo should exist"
    );
    assert_eq!(
        Some(&serde_json::Value::String(
            "https://example.org/new".to_string()
        )),
        url,
        "The foo repository should have been updated to the new URL"
    );

    drop(tear_down);
}
