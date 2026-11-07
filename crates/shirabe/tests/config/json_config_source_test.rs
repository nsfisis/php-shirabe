//! ref: composer/tests/Composer/Test/Config/JsonConfigSourceTest.php sha1:97705eb746c7a88f7083059db9ba74d3e6d0f1ac

use crate::test_case::get_unique_tmp_directory;
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe::config::{ConfigSourceInterface, JsonConfigSource};
use shirabe::json::JsonFile;
use shirabe::util::Filesystem;
use shirabe_php_shim::{PhpMixed, bconcat, path_from_bytes};
use std::path::PathBuf;
use tempfile::TempDir;

fn set_up() -> TearDown {
    let fs = Filesystem::new(None);
    let working_dir = get_unique_tmp_directory();
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

fn fixture_path(name: impl AsRef<std::path::Path>) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../composer/tests/Composer/Test/Config/Fixtures")
        .join(name)
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
    let json_file = JsonFile::new(config, None, None).unwrap();
    JsonConfigSource::new(std::rc::Rc::new(std::cell::RefCell::new(json_file)), false)
}

#[test]
fn test_add_repository() {
    let tear_down = set_up();
    let config = tear_down.working_dir().join("composer.json");
    std::fs::copy(fixture_path("composer-repositories.json"), &config).unwrap();
    let mut json_config_source = json_config_source(&config);

    let mut repo = IndexMap::new();
    repo.insert("type".into(), PhpMixed::string(BString::from("git")));
    repo.insert("url".into(), PhpMixed::string(BString::from("example.tld")));
    json_config_source
        .add_repository(bstr::BStr::new("example_tld"), PhpMixed::Array(repo), true)
        .unwrap();

    assert_file_equals(
        &fixture_path("config/config-with-exampletld-repository.json"),
        &config,
    );
}

#[test]
fn test_add_repository_as_list() {
    let tear_down = set_up();
    let config = tear_down.working_dir().join("composer.json");
    std::fs::copy(fixture_path("composer-repositories.json"), &config).unwrap();
    let mut json_config_source = json_config_source(&config);

    let mut repo = IndexMap::new();
    repo.insert("type".into(), PhpMixed::string(BString::from("git")));
    repo.insert("url".into(), PhpMixed::string(BString::from("example.tld")));
    json_config_source
        .add_repository(bstr::BStr::new(""), PhpMixed::Array(repo), true)
        .unwrap();

    assert_file_equals(
        &fixture_path("config/config-with-exampletld-repository-as-list.json"),
        &config,
    );
}

#[test]
fn test_add_repository_with_options() {
    let tear_down = set_up();
    let config = tear_down.working_dir().join("composer.json");
    std::fs::copy(fixture_path("composer-repositories.json"), &config).unwrap();
    let mut json_config_source = json_config_source(&config);

    let mut repo = IndexMap::new();
    repo.insert("type".into(), PhpMixed::string(BString::from("composer")));
    repo.insert(
        "url".into(),
        PhpMixed::string(BString::from("https://example.tld")),
    );
    {
        let mut local_cert = IndexMap::new();
        local_cert.insert(
            "local_cert".into(),
            PhpMixed::string(BString::from("/home/composer/.ssl/composer.pem")),
        );
        let mut ssl = IndexMap::new();
        ssl.insert("ssl".into(), PhpMixed::Array(local_cert));
        repo.insert("options".into(), PhpMixed::Array(ssl));
    }

    json_config_source
        .add_repository(bstr::BStr::new("example_tld"), PhpMixed::Array(repo), true)
        .unwrap();

    assert_file_equals(
        &fixture_path("config/config-with-exampletld-repository-and-options.json"),
        &config,
    );
}

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
    json_config_source
        .remove_repository(bstr::BStr::new("example_tld"))
        .unwrap();

    assert_file_equals(&fixture_path("composer-empty.json"), &config);
}

#[test]
fn test_add_packagist_repository_with_false_value() {
    let tear_down = set_up();
    let config = tear_down.working_dir().join("composer.json");
    std::fs::copy(fixture_path("composer-repositories.json"), &config).unwrap();
    let mut json_config_source = json_config_source(&config);
    json_config_source
        .add_repository(bstr::BStr::new("packagist"), PhpMixed::Bool(false), true)
        .unwrap();

    assert_file_equals(
        &fixture_path("config/config-with-packagist-false.json"),
        &config,
    );
}

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
    json_config_source
        .remove_repository(bstr::BStr::new("packagist"))
        .unwrap();

    assert_file_equals(&fixture_path("composer-empty.json"), &config);
}

/// Mirror of provideAddLinkData(): (sourceFile, type, name, value, compareAgainst).
fn provide_add_link_data() -> Vec<(
    PathBuf,
    &'static BStr,
    &'static BStr,
    &'static BStr,
    PathBuf,
)> {
    let empty = fixture_path("composer-empty.json");
    let one_of_everything = fixture_path("composer-one-of-everything.json");
    let two_of_everything = fixture_path("composer-two-of-everything.json");

    let add_link_data_arguments = |r#type: &'static BStr,
                                   name: &'static BStr,
                                   value: &'static BStr,
                                   fixture_basename: &BStr,
                                   before: &PathBuf| {
        (
            before.clone(),
            r#type,
            name,
            value,
            fixture_path(path_from_bytes(&bconcat!(
                "addLink/",
                fixture_basename,
                ".json"
            ))),
        )
    };

    vec![
        add_link_data_arguments(
            BStr::new("require"),
            BStr::new("my-vend/my-lib"),
            BStr::new("1.*"),
            BStr::new("require-from-empty"),
            &empty,
        ),
        add_link_data_arguments(
            BStr::new("require"),
            BStr::new("my-vend/my-lib"),
            BStr::new("1.*"),
            BStr::new("require-from-oneOfEverything"),
            &one_of_everything,
        ),
        add_link_data_arguments(
            BStr::new("require"),
            BStr::new("my-vend/my-lib"),
            BStr::new("1.*"),
            BStr::new("require-from-twoOfEverything"),
            &two_of_everything,
        ),
        add_link_data_arguments(
            BStr::new("require-dev"),
            BStr::new("my-vend/my-lib-tests"),
            BStr::new("1.*"),
            BStr::new("require-dev-from-empty"),
            &empty,
        ),
        add_link_data_arguments(
            BStr::new("require-dev"),
            BStr::new("my-vend/my-lib-tests"),
            BStr::new("1.*"),
            BStr::new("require-dev-from-oneOfEverything"),
            &one_of_everything,
        ),
        add_link_data_arguments(
            BStr::new("require-dev"),
            BStr::new("my-vend/my-lib-tests"),
            BStr::new("1.*"),
            BStr::new("require-dev-from-twoOfEverything"),
            &two_of_everything,
        ),
        add_link_data_arguments(
            BStr::new("provide"),
            BStr::new("my-vend/my-lib-interface"),
            BStr::new("1.*"),
            BStr::new("provide-from-empty"),
            &empty,
        ),
        add_link_data_arguments(
            BStr::new("provide"),
            BStr::new("my-vend/my-lib-interface"),
            BStr::new("1.*"),
            BStr::new("provide-from-oneOfEverything"),
            &one_of_everything,
        ),
        add_link_data_arguments(
            BStr::new("provide"),
            BStr::new("my-vend/my-lib-interface"),
            BStr::new("1.*"),
            BStr::new("provide-from-twoOfEverything"),
            &two_of_everything,
        ),
        add_link_data_arguments(
            BStr::new("suggest"),
            BStr::new("my-vend/my-optional-extension"),
            BStr::new("1.*"),
            BStr::new("suggest-from-empty"),
            &empty,
        ),
        add_link_data_arguments(
            BStr::new("suggest"),
            BStr::new("my-vend/my-optional-extension"),
            BStr::new("1.*"),
            BStr::new("suggest-from-oneOfEverything"),
            &one_of_everything,
        ),
        add_link_data_arguments(
            BStr::new("suggest"),
            BStr::new("my-vend/my-optional-extension"),
            BStr::new("1.*"),
            BStr::new("suggest-from-twoOfEverything"),
            &two_of_everything,
        ),
        add_link_data_arguments(
            BStr::new("replace"),
            BStr::new("my-vend/other-app"),
            BStr::new("1.*"),
            BStr::new("replace-from-empty"),
            &empty,
        ),
        add_link_data_arguments(
            BStr::new("replace"),
            BStr::new("my-vend/other-app"),
            BStr::new("1.*"),
            BStr::new("replace-from-oneOfEverything"),
            &one_of_everything,
        ),
        add_link_data_arguments(
            BStr::new("replace"),
            BStr::new("my-vend/other-app"),
            BStr::new("1.*"),
            BStr::new("replace-from-twoOfEverything"),
            &two_of_everything,
        ),
        add_link_data_arguments(
            BStr::new("conflict"),
            BStr::new("my-vend/my-old-app"),
            BStr::new("1.*"),
            BStr::new("conflict-from-empty"),
            &empty,
        ),
        add_link_data_arguments(
            BStr::new("conflict"),
            BStr::new("my-vend/my-old-app"),
            BStr::new("1.*"),
            BStr::new("conflict-from-oneOfEverything"),
            &one_of_everything,
        ),
        add_link_data_arguments(
            BStr::new("conflict"),
            BStr::new("my-vend/my-old-app"),
            BStr::new("1.*"),
            BStr::new("conflict-from-twoOfEverything"),
            &two_of_everything,
        ),
    ]
}

#[test]
fn test_add_link() {
    for (source_file, r#type, name, value, compare_against) in provide_add_link_data() {
        let tear_down = set_up();
        let composer_json = tear_down.working_dir().join("composer.json");
        std::fs::copy(&source_file, &composer_json).unwrap();
        let mut json_config_source = json_config_source(&composer_json);

        json_config_source
            .add_link(
                bstr::BStr::new(r#type),
                bstr::BStr::new(name),
                bstr::BStr::new(value),
            )
            .unwrap();

        assert_file_equals(&compare_against, &composer_json);
    }
}

/// Mirror of provideRemoveLinkData(): (sourceFile, type, name, compareAgainst).
fn provide_remove_link_data() -> Vec<(PathBuf, &'static BStr, &'static BStr, PathBuf)> {
    let one_of_everything = fixture_path("composer-one-of-everything.json");
    let two_of_everything = fixture_path("composer-two-of-everything.json");

    let remove_link_data_arguments =
        |r#type: &'static BStr,
         name: &'static BStr,
         fixture_basename: &BStr,
         after: Option<&PathBuf>| {
            let after = after.cloned().unwrap_or_else(|| {
                fixture_path(path_from_bytes(&bconcat!(
                    "removeLink/",
                    fixture_basename,
                    "-after.json"
                )))
            });
            (
                fixture_path(path_from_bytes(&bconcat!(
                    "removeLink/",
                    fixture_basename,
                    ".json"
                ))),
                r#type,
                name,
                after,
            )
        };

    vec![
        remove_link_data_arguments(
            BStr::new("require"),
            BStr::new("my-vend/my-lib"),
            BStr::new("require-to-empty"),
            None,
        ),
        remove_link_data_arguments(
            BStr::new("require"),
            BStr::new("my-vend/my-lib"),
            BStr::new("require-to-oneOfEverything"),
            Some(&one_of_everything),
        ),
        remove_link_data_arguments(
            BStr::new("require"),
            BStr::new("my-vend/my-lib"),
            BStr::new("require-to-twoOfEverything"),
            Some(&two_of_everything),
        ),
        remove_link_data_arguments(
            BStr::new("require-dev"),
            BStr::new("my-vend/my-lib-tests"),
            BStr::new("require-dev-to-empty"),
            None,
        ),
        remove_link_data_arguments(
            BStr::new("require-dev"),
            BStr::new("my-vend/my-lib-tests"),
            BStr::new("require-dev-to-oneOfEverything"),
            Some(&one_of_everything),
        ),
        remove_link_data_arguments(
            BStr::new("require-dev"),
            BStr::new("my-vend/my-lib-tests"),
            BStr::new("require-dev-to-twoOfEverything"),
            Some(&two_of_everything),
        ),
        remove_link_data_arguments(
            BStr::new("provide"),
            BStr::new("my-vend/my-lib-interface"),
            BStr::new("provide-to-empty"),
            None,
        ),
        remove_link_data_arguments(
            BStr::new("provide"),
            BStr::new("my-vend/my-lib-interface"),
            BStr::new("provide-to-oneOfEverything"),
            Some(&one_of_everything),
        ),
        remove_link_data_arguments(
            BStr::new("provide"),
            BStr::new("my-vend/my-lib-interface"),
            BStr::new("provide-to-twoOfEverything"),
            Some(&two_of_everything),
        ),
        remove_link_data_arguments(
            BStr::new("suggest"),
            BStr::new("my-vend/my-optional-extension"),
            BStr::new("suggest-to-empty"),
            None,
        ),
        remove_link_data_arguments(
            BStr::new("suggest"),
            BStr::new("my-vend/my-optional-extension"),
            BStr::new("suggest-to-oneOfEverything"),
            Some(&one_of_everything),
        ),
        remove_link_data_arguments(
            BStr::new("suggest"),
            BStr::new("my-vend/my-optional-extension"),
            BStr::new("suggest-to-twoOfEverything"),
            Some(&two_of_everything),
        ),
        remove_link_data_arguments(
            BStr::new("replace"),
            BStr::new("my-vend/other-app"),
            BStr::new("replace-to-empty"),
            None,
        ),
        remove_link_data_arguments(
            BStr::new("replace"),
            BStr::new("my-vend/other-app"),
            BStr::new("replace-to-oneOfEverything"),
            Some(&one_of_everything),
        ),
        remove_link_data_arguments(
            BStr::new("replace"),
            BStr::new("my-vend/other-app"),
            BStr::new("replace-to-twoOfEverything"),
            Some(&two_of_everything),
        ),
        remove_link_data_arguments(
            BStr::new("conflict"),
            BStr::new("my-vend/my-old-app"),
            BStr::new("conflict-to-empty"),
            None,
        ),
        remove_link_data_arguments(
            BStr::new("conflict"),
            BStr::new("my-vend/my-old-app"),
            BStr::new("conflict-to-oneOfEverything"),
            Some(&one_of_everything),
        ),
        remove_link_data_arguments(
            BStr::new("conflict"),
            BStr::new("my-vend/my-old-app"),
            BStr::new("conflict-to-twoOfEverything"),
            Some(&two_of_everything),
        ),
    ]
}

#[test]
fn test_remove_link() {
    for (source_file, r#type, name, compare_against) in provide_remove_link_data() {
        let tear_down = set_up();
        let composer_json = tear_down.working_dir().join("composer.json");
        std::fs::copy(&source_file, &composer_json).unwrap();
        let mut json_config_source = json_config_source(&composer_json);

        json_config_source
            .remove_link(bstr::BStr::new(r#type), bstr::BStr::new(name))
            .unwrap();

        assert_file_equals(&compare_against, &composer_json);
    }
}
