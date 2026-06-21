//! ref: composer/tests/Composer/Test/Package/Loader/ArrayLoaderTest.php

use indexmap::IndexMap;
use shirabe::package::ArrayDumper;
use shirabe::package::Link;
use shirabe::package::loader::{ArrayLoader, LoaderInterface};
use shirabe::package::version::VersionParser;
use shirabe_php_shim::PhpMixed;

fn set_up() -> ArrayLoader {
    ArrayLoader::new(None, false)
}

fn s(value: &str) -> PhpMixed {
    PhpMixed::String(value.to_string())
}

fn map(entries: Vec<(&str, PhpMixed)>) -> PhpMixed {
    let mut m: IndexMap<String, PhpMixed> = IndexMap::new();
    for (k, v) in entries {
        m.insert(k.to_string(), v);
    }
    PhpMixed::Array(m)
}

fn make_config(entries: Vec<(&str, PhpMixed)>) -> IndexMap<String, PhpMixed> {
    let mut m: IndexMap<String, PhpMixed> = IndexMap::new();
    for (k, v) in entries {
        m.insert(k.to_string(), v);
    }
    m
}

fn list(items: Vec<PhpMixed>) -> PhpMixed {
    PhpMixed::List(items)
}

/// ref: ArrayLoaderTest::parseDumpProvider valid config
fn valid_config() -> IndexMap<String, PhpMixed> {
    make_config(vec![
        ("name", s("A/B")),
        ("version", s("1.2.3")),
        ("version_normalized", s("1.2.3.0")),
        ("description", s("Foo bar")),
        ("type", s("library")),
        ("keywords", list(vec![s("a"), s("b"), s("c")])),
        ("homepage", s("http://example.com")),
        ("license", list(vec![s("MIT"), s("GPLv3")])),
        (
            "authors",
            list(vec![map(vec![
                ("name", s("Bob")),
                ("email", s("bob@example.org")),
                ("homepage", s("example.org")),
                ("role", s("Developer")),
            ])]),
        ),
        (
            "funding",
            list(vec![map(vec![
                ("type", s("example")),
                ("url", s("https://example.org/fund")),
            ])]),
        ),
        ("require", map(vec![("foo/bar", s("1.0"))])),
        ("require-dev", map(vec![("foo/baz", s("1.0"))])),
        ("replace", map(vec![("foo/qux", s("1.0"))])),
        ("conflict", map(vec![("foo/quux", s("1.0"))])),
        ("provide", map(vec![("foo/quuux", s("1.0"))])),
        (
            "autoload",
            map(vec![
                ("psr-0", map(vec![("Ns\\Prefix", s("path"))])),
                ("classmap", list(vec![s("path"), s("path2")])),
            ]),
        ),
        ("include-path", list(vec![s("path3"), s("path4")])),
        ("target-dir", s("some/prefix")),
        (
            "extra",
            map(vec![(
                "random",
                map(vec![("things", s("of")), ("any", s("shape"))]),
            )]),
        ),
        ("bin", list(vec![s("bin1"), s("bin/foo")])),
        (
            "archive",
            map(vec![(
                "exclude",
                list(vec![s("/foo/bar"), s("baz"), s("!/foo/bar/baz")]),
            )]),
        ),
        (
            "transport-options",
            map(vec![(
                "ssl",
                map(vec![("local_cert", s("/opt/certs/test.pem"))]),
            )]),
        ),
        ("abandoned", s("foo/bar")),
    ])
}

/// ref: ArrayLoaderTest::fixConfigWhenLoadConfigIsFalse
fn fix_config_when_load_config_is_false(
    config: &IndexMap<String, PhpMixed>,
) -> IndexMap<String, PhpMixed> {
    let mut expected_config = config.clone();
    expected_config.shift_remove("transport-options");
    expected_config
}

#[ignore]
#[test]
fn test_self_version() {
    let loader = set_up();
    let config = make_config(vec![
        ("name", s("A")),
        ("version", s("1.2.3.4")),
        ("replace", map(vec![("foo", s("self.version"))])),
    ]);

    let package = loader.load(config, None).unwrap();
    let replaces = package.get_replaces();
    assert_eq!(
        "== 1.2.3.4",
        replaces.get("foo").unwrap().get_constraint().to_string()
    );
}

#[ignore]
#[test]
fn test_type_default() {
    let loader = set_up();
    let config = make_config(vec![("name", s("A")), ("version", s("1.0"))]);

    let package = loader.load(config, None).unwrap();
    assert_eq!("library", package.get_type());

    let config = make_config(vec![
        ("name", s("A")),
        ("version", s("1.0")),
        ("type", s("foo")),
    ]);

    let package = loader.load(config, None).unwrap();
    assert_eq!("foo", package.get_type());
}

#[ignore]
#[test]
fn test_normalized_version_optimization() {
    let loader = set_up();
    let config = make_config(vec![("name", s("A")), ("version", s("1.2.3"))]);

    let package = loader.load(config, None).unwrap();
    assert_eq!("1.2.3.0", package.get_version());

    let config = make_config(vec![
        ("name", s("A")),
        ("version", s("1.2.3")),
        ("version_normalized", s("1.2.3.4")),
    ]);

    let package = loader.load(config, None).unwrap();
    assert_eq!("1.2.3.4", package.get_version());
}

#[ignore]
#[test]
fn test_parse_dump_default_load_config() {
    let loader = set_up();
    let config = valid_config();
    let package = loader.load(config.clone(), None).unwrap();
    let dumper = ArrayDumper::new();
    let expected_config = fix_config_when_load_config_is_false(&config);
    assert_eq!(expected_config, dumper.dump(package));
}

#[ignore]
#[test]
fn test_parse_dump_true_load_config() {
    let config = valid_config();
    let loader = ArrayLoader::new(None, true);
    let package = loader.load(config.clone(), None).unwrap();
    let dumper = ArrayDumper::new();
    let expected_config = config;
    assert_eq!(expected_config, dumper.dump(package));
}

#[ignore]
#[test]
fn test_parse_dump_false_load_config() {
    let config = valid_config();
    let loader = ArrayLoader::new(None, false);
    let package = loader.load(config.clone(), None).unwrap();
    let dumper = ArrayDumper::new();
    let expected_config = fix_config_when_load_config_is_false(&config);
    assert_eq!(expected_config, dumper.dump(package));
}

#[ignore]
#[test]
fn test_package_with_branch_alias() {
    let loader = set_up();
    let config = make_config(vec![
        ("name", s("A")),
        ("version", s("dev-master")),
        (
            "extra",
            map(vec![(
                "branch-alias",
                map(vec![("dev-master", s("1.0.x-dev"))]),
            )]),
        ),
    ]);

    let package = loader.load(config, None).unwrap();

    assert!(package.as_complete_alias_package().is_some());
    assert_eq!("1.0.x-dev", package.get_pretty_version());

    let config = make_config(vec![
        ("name", s("A")),
        ("version", s("dev-master")),
        (
            "extra",
            map(vec![(
                "branch-alias",
                map(vec![("dev-master", s("1.0-dev"))]),
            )]),
        ),
    ]);

    let package = loader.load(config, None).unwrap();

    assert!(package.as_complete_alias_package().is_some());
    assert_eq!("1.0.x-dev", package.get_pretty_version());

    let config = make_config(vec![
        ("name", s("B")),
        ("version", s("4.x-dev")),
        (
            "extra",
            map(vec![(
                "branch-alias",
                map(vec![("4.x-dev", s("4.0.x-dev"))]),
            )]),
        ),
    ]);

    let package = loader.load(config, None).unwrap();

    assert!(package.as_complete_alias_package().is_some());
    assert_eq!("4.0.x-dev", package.get_pretty_version());

    let config = make_config(vec![
        ("name", s("B")),
        ("version", s("4.x-dev")),
        (
            "extra",
            map(vec![("branch-alias", map(vec![("4.x-dev", s("4.0-dev"))]))]),
        ),
    ]);

    let package = loader.load(config, None).unwrap();

    assert!(package.as_complete_alias_package().is_some());
    assert_eq!("4.0.x-dev", package.get_pretty_version());

    let config = make_config(vec![
        ("name", s("C")),
        ("version", s("4.x-dev")),
        (
            "extra",
            map(vec![(
                "branch-alias",
                map(vec![("4.x-dev", s("3.4.x-dev"))]),
            )]),
        ),
    ]);

    let package = loader.load(config, None).unwrap();

    assert!(package.as_complete_package().is_some());
    assert_eq!("4.x-dev", package.get_pretty_version());
}

#[ignore]
#[test]
fn test_package_aliasing_without_branch_alias() {
    let loader = set_up();
    // non-numeric gets a default alias
    let config = make_config(vec![
        ("name", s("A")),
        ("version", s("dev-main")),
        ("default-branch", PhpMixed::Bool(true)),
    ]);

    let package = loader.load(config, None).unwrap();

    assert!(package.as_complete_alias_package().is_some());
    assert_eq!(
        VersionParser::DEFAULT_BRANCH_ALIAS,
        package.get_pretty_version()
    );

    // non-default branch gets no alias even if non-numeric
    let config = make_config(vec![
        ("name", s("A")),
        ("version", s("dev-main")),
        ("default-branch", PhpMixed::Bool(false)),
    ]);

    let package = loader.load(config, None).unwrap();

    assert!(package.as_complete_package().is_some());
    assert_eq!("dev-main", package.get_pretty_version());

    // default branch gets no alias if already numeric
    let config = make_config(vec![
        ("name", s("A")),
        ("version", s("2.x-dev")),
        ("default-branch", PhpMixed::Bool(true)),
    ]);

    let package = loader.load(config, None).unwrap();

    assert!(package.as_complete_package().is_some());
    assert_eq!("2.9999999.9999999.9999999-dev", package.get_version());

    // default branch gets no alias if already numeric, with v prefix
    let config = make_config(vec![
        ("name", s("A")),
        ("version", s("v2.x-dev")),
        ("default-branch", PhpMixed::Bool(true)),
    ]);

    let package = loader.load(config, None).unwrap();

    assert!(package.as_complete_package().is_some());
    assert_eq!("2.9999999.9999999.9999999-dev", package.get_version());
}

#[ignore]
#[test]
fn test_abandoned() {
    let loader = set_up();
    let config = make_config(vec![
        ("name", s("A")),
        ("version", s("1.2.3.4")),
        ("abandoned", s("foo/bar")),
    ]);

    let package = loader.load(config, None).unwrap();
    let package = package.as_complete().unwrap();
    assert!(package.is_abandoned());
    assert_eq!(
        Some("foo/bar".to_string()),
        package.get_replacement_package()
    );
}

#[ignore]
#[test]
fn test_not_abandoned() {
    let loader = set_up();
    let config = make_config(vec![("name", s("A")), ("version", s("1.2.3.4"))]);

    let package = loader.load(config, None).unwrap();
    let package = package.as_complete().unwrap();
    assert!(!package.is_abandoned());
}

/// ref: ArrayLoaderTest::providePluginApiVersions
fn provide_plugin_api_versions() -> Vec<&'static str> {
    vec![
        "1.0",
        "1.0.0",
        "1.0.0.0",
        "1",
        "=1.0.0",
        "==1.0",
        "~1.0.0",
        "*",
        "3.0.*",
        "@stable",
        "1.0.0@stable",
        "^5.1",
        ">=1.0.0 <2.5",
        "x",
        "1.0.0-dev",
    ]
}

#[ignore]
#[test]
fn test_plugin_api_version_are_kept_as_declared() {
    let loader = set_up();
    for api_version in provide_plugin_api_versions() {
        let links = loader
            .parse_links(
                "Plugin",
                "9.9.9",
                Link::TYPE_REQUIRE,
                make_config(vec![("composer-plugin-api", s(api_version))]),
            )
            .unwrap();

        assert!(links.contains_key("composer-plugin-api"));
        assert_eq!(
            api_version,
            links
                .get("composer-plugin-api")
                .unwrap()
                .get_constraint()
                .get_pretty_string()
        );
    }
}

#[ignore]
#[test]
fn test_plugin_api_version_does_support_self_version() {
    let loader = set_up();
    let links = loader
        .parse_links(
            "Plugin",
            "6.6.6",
            Link::TYPE_REQUIRE,
            make_config(vec![("composer-plugin-api", s("self.version"))]),
        )
        .unwrap();

    assert!(links.contains_key("composer-plugin-api"));
    assert_eq!(
        "6.6.6",
        links
            .get("composer-plugin-api")
            .unwrap()
            .get_constraint()
            .get_pretty_string()
    );
}

#[ignore]
#[test]
fn test_parse_links_integer_target() {
    let loader = set_up();
    let links = loader
        .parse_links(
            "Plugin",
            "9.9.9",
            Link::TYPE_REQUIRE,
            make_config(vec![("1", s("dev-main"))]),
        )
        .unwrap();

    assert!(links.contains_key("1"));
}

#[ignore]
#[test]
fn test_parse_links_invalid_version() {
    let loader = set_up();
    let err = loader
        .parse_links(
            "Plugin",
            "9.9.9",
            Link::TYPE_REQUIRE,
            make_config(vec![("composer-plugin-api", s("^^^"))]),
        )
        .unwrap_err();
    assert_eq!(
        "Link constraint in Plugin requires > composer-plugin-api should be a valid version constraint, got \"^^^\"",
        err.to_string()
    );
}

#[ignore]
#[test]
fn test_none_string_version() {
    let loader = set_up();
    let config = make_config(vec![
        ("name", s("acme/package")),
        ("version", PhpMixed::Int(1)),
    ]);

    let package = loader.load(config, None).unwrap();
    assert_eq!("1", package.get_pretty_version());
}

#[ignore]
#[test]
fn test_none_string_source_dist_reference() {
    let loader = set_up();
    let config = make_config(vec![
        ("name", s("acme/package")),
        ("version", s("dev-main")),
        (
            "source",
            map(vec![
                ("type", s("svn")),
                ("url", s("https://example.org/")),
                ("reference", PhpMixed::Int(2019)),
            ]),
        ),
        (
            "dist",
            map(vec![
                ("type", s("zip")),
                ("url", s("https://example.org/")),
                ("reference", PhpMixed::Int(2019)),
            ]),
        ),
    ]);

    let package = loader.load(config, None).unwrap();
    assert_eq!(Some("2019".to_string()), package.get_source_reference());
    assert_eq!(Some("2019".to_string()), package.get_dist_reference());
}

#[ignore]
#[test]
fn test_branch_alias_integer_index() {
    let loader = set_up();
    let config = make_config(vec![
        ("name", s("acme/package")),
        ("version", s("dev-1")),
        (
            "extra",
            map(vec![("branch-alias", map(vec![("1", s("1.3-dev"))]))]),
        ),
        (
            "dist",
            map(vec![("type", s("zip")), ("url", s("https://example.org/"))]),
        ),
    ]);

    assert_eq!(None, loader.get_branch_alias(&config).unwrap());
}

#[ignore]
#[test]
fn test_package_links_require() {
    let loader = set_up();
    let config = make_config(vec![
        ("name", s("acme/package")),
        ("version", s("dev-1")),
        ("require", map(vec![("foo/bar", s("1.0"))])),
    ]);

    let package = loader.load(config, None).unwrap();
    assert!(package.get_requires().contains_key("foo/bar"));
    assert_eq!(
        "1.0",
        package
            .get_requires()
            .get("foo/bar")
            .unwrap()
            .get_constraint()
            .get_pretty_string()
    );
}

#[ignore]
#[test]
fn test_package_links_require_invalid() {
    let loader = set_up();
    let config = make_config(vec![
        ("name", s("acme/package")),
        ("version", s("dev-1")),
        (
            "require",
            map(vec![("foo/bar", map(vec![("random-string", s("1.0"))]))]),
        ),
    ]);

    let package = loader.load(config, None).unwrap();
    assert_eq!(0, package.get_requires().len());
}

#[ignore]
#[test]
fn test_package_links_replace() {
    let loader = set_up();
    let config = make_config(vec![
        ("name", s("acme/package")),
        ("version", s("dev-1")),
        ("replace", map(vec![("coyote/package", s("self.version"))])),
    ]);

    let package = loader.load(config, None).unwrap();
    assert!(package.get_replaces().contains_key("coyote/package"));
    assert_eq!(
        "dev-1",
        package
            .get_replaces()
            .get("coyote/package")
            .unwrap()
            .get_constraint()
            .get_pretty_string()
    );
}

#[ignore]
#[test]
fn test_package_links_replace_invalid() {
    let loader = set_up();
    let config = make_config(vec![
        ("name", s("acme/package")),
        ("version", s("dev-1")),
        ("replace", s("coyote/package")),
    ]);

    let package = loader.load(config, None).unwrap();
    assert_eq!(0, package.get_replaces().len());
}

#[ignore]
#[test]
fn test_support_string_value() {
    let loader = set_up();
    let config = make_config(vec![
        ("name", s("acme/package")),
        ("version", s("dev-1")),
        ("support", s("https://example.org")),
    ]);

    let package = loader.load(config, None).unwrap();
    let package = package.as_complete().unwrap();
    assert_eq!(0, package.get_support().len());
}

#[ignore]
#[test]
fn test_invalid_version() {
    let loader = set_up();
    let config = make_config(vec![("name", s("acme/package")), ("version", s("AA"))]);

    let err = loader.load(config, None).unwrap_err();
    assert_eq!(
        "Failed to normalize version for package \"acme/package\": Invalid version string \"AA\"",
        err.to_string()
    );
}
