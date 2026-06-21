//! ref: composer/tests/Composer/Test/InstalledVersionsTest.php

// setUpBeforeClass reflects into ClassLoader::registeredLoaders to make it seem like no class
// loaders are registered; this is not ported because InstalledVersions::reload already disables
// the multiple-ClassLoader-based checks by setting installedIsLocalDir to false. The PHP setUp
// loads the installed_relative.php fixture via `require`; here the fixture is built inline as an
// IndexMap<String, PhpMixed> with the tmp root substituted for `$dir`.

use indexmap::IndexMap;
use shirabe::installed_versions::InstalledVersions;
use shirabe_php_shim::{PhpMixed, realpath};
use shirabe_semver::version_parser::VersionParser;
use tempfile::TempDir;

fn arr(entries: Vec<(&str, PhpMixed)>) -> PhpMixed {
    let mut m = IndexMap::new();
    for (k, v) in entries {
        m.insert(k.to_string(), v);
    }
    PhpMixed::Array(m)
}

fn list(items: Vec<PhpMixed>) -> PhpMixed {
    PhpMixed::List(items)
}

fn s(value: &str) -> PhpMixed {
    PhpMixed::String(value.to_string())
}

/// Builds the installed_relative.php fixture with `$dir` substituted by `dir`.
fn fixture(dir: &str) -> IndexMap<String, PhpMixed> {
    let root_install_path = format!("{}/./", dir);
    let mut data = IndexMap::new();

    data.insert(
        "root".to_string(),
        arr(vec![
            ("name", s("__root__")),
            ("pretty_version", s("dev-master")),
            ("version", s("dev-master")),
            ("reference", s("sourceref-by-default")),
            ("type", s("library")),
            ("install_path", s(&root_install_path)),
            ("aliases", list(vec![s("1.10.x-dev")])),
            ("dev", PhpMixed::Bool(true)),
        ]),
    );

    data.insert(
        "versions".to_string(),
        arr(vec![
            (
                "__root__",
                arr(vec![
                    ("pretty_version", s("dev-master")),
                    ("version", s("dev-master")),
                    ("reference", s("sourceref-by-default")),
                    ("type", s("library")),
                    ("install_path", s(&root_install_path)),
                    ("aliases", list(vec![s("1.10.x-dev")])),
                    ("dev_requirement", PhpMixed::Bool(false)),
                ]),
            ),
            (
                "a/provider",
                arr(vec![
                    ("pretty_version", s("1.1")),
                    ("version", s("1.1.0.0")),
                    ("reference", s("distref-as-no-source")),
                    ("type", s("library")),
                    ("install_path", s(&format!("{}/vendor/a/provider", dir))),
                    ("aliases", list(vec![])),
                    ("dev_requirement", PhpMixed::Bool(false)),
                ]),
            ),
            (
                "a/provider2",
                arr(vec![
                    ("pretty_version", s("1.2")),
                    ("version", s("1.2.0.0")),
                    ("reference", s("distref-as-installed-from-dist")),
                    ("type", s("library")),
                    ("install_path", s(&format!("{}/vendor/a/provider2", dir))),
                    ("aliases", list(vec![s("1.4")])),
                    ("dev_requirement", PhpMixed::Bool(false)),
                ]),
            ),
            (
                "b/replacer",
                arr(vec![
                    ("pretty_version", s("2.2")),
                    ("version", s("2.2.0.0")),
                    ("reference", PhpMixed::Null),
                    ("type", s("library")),
                    ("install_path", s(&format!("{}/vendor/b/replacer", dir))),
                    ("aliases", list(vec![])),
                    ("dev_requirement", PhpMixed::Bool(false)),
                ]),
            ),
            (
                "c/c",
                arr(vec![
                    ("pretty_version", s("3.0")),
                    ("version", s("3.0.0.0")),
                    ("reference", PhpMixed::Null),
                    ("type", s("library")),
                    ("install_path", s("/foo/bar/vendor/c/c")),
                    ("aliases", list(vec![])),
                    ("dev_requirement", PhpMixed::Bool(true)),
                ]),
            ),
            (
                "foo/impl",
                arr(vec![
                    ("dev_requirement", PhpMixed::Bool(false)),
                    (
                        "provided",
                        list(vec![s("^1.1"), s("1.2"), s("1.4"), s("2.0")]),
                    ),
                ]),
            ),
            (
                "foo/impl2",
                arr(vec![
                    ("dev_requirement", PhpMixed::Bool(false)),
                    ("provided", list(vec![s("2.0")])),
                    ("replaced", list(vec![s("2.2")])),
                ]),
            ),
            (
                "foo/replaced",
                arr(vec![
                    ("dev_requirement", PhpMixed::Bool(false)),
                    ("replaced", list(vec![s("^3.0")])),
                ]),
            ),
            (
                "meta/package",
                arr(vec![
                    ("pretty_version", s("3.0")),
                    ("version", s("3.0.0.0")),
                    ("reference", PhpMixed::Null),
                    ("type", s("metapackage")),
                    ("install_path", PhpMixed::Null),
                    ("aliases", list(vec![])),
                    ("dev_requirement", PhpMixed::Bool(false)),
                ]),
            ),
        ]),
    );

    data
}

/// Returns the tmp root (kept alive for the duration of the test) and its path string.
fn set_up() -> (TempDir, String) {
    let root = TempDir::new().unwrap();
    let dir = root.path().to_str().unwrap().to_string();

    InstalledVersions::reload(fixture(&dir));

    (root, dir)
}

#[ignore]
#[test]
fn test_get_installed_packages() {
    let (_root, _dir) = set_up();

    let names = vec![
        "__root__".to_string(),
        "a/provider".to_string(),
        "a/provider2".to_string(),
        "b/replacer".to_string(),
        "c/c".to_string(),
        "foo/impl".to_string(),
        "foo/impl2".to_string(),
        "foo/replaced".to_string(),
        "meta/package".to_string(),
    ];
    assert_eq!(names, InstalledVersions::get_installed_packages());
}

#[ignore]
#[test]
fn test_is_installed() {
    let (_root, _dir) = set_up();

    let cases: Vec<(bool, &str, bool)> = vec![
        (true, "foo/impl", true),
        (true, "foo/replaced", true),
        (true, "c/c", true),
        (false, "c/c", false),
        (true, "__root__", true),
        (true, "b/replacer", true),
        (false, "not/there", true),
        (true, "meta/package", true),
    ];

    for (expected, name, include_dev_requirements) in cases {
        assert_eq!(
            expected,
            InstalledVersions::is_installed(name, include_dev_requirements)
        );
    }
}

#[ignore]
#[test]
fn test_satisfies() {
    let (_root, _dir) = set_up();

    let cases: Vec<(bool, &str, &str)> = vec![
        (true, "foo/impl", "1.5"),
        (true, "foo/impl", "1.2"),
        (true, "foo/impl", "^1.0"),
        (true, "foo/impl", "^3 || ^2"),
        (false, "foo/impl", "^3"),
        (true, "foo/replaced", "3.5"),
        (true, "foo/replaced", "^3.2"),
        (false, "foo/replaced", "4.0"),
        (true, "c/c", "3.0.0"),
        (true, "c/c", "^3"),
        (false, "c/c", "^3.1"),
        (true, "__root__", "dev-master"),
        (true, "__root__", "^1.10"),
        (false, "__root__", "^2"),
        (true, "b/replacer", "^2.1"),
        (false, "b/replacer", "^2.3"),
        (true, "a/provider2", "^1.2"),
        (true, "a/provider2", "^1.4"),
        (false, "a/provider2", "^1.5"),
    ];

    for (expected, name, constraint) in cases {
        assert_eq!(
            expected,
            InstalledVersions::satisfies(&VersionParser, name, Some(constraint)).unwrap()
        );
    }
}

#[ignore]
#[test]
fn test_get_version_ranges() {
    let (_root, _dir) = set_up();

    let cases: Vec<(&str, &str)> = vec![
        ("dev-master || 1.10.x-dev", "__root__"),
        ("^1.1 || 1.2 || 1.4 || 2.0", "foo/impl"),
        ("2.2 || 2.0", "foo/impl2"),
        ("^3.0", "foo/replaced"),
        ("1.1", "a/provider"),
        ("1.2 || 1.4", "a/provider2"),
        ("2.2", "b/replacer"),
        ("3.0", "c/c"),
    ];

    for (expected, name) in cases {
        assert_eq!(
            expected,
            InstalledVersions::get_version_ranges(name).unwrap()
        );
    }
}

#[ignore]
#[test]
fn test_get_version() {
    let (_root, _dir) = set_up();

    let cases: Vec<(Option<&str>, &str)> = vec![
        (Some("dev-master"), "__root__"),
        (None, "foo/impl"),
        (None, "foo/impl2"),
        (None, "foo/replaced"),
        (Some("1.1.0.0"), "a/provider"),
        (Some("1.2.0.0"), "a/provider2"),
        (Some("2.2.0.0"), "b/replacer"),
        (Some("3.0.0.0"), "c/c"),
    ];

    for (expected, name) in cases {
        assert_eq!(
            expected.map(|s| s.to_string()),
            InstalledVersions::get_version(name).unwrap()
        );
    }
}

#[ignore]
#[test]
fn test_get_pretty_version() {
    let (_root, _dir) = set_up();

    let cases: Vec<(Option<&str>, &str)> = vec![
        (Some("dev-master"), "__root__"),
        (None, "foo/impl"),
        (None, "foo/impl2"),
        (None, "foo/replaced"),
        (Some("1.1"), "a/provider"),
        (Some("1.2"), "a/provider2"),
        (Some("2.2"), "b/replacer"),
        (Some("3.0"), "c/c"),
    ];

    for (expected, name) in cases {
        assert_eq!(
            expected.map(|s| s.to_string()),
            InstalledVersions::get_pretty_version(name).unwrap()
        );
    }
}

#[ignore]
#[test]
fn test_get_version_out_of_bounds() {
    let (_root, _dir) = set_up();

    assert!(InstalledVersions::get_version("not/installed").is_err());
}

#[ignore]
#[test]
fn test_get_root_package() {
    let (_root, dir) = set_up();

    let expected = {
        let mut m = IndexMap::new();
        m.insert("name".to_string(), s("__root__"));
        m.insert("pretty_version".to_string(), s("dev-master"));
        m.insert("version".to_string(), s("dev-master"));
        m.insert("reference".to_string(), s("sourceref-by-default"));
        m.insert("type".to_string(), s("library"));
        m.insert("install_path".to_string(), s(&format!("{}/./", dir)));
        m.insert("aliases".to_string(), list(vec![s("1.10.x-dev")]));
        m.insert("dev".to_string(), PhpMixed::Bool(true));
        m
    };

    assert_eq!(expected, InstalledVersions::get_root_package());
}

#[ignore = "InstalledVersions::get_raw_data not ported (only get_all_raw_data exists)"]
#[test]
fn test_get_raw_data() {
    let _root = set_up();
    todo!()
}

#[ignore]
#[test]
fn test_get_reference() {
    let (_root, _dir) = set_up();

    let cases: Vec<(Option<&str>, &str)> = vec![
        (Some("sourceref-by-default"), "__root__"),
        (None, "foo/impl"),
        (None, "foo/impl2"),
        (None, "foo/replaced"),
        (Some("distref-as-no-source"), "a/provider"),
        (Some("distref-as-installed-from-dist"), "a/provider2"),
        (None, "b/replacer"),
        (None, "c/c"),
    ];

    for (expected, name) in cases {
        assert_eq!(
            expected.map(|s| s.to_string()),
            InstalledVersions::get_reference(name).unwrap()
        );
    }
}

#[ignore]
#[test]
fn test_get_installed_packages_by_type() {
    let (_root, _dir) = set_up();

    let names = vec![
        "__root__".to_string(),
        "a/provider".to_string(),
        "a/provider2".to_string(),
        "b/replacer".to_string(),
        "c/c".to_string(),
    ];

    assert_eq!(
        names,
        InstalledVersions::get_installed_packages_by_type("library")
    );
}

#[ignore]
#[test]
fn test_get_install_path() {
    let (_root, dir) = set_up();

    assert_eq!(
        realpath(&dir),
        realpath(
            &InstalledVersions::get_install_path("__root__")
                .unwrap()
                .unwrap()
        )
    );
    assert_eq!(
        Some("/foo/bar/vendor/c/c".to_string()),
        InstalledVersions::get_install_path("c/c").unwrap()
    );
    assert_eq!(
        None,
        InstalledVersions::get_install_path("foo/impl").unwrap()
    );
}

#[ignore]
#[test]
fn test_with_class_loader_loaded() {
    let (_root, _dir) = set_up();

    // The reflection into ClassLoader::registeredLoaders is not ported; installedIsLocalDir is
    // toggled directly via the exposed setter to mirror the PHP reflection on it.
    InstalledVersions::set_installed_is_local_dir(true);

    assert!(!InstalledVersions::is_installed("foo/bar", true));

    let reload_data = {
        let mut m = IndexMap::new();
        m.insert(
            "root".to_string(),
            PhpMixed::Array(InstalledVersions::get_root_package()),
        );
        m.insert(
            "versions".to_string(),
            arr(vec![(
                "foo/bar",
                arr(vec![
                    ("version", s("1.0.0")),
                    ("dev_requirement", PhpMixed::Bool(false)),
                ]),
            )]),
        );
        m
    };
    InstalledVersions::reload(reload_data);
    assert!(InstalledVersions::is_installed("foo/bar", true));
}
