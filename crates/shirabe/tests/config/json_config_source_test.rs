//! ref: composer/tests/Composer/Test/Config/JsonConfigSourceTest.php

use indexmap::IndexMap;
use shirabe::config::ConfigSourceInterface;
use shirabe::config::JsonConfigSource;
use shirabe::json::JsonFile;
use shirabe::util::filesystem::Filesystem;
use shirabe_php_shim::PhpMixed;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use tempfile::TempDir;

fn set_up() -> TearDown {
    let fs = Filesystem::new(None);
    // getUniqueTmpDirectory creates a fresh unique temp directory.
    let working_dir = TempDir::new().unwrap();
    TearDown { fs, working_dir }
}

struct TearDown {
    fs: Filesystem,
    working_dir: TempDir,
}

impl TearDown {
    fn working_dir(&self) -> PathBuf {
        self.working_dir.path().to_path_buf()
    }
}

impl Drop for TearDown {
    fn drop(&mut self) {
        let working_dir = self.working_dir.path();
        if working_dir.is_dir() {
            self.fs.remove_directory(working_dir).unwrap();
        }
    }
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(format!(
        "{}/../../composer/tests/Composer/Test/Config/Fixtures/{}",
        env!("CARGO_MANIFEST_DIR"),
        name
    ))
}

fn assert_file_equals(expected: &std::path::Path, actual: &std::path::Path) {
    let expected_contents = std::fs::read(expected).unwrap();
    let actual_contents = std::fs::read(actual).unwrap();
    assert_eq!(
        expected_contents, actual_contents,
        "Failed asserting that file {:?} matches {:?}",
        actual, expected
    );
}

fn json_config_source(config: &std::path::Path) -> JsonConfigSource {
    let json_file = JsonFile::new(config.to_string_lossy().to_string(), None, None).unwrap();
    JsonConfigSource::new(Rc::new(RefCell::new(json_file)), false)
}

#[ignore]
#[test]
fn test_add_repository() {
    let tear_down = set_up();
    let config = tear_down.working_dir().join("composer.json");
    std::fs::copy(fixture_path("composer-repositories.json"), &config).unwrap();
    let mut json_config_source = json_config_source(&config);

    let mut repo = IndexMap::new();
    repo.insert("type".to_string(), PhpMixed::String("git".to_string()));
    repo.insert(
        "url".to_string(),
        PhpMixed::String("example.tld".to_string()),
    );
    json_config_source
        .add_repository("example_tld", PhpMixed::Array(repo), true)
        .unwrap();

    assert_file_equals(
        &fixture_path("config/config-with-exampletld-repository.json"),
        &config,
    );
}

#[ignore]
#[test]
fn test_add_repository_as_list() {
    let tear_down = set_up();
    let config = tear_down.working_dir().join("composer.json");
    std::fs::copy(fixture_path("composer-repositories.json"), &config).unwrap();
    let mut json_config_source = json_config_source(&config);

    let mut repo = IndexMap::new();
    repo.insert("type".to_string(), PhpMixed::String("git".to_string()));
    repo.insert(
        "url".to_string(),
        PhpMixed::String("example.tld".to_string()),
    );
    json_config_source
        .add_repository("", PhpMixed::Array(repo), true)
        .unwrap();

    assert_file_equals(
        &fixture_path("config/config-with-exampletld-repository-as-list.json"),
        &config,
    );
}

#[ignore]
#[test]
fn test_add_repository_with_options() {
    let tear_down = set_up();
    let config = tear_down.working_dir().join("composer.json");
    std::fs::copy(fixture_path("composer-repositories.json"), &config).unwrap();
    let mut json_config_source = json_config_source(&config);

    let mut repo = IndexMap::new();
    repo.insert("type".to_string(), PhpMixed::String("composer".to_string()));
    repo.insert(
        "url".to_string(),
        PhpMixed::String("https://example.tld".to_string()),
    );
    {
        let mut local_cert = IndexMap::new();
        local_cert.insert(
            "local_cert".to_string(),
            PhpMixed::String("/home/composer/.ssl/composer.pem".to_string()),
        );
        let mut ssl = IndexMap::new();
        ssl.insert("ssl".to_string(), PhpMixed::Array(local_cert));
        repo.insert("options".to_string(), PhpMixed::Array(ssl));
    }

    json_config_source
        .add_repository("example_tld", PhpMixed::Array(repo), true)
        .unwrap();

    assert_file_equals(
        &fixture_path("config/config-with-exampletld-repository-and-options.json"),
        &config,
    );
}

#[ignore]
#[test]
fn test_remove_repository() {
    let tear_down = set_up();
    let config = tear_down.working_dir().join("composer.json");
    std::fs::copy(
        fixture_path("config/config-with-exampletld-repository.json"),
        &config,
    )
    .unwrap();
    let mut json_config_source = json_config_source(&config);
    json_config_source.remove_repository("example_tld").unwrap();

    assert_file_equals(&fixture_path("composer-empty.json"), &config);
}

#[ignore]
#[test]
fn test_add_packagist_repository_with_false_value() {
    let tear_down = set_up();
    let config = tear_down.working_dir().join("composer.json");
    std::fs::copy(fixture_path("composer-repositories.json"), &config).unwrap();
    let mut json_config_source = json_config_source(&config);
    json_config_source
        .add_repository("packagist", PhpMixed::Bool(false), true)
        .unwrap();

    assert_file_equals(
        &fixture_path("config/config-with-packagist-false.json"),
        &config,
    );
}

#[ignore]
#[test]
fn test_remove_packagist() {
    let tear_down = set_up();
    let config = tear_down.working_dir().join("composer.json");
    std::fs::copy(
        fixture_path("config/config-with-packagist-false.json"),
        &config,
    )
    .unwrap();
    let mut json_config_source = json_config_source(&config);
    json_config_source.remove_repository("packagist").unwrap();

    assert_file_equals(&fixture_path("composer-empty.json"), &config);
}

/// Mirror of provideAddLinkData(): (sourceFile, type, name, value, compareAgainst).
fn provide_add_link_data() -> Vec<(PathBuf, &'static str, &'static str, &'static str, PathBuf)> {
    let empty = fixture_path("composer-empty.json");
    let one_of_everything = fixture_path("composer-one-of-everything.json");
    let two_of_everything = fixture_path("composer-two-of-everything.json");

    let add_link_data_arguments = |r#type: &'static str,
                                   name: &'static str,
                                   value: &'static str,
                                   fixture_basename: &str,
                                   before: &PathBuf| {
        (
            before.clone(),
            r#type,
            name,
            value,
            fixture_path(&format!("addLink/{}.json", fixture_basename)),
        )
    };

    vec![
        add_link_data_arguments(
            "require",
            "my-vend/my-lib",
            "1.*",
            "require-from-empty",
            &empty,
        ),
        add_link_data_arguments(
            "require",
            "my-vend/my-lib",
            "1.*",
            "require-from-oneOfEverything",
            &one_of_everything,
        ),
        add_link_data_arguments(
            "require",
            "my-vend/my-lib",
            "1.*",
            "require-from-twoOfEverything",
            &two_of_everything,
        ),
        add_link_data_arguments(
            "require-dev",
            "my-vend/my-lib-tests",
            "1.*",
            "require-dev-from-empty",
            &empty,
        ),
        add_link_data_arguments(
            "require-dev",
            "my-vend/my-lib-tests",
            "1.*",
            "require-dev-from-oneOfEverything",
            &one_of_everything,
        ),
        add_link_data_arguments(
            "require-dev",
            "my-vend/my-lib-tests",
            "1.*",
            "require-dev-from-twoOfEverything",
            &two_of_everything,
        ),
        add_link_data_arguments(
            "provide",
            "my-vend/my-lib-interface",
            "1.*",
            "provide-from-empty",
            &empty,
        ),
        add_link_data_arguments(
            "provide",
            "my-vend/my-lib-interface",
            "1.*",
            "provide-from-oneOfEverything",
            &one_of_everything,
        ),
        add_link_data_arguments(
            "provide",
            "my-vend/my-lib-interface",
            "1.*",
            "provide-from-twoOfEverything",
            &two_of_everything,
        ),
        add_link_data_arguments(
            "suggest",
            "my-vend/my-optional-extension",
            "1.*",
            "suggest-from-empty",
            &empty,
        ),
        add_link_data_arguments(
            "suggest",
            "my-vend/my-optional-extension",
            "1.*",
            "suggest-from-oneOfEverything",
            &one_of_everything,
        ),
        add_link_data_arguments(
            "suggest",
            "my-vend/my-optional-extension",
            "1.*",
            "suggest-from-twoOfEverything",
            &two_of_everything,
        ),
        add_link_data_arguments(
            "replace",
            "my-vend/other-app",
            "1.*",
            "replace-from-empty",
            &empty,
        ),
        add_link_data_arguments(
            "replace",
            "my-vend/other-app",
            "1.*",
            "replace-from-oneOfEverything",
            &one_of_everything,
        ),
        add_link_data_arguments(
            "replace",
            "my-vend/other-app",
            "1.*",
            "replace-from-twoOfEverything",
            &two_of_everything,
        ),
        add_link_data_arguments(
            "conflict",
            "my-vend/my-old-app",
            "1.*",
            "conflict-from-empty",
            &empty,
        ),
        add_link_data_arguments(
            "conflict",
            "my-vend/my-old-app",
            "1.*",
            "conflict-from-oneOfEverything",
            &one_of_everything,
        ),
        add_link_data_arguments(
            "conflict",
            "my-vend/my-old-app",
            "1.*",
            "conflict-from-twoOfEverything",
            &two_of_everything,
        ),
    ]
}

#[ignore]
#[test]
fn test_add_link() {
    for (source_file, r#type, name, value, compare_against) in provide_add_link_data() {
        let tear_down = set_up();
        let composer_json = tear_down.working_dir().join("composer.json");
        std::fs::copy(&source_file, &composer_json).unwrap();
        let mut json_config_source = json_config_source(&composer_json);

        json_config_source.add_link(r#type, name, value).unwrap();

        assert_file_equals(&compare_against, &composer_json);
    }
}

/// Mirror of provideRemoveLinkData(): (sourceFile, type, name, compareAgainst).
fn provide_remove_link_data() -> Vec<(PathBuf, &'static str, &'static str, PathBuf)> {
    let one_of_everything = fixture_path("composer-one-of-everything.json");
    let two_of_everything = fixture_path("composer-two-of-everything.json");

    let remove_link_data_arguments =
        |r#type: &'static str,
         name: &'static str,
         fixture_basename: &str,
         after: Option<&PathBuf>| {
            let after = after.cloned().unwrap_or_else(|| {
                fixture_path(&format!("removeLink/{}-after.json", fixture_basename))
            });
            (
                fixture_path(&format!("removeLink/{}.json", fixture_basename)),
                r#type,
                name,
                after,
            )
        };

    vec![
        remove_link_data_arguments("require", "my-vend/my-lib", "require-to-empty", None),
        remove_link_data_arguments(
            "require",
            "my-vend/my-lib",
            "require-to-oneOfEverything",
            Some(&one_of_everything),
        ),
        remove_link_data_arguments(
            "require",
            "my-vend/my-lib",
            "require-to-twoOfEverything",
            Some(&two_of_everything),
        ),
        remove_link_data_arguments(
            "require-dev",
            "my-vend/my-lib-tests",
            "require-dev-to-empty",
            None,
        ),
        remove_link_data_arguments(
            "require-dev",
            "my-vend/my-lib-tests",
            "require-dev-to-oneOfEverything",
            Some(&one_of_everything),
        ),
        remove_link_data_arguments(
            "require-dev",
            "my-vend/my-lib-tests",
            "require-dev-to-twoOfEverything",
            Some(&two_of_everything),
        ),
        remove_link_data_arguments(
            "provide",
            "my-vend/my-lib-interface",
            "provide-to-empty",
            None,
        ),
        remove_link_data_arguments(
            "provide",
            "my-vend/my-lib-interface",
            "provide-to-oneOfEverything",
            Some(&one_of_everything),
        ),
        remove_link_data_arguments(
            "provide",
            "my-vend/my-lib-interface",
            "provide-to-twoOfEverything",
            Some(&two_of_everything),
        ),
        remove_link_data_arguments(
            "suggest",
            "my-vend/my-optional-extension",
            "suggest-to-empty",
            None,
        ),
        remove_link_data_arguments(
            "suggest",
            "my-vend/my-optional-extension",
            "suggest-to-oneOfEverything",
            Some(&one_of_everything),
        ),
        remove_link_data_arguments(
            "suggest",
            "my-vend/my-optional-extension",
            "suggest-to-twoOfEverything",
            Some(&two_of_everything),
        ),
        remove_link_data_arguments("replace", "my-vend/other-app", "replace-to-empty", None),
        remove_link_data_arguments(
            "replace",
            "my-vend/other-app",
            "replace-to-oneOfEverything",
            Some(&one_of_everything),
        ),
        remove_link_data_arguments(
            "replace",
            "my-vend/other-app",
            "replace-to-twoOfEverything",
            Some(&two_of_everything),
        ),
        remove_link_data_arguments("conflict", "my-vend/my-old-app", "conflict-to-empty", None),
        remove_link_data_arguments(
            "conflict",
            "my-vend/my-old-app",
            "conflict-to-oneOfEverything",
            Some(&one_of_everything),
        ),
        remove_link_data_arguments(
            "conflict",
            "my-vend/my-old-app",
            "conflict-to-twoOfEverything",
            Some(&two_of_everything),
        ),
    ]
}

#[ignore]
#[test]
fn test_remove_link() {
    for (source_file, r#type, name, compare_against) in provide_remove_link_data() {
        let tear_down = set_up();
        let composer_json = tear_down.working_dir().join("composer.json");
        std::fs::copy(&source_file, &composer_json).unwrap();
        let mut json_config_source = json_config_source(&composer_json);

        json_config_source.remove_link(r#type, name).unwrap();

        assert_file_equals(&compare_against, &composer_json);
    }
}
