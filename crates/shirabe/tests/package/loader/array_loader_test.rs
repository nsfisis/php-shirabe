//! ref: composer/tests/Composer/Test/Package/Loader/ArrayLoaderTest.php

use bstr::{B, BStr, BString};
use indexmap::IndexMap;
use shirabe::package::loader::{ArrayLoader, LoaderInterface};
use shirabe::package::version::VersionParser;
use shirabe::package::{ArrayDumper, Link};
use shirabe_php_shim::{PhpMixed, bconcat, error_message};

fn set_up() -> ArrayLoader {
    ArrayLoader::new(None, false)
}

fn s(value: &BStr) -> PhpMixed {
    PhpMixed::string(bconcat!(value))
}

fn map(entries: Vec<(&BStr, PhpMixed)>) -> PhpMixed {
    let mut m: IndexMap<BString, PhpMixed> = IndexMap::new();
    for (k, v) in entries {
        m.insert(bconcat!(k), v);
    }
    PhpMixed::Array(m)
}

fn make_config(entries: Vec<(&BStr, PhpMixed)>) -> IndexMap<BString, PhpMixed> {
    let mut m: IndexMap<BString, PhpMixed> = IndexMap::new();
    for (k, v) in entries {
        m.insert(bconcat!(k), v);
    }
    m
}

fn list(items: Vec<PhpMixed>) -> PhpMixed {
    PhpMixed::List(items)
}

/// ref: ArrayLoaderTest::parseDumpProvider valid config
fn valid_config() -> IndexMap<BString, PhpMixed> {
    make_config(vec![
        (BStr::new("name"), s(BStr::new("A/B"))),
        (BStr::new("version"), s(BStr::new("1.2.3"))),
        (BStr::new("version_normalized"), s(BStr::new("1.2.3.0"))),
        (BStr::new("description"), s(BStr::new("Foo bar"))),
        (BStr::new("type"), s(BStr::new("library"))),
        (
            BStr::new("keywords"),
            list(vec![
                s(BStr::new("a")),
                s(BStr::new("b")),
                s(BStr::new("c")),
            ]),
        ),
        (BStr::new("homepage"), s(BStr::new("http://example.com"))),
        (
            BStr::new("license"),
            list(vec![s(BStr::new("MIT")), s(BStr::new("GPLv3"))]),
        ),
        (
            BStr::new("authors"),
            list(vec![map(vec![
                (BStr::new("name"), s(BStr::new("Bob"))),
                (BStr::new("email"), s(BStr::new("bob@example.org"))),
                (BStr::new("homepage"), s(BStr::new("example.org"))),
                (BStr::new("role"), s(BStr::new("Developer"))),
            ])]),
        ),
        (
            BStr::new("funding"),
            list(vec![map(vec![
                (BStr::new("type"), s(BStr::new("example"))),
                (BStr::new("url"), s(BStr::new("https://example.org/fund"))),
            ])]),
        ),
        (
            BStr::new("require"),
            map(vec![(BStr::new("foo/bar"), s(BStr::new("1.0")))]),
        ),
        (
            BStr::new("require-dev"),
            map(vec![(BStr::new("foo/baz"), s(BStr::new("1.0")))]),
        ),
        (
            BStr::new("replace"),
            map(vec![(BStr::new("foo/qux"), s(BStr::new("1.0")))]),
        ),
        (
            BStr::new("conflict"),
            map(vec![(BStr::new("foo/quux"), s(BStr::new("1.0")))]),
        ),
        (
            BStr::new("provide"),
            map(vec![(BStr::new("foo/quuux"), s(BStr::new("1.0")))]),
        ),
        (
            BStr::new("autoload"),
            map(vec![
                (
                    BStr::new("psr-0"),
                    map(vec![(BStr::new("Ns\\Prefix"), s(BStr::new("path")))]),
                ),
                (
                    BStr::new("classmap"),
                    list(vec![s(BStr::new("path")), s(BStr::new("path2"))]),
                ),
            ]),
        ),
        (
            BStr::new("include-path"),
            list(vec![s(BStr::new("path3")), s(BStr::new("path4"))]),
        ),
        (BStr::new("target-dir"), s(BStr::new("some/prefix"))),
        (
            BStr::new("extra"),
            map(vec![(
                BStr::new("random"),
                map(vec![
                    (BStr::new("things"), s(BStr::new("of"))),
                    (BStr::new("any"), s(BStr::new("shape"))),
                ]),
            )]),
        ),
        (
            BStr::new("bin"),
            list(vec![s(BStr::new("bin1")), s(BStr::new("bin/foo"))]),
        ),
        (
            BStr::new("archive"),
            map(vec![(
                BStr::new("exclude"),
                list(vec![
                    s(BStr::new("/foo/bar")),
                    s(BStr::new("baz")),
                    s(BStr::new("!/foo/bar/baz")),
                ]),
            )]),
        ),
        (
            BStr::new("transport-options"),
            map(vec![(
                BStr::new("ssl"),
                map(vec![(
                    BStr::new("local_cert"),
                    s(BStr::new("/opt/certs/test.pem")),
                )]),
            )]),
        ),
        (BStr::new("abandoned"), s(BStr::new("foo/bar"))),
    ])
}

/// ref: ArrayLoaderTest::fixConfigWhenLoadConfigIsFalse
fn fix_config_when_load_config_is_false(
    config: &IndexMap<BString, PhpMixed>,
) -> IndexMap<BString, PhpMixed> {
    let mut expected_config = config.clone();
    expected_config.shift_remove(B("transport-options"));
    expected_config
}

#[test]
fn test_self_version() {
    let loader = set_up();
    let config = make_config(vec![
        (BStr::new("name"), s(BStr::new("A"))),
        (BStr::new("version"), s(BStr::new("1.2.3.4"))),
        (
            BStr::new("replace"),
            map(vec![(BStr::new("foo"), s(BStr::new("self.version")))]),
        ),
    ]);

    let package = loader.load(config, None).unwrap();
    let replaces = package.get_replaces();
    assert_eq!(
        "== 1.2.3.4",
        replaces
            .get(bstr::B("foo"))
            .unwrap()
            .get_constraint()
            .to_php_string()
    );
}

#[test]
fn test_type_default() {
    let loader = set_up();
    let config = make_config(vec![
        (BStr::new("name"), s(BStr::new("A"))),
        (BStr::new("version"), s(BStr::new("1.0"))),
    ]);

    let package = loader.load(config, None).unwrap();
    assert_eq!("library", package.get_type());

    let config = make_config(vec![
        (BStr::new("name"), s(BStr::new("A"))),
        (BStr::new("version"), s(BStr::new("1.0"))),
        (BStr::new("type"), s(BStr::new("foo"))),
    ]);

    let package = loader.load(config, None).unwrap();
    assert_eq!("foo", package.get_type());
}

#[test]
fn test_normalized_version_optimization() {
    let loader = set_up();
    let config = make_config(vec![
        (BStr::new("name"), s(BStr::new("A"))),
        (BStr::new("version"), s(BStr::new("1.2.3"))),
    ]);

    let package = loader.load(config, None).unwrap();
    assert_eq!("1.2.3.0", package.get_version());

    let config = make_config(vec![
        (BStr::new("name"), s(BStr::new("A"))),
        (BStr::new("version"), s(BStr::new("1.2.3"))),
        (BStr::new("version_normalized"), s(BStr::new("1.2.3.4"))),
    ]);

    let package = loader.load(config, None).unwrap();
    assert_eq!("1.2.3.4", package.get_version());
}

#[test]
fn test_parse_dump_default_load_config() {
    let loader = set_up();
    let config = valid_config();
    let package = loader.load(config.clone(), None).unwrap();
    let dumper = ArrayDumper::new();
    let expected_config = fix_config_when_load_config_is_false(&config);
    assert_eq!(expected_config, dumper.dump(package));
}

#[test]
fn test_parse_dump_true_load_config() {
    let config = valid_config();
    let loader = ArrayLoader::new(None, true);
    let package = loader.load(config.clone(), None).unwrap();
    let dumper = ArrayDumper::new();
    let expected_config = config;
    assert_eq!(expected_config, dumper.dump(package));
}

#[test]
fn test_parse_dump_false_load_config() {
    let config = valid_config();
    let loader = ArrayLoader::new(None, false);
    let package = loader.load(config.clone(), None).unwrap();
    let dumper = ArrayDumper::new();
    let expected_config = fix_config_when_load_config_is_false(&config);
    assert_eq!(expected_config, dumper.dump(package));
}

#[test]
fn test_package_with_branch_alias() {
    let loader = set_up();
    let config = make_config(vec![
        (BStr::new("name"), s(BStr::new("A"))),
        (BStr::new("version"), s(BStr::new("dev-master"))),
        (
            BStr::new("extra"),
            map(vec![(
                BStr::new("branch-alias"),
                map(vec![(BStr::new("dev-master"), s(BStr::new("1.0.x-dev")))]),
            )]),
        ),
    ]);

    let package = loader.load(config, None).unwrap();

    assert!(package.as_complete_alias_package().is_some());
    assert_eq!("1.0.x-dev", package.get_pretty_version());

    let config = make_config(vec![
        (BStr::new("name"), s(BStr::new("A"))),
        (BStr::new("version"), s(BStr::new("dev-master"))),
        (
            BStr::new("extra"),
            map(vec![(
                BStr::new("branch-alias"),
                map(vec![(BStr::new("dev-master"), s(BStr::new("1.0-dev")))]),
            )]),
        ),
    ]);

    let package = loader.load(config, None).unwrap();

    assert!(package.as_complete_alias_package().is_some());
    assert_eq!("1.0.x-dev", package.get_pretty_version());

    let config = make_config(vec![
        (BStr::new("name"), s(BStr::new("B"))),
        (BStr::new("version"), s(BStr::new("4.x-dev"))),
        (
            BStr::new("extra"),
            map(vec![(
                BStr::new("branch-alias"),
                map(vec![(BStr::new("4.x-dev"), s(BStr::new("4.0.x-dev")))]),
            )]),
        ),
    ]);

    let package = loader.load(config, None).unwrap();

    assert!(package.as_complete_alias_package().is_some());
    assert_eq!("4.0.x-dev", package.get_pretty_version());

    let config = make_config(vec![
        (BStr::new("name"), s(BStr::new("B"))),
        (BStr::new("version"), s(BStr::new("4.x-dev"))),
        (
            BStr::new("extra"),
            map(vec![(
                BStr::new("branch-alias"),
                map(vec![(BStr::new("4.x-dev"), s(BStr::new("4.0-dev")))]),
            )]),
        ),
    ]);

    let package = loader.load(config, None).unwrap();

    assert!(package.as_complete_alias_package().is_some());
    assert_eq!("4.0.x-dev", package.get_pretty_version());

    let config = make_config(vec![
        (BStr::new("name"), s(BStr::new("C"))),
        (BStr::new("version"), s(BStr::new("4.x-dev"))),
        (
            BStr::new("extra"),
            map(vec![(
                BStr::new("branch-alias"),
                map(vec![(BStr::new("4.x-dev"), s(BStr::new("3.4.x-dev")))]),
            )]),
        ),
    ]);

    let package = loader.load(config, None).unwrap();

    assert!(package.as_complete_package().is_some());
    assert_eq!("4.x-dev", package.get_pretty_version());
}

#[test]
fn test_package_aliasing_without_branch_alias() {
    let loader = set_up();
    // non-numeric gets a default alias
    let config = make_config(vec![
        (BStr::new("name"), s(BStr::new("A"))),
        (BStr::new("version"), s(BStr::new("dev-main"))),
        (BStr::new("default-branch"), PhpMixed::Bool(true)),
    ]);

    let package = loader.load(config, None).unwrap();

    assert!(package.as_complete_alias_package().is_some());
    assert_eq!(
        VersionParser::DEFAULT_BRANCH_ALIAS,
        package.get_pretty_version()
    );

    // non-default branch gets no alias even if non-numeric
    let config = make_config(vec![
        (BStr::new("name"), s(BStr::new("A"))),
        (BStr::new("version"), s(BStr::new("dev-main"))),
        (BStr::new("default-branch"), PhpMixed::Bool(false)),
    ]);

    let package = loader.load(config, None).unwrap();

    assert!(package.as_complete_package().is_some());
    assert_eq!("dev-main", package.get_pretty_version());

    // default branch gets no alias if already numeric
    let config = make_config(vec![
        (BStr::new("name"), s(BStr::new("A"))),
        (BStr::new("version"), s(BStr::new("2.x-dev"))),
        (BStr::new("default-branch"), PhpMixed::Bool(true)),
    ]);

    let package = loader.load(config, None).unwrap();

    assert!(package.as_complete_package().is_some());
    assert_eq!("2.9999999.9999999.9999999-dev", package.get_version());

    // default branch gets no alias if already numeric, with v prefix
    let config = make_config(vec![
        (BStr::new("name"), s(BStr::new("A"))),
        (BStr::new("version"), s(BStr::new("v2.x-dev"))),
        (BStr::new("default-branch"), PhpMixed::Bool(true)),
    ]);

    let package = loader.load(config, None).unwrap();

    assert!(package.as_complete_package().is_some());
    assert_eq!("2.9999999.9999999.9999999-dev", package.get_version());
}

#[test]
fn test_abandoned() {
    let loader = set_up();
    let config = make_config(vec![
        (BStr::new("name"), s(BStr::new("A"))),
        (BStr::new("version"), s(BStr::new("1.2.3.4"))),
        (BStr::new("abandoned"), s(BStr::new("foo/bar"))),
    ]);

    let package = loader.load(config, None).unwrap();
    let package = package.as_complete().unwrap();
    assert!(package.is_abandoned());
    assert_eq!(Some("foo/bar".into()), package.get_replacement_package());
}

#[test]
fn test_not_abandoned() {
    let loader = set_up();
    let config = make_config(vec![
        (BStr::new("name"), s(BStr::new("A"))),
        (BStr::new("version"), s(BStr::new("1.2.3.4"))),
    ]);

    let package = loader.load(config, None).unwrap();
    let package = package.as_complete().unwrap();
    assert!(!package.is_abandoned());
}

/// ref: ArrayLoaderTest::providePluginApiVersions
fn provide_plugin_api_versions() -> Vec<&'static BStr> {
    vec![
        BStr::new("1.0"),
        BStr::new("1.0.0"),
        BStr::new("1.0.0.0"),
        BStr::new("1"),
        BStr::new("=1.0.0"),
        BStr::new("==1.0"),
        BStr::new("~1.0.0"),
        BStr::new("*"),
        BStr::new("3.0.*"),
        BStr::new("@stable"),
        BStr::new("1.0.0@stable"),
        BStr::new("^5.1"),
        BStr::new(">=1.0.0 <2.5"),
        BStr::new("x"),
        BStr::new("1.0.0-dev"),
    ]
}

#[test]
fn test_plugin_api_version_are_kept_as_declared() {
    let loader = set_up();
    for api_version in provide_plugin_api_versions() {
        let links = loader
            .parse_links(
                bstr::BStr::new("Plugin"),
                bstr::BStr::new("9.9.9"),
                Link::TYPE_REQUIRE,
                make_config(vec![(BStr::new("composer-plugin-api"), s(api_version))]),
            )
            .unwrap();

        assert!(links.contains_key(bstr::B("composer-plugin-api")));
        assert_eq!(
            api_version,
            links
                .get(bstr::B("composer-plugin-api"))
                .unwrap()
                .get_constraint()
                .get_pretty_string()
        );
    }
}

#[test]
fn test_plugin_api_version_does_support_self_version() {
    let loader = set_up();
    let links = loader
        .parse_links(
            bstr::BStr::new("Plugin"),
            bstr::BStr::new("6.6.6"),
            Link::TYPE_REQUIRE,
            make_config(vec![(
                BStr::new("composer-plugin-api"),
                s(BStr::new("self.version")),
            )]),
        )
        .unwrap();

    assert!(links.contains_key(bstr::B("composer-plugin-api")));
    assert_eq!(
        "6.6.6",
        links
            .get(bstr::B("composer-plugin-api"))
            .unwrap()
            .get_constraint()
            .get_pretty_string()
    );
}

#[test]
fn test_parse_links_integer_target() {
    let loader = set_up();
    let links = loader
        .parse_links(
            bstr::BStr::new("Plugin"),
            bstr::BStr::new("9.9.9"),
            Link::TYPE_REQUIRE,
            make_config(vec![(BStr::new("1"), s(BStr::new("dev-main")))]),
        )
        .unwrap();

    assert!(links.contains_key(bstr::B("1")));
}

#[test]
fn test_parse_links_invalid_version() {
    let loader = set_up();
    let err = loader
        .parse_links(
            bstr::BStr::new("Plugin"),
            bstr::BStr::new("9.9.9"),
            Link::TYPE_REQUIRE,
            make_config(vec![(
                BStr::new("composer-plugin-api"),
                s(BStr::new("^^^")),
            )]),
        )
        .unwrap_err();
    assert_eq!(
        "Link constraint in Plugin requires > composer-plugin-api should be a valid version constraint, got \"^^^\"",
        error_message(&err)
    );
}

#[test]
fn test_none_string_version() {
    let loader = set_up();
    let config = make_config(vec![
        (BStr::new("name"), s(BStr::new("acme/package"))),
        (BStr::new("version"), PhpMixed::Int(1)),
    ]);

    let package = loader.load(config, None).unwrap();
    assert_eq!("1", package.get_pretty_version());
}

#[test]
fn test_none_string_source_dist_reference() {
    let loader = set_up();
    let config = make_config(vec![
        (BStr::new("name"), s(BStr::new("acme/package"))),
        (BStr::new("version"), s(BStr::new("dev-main"))),
        (
            BStr::new("source"),
            map(vec![
                (BStr::new("type"), s(BStr::new("svn"))),
                (BStr::new("url"), s(BStr::new("https://example.org/"))),
                (BStr::new("reference"), PhpMixed::Int(2019)),
            ]),
        ),
        (
            BStr::new("dist"),
            map(vec![
                (BStr::new("type"), s(BStr::new("zip"))),
                (BStr::new("url"), s(BStr::new("https://example.org/"))),
                (BStr::new("reference"), PhpMixed::Int(2019)),
            ]),
        ),
    ]);

    let package = loader.load(config, None).unwrap();
    assert_eq!(Some("2019".into()), package.get_source_reference());
    assert_eq!(Some("2019".into()), package.get_dist_reference());
}

#[test]
fn test_branch_alias_integer_index() {
    let loader = set_up();
    let config = make_config(vec![
        (BStr::new("name"), s(BStr::new("acme/package"))),
        (BStr::new("version"), s(BStr::new("dev-1"))),
        (
            BStr::new("extra"),
            map(vec![(
                BStr::new("branch-alias"),
                map(vec![(BStr::new("1"), s(BStr::new("1.3-dev")))]),
            )]),
        ),
        (
            BStr::new("dist"),
            map(vec![
                (BStr::new("type"), s(BStr::new("zip"))),
                (BStr::new("url"), s(BStr::new("https://example.org/"))),
            ]),
        ),
    ]);

    assert_eq!(None, loader.get_branch_alias(&config).unwrap());
}

#[test]
fn test_package_links_require() {
    let loader = set_up();
    let config = make_config(vec![
        (BStr::new("name"), s(BStr::new("acme/package"))),
        (BStr::new("version"), s(BStr::new("dev-1"))),
        (
            BStr::new("require"),
            map(vec![(BStr::new("foo/bar"), s(BStr::new("1.0")))]),
        ),
    ]);

    let package = loader.load(config, None).unwrap();
    assert!(package.get_requires().contains_key(bstr::B("foo/bar")));
    assert_eq!(
        "1.0",
        package
            .get_requires()
            .get(bstr::B("foo/bar"))
            .unwrap()
            .get_constraint()
            .get_pretty_string()
    );
}

#[test]
fn test_package_links_require_invalid() {
    let loader = set_up();
    let config = make_config(vec![
        (BStr::new("name"), s(BStr::new("acme/package"))),
        (BStr::new("version"), s(BStr::new("dev-1"))),
        (
            BStr::new("require"),
            map(vec![(
                BStr::new("foo/bar"),
                map(vec![(BStr::new("random-string"), s(BStr::new("1.0")))]),
            )]),
        ),
    ]);

    let package = loader.load(config, None).unwrap();
    assert_eq!(0, package.get_requires().len());
}

#[test]
fn test_package_links_replace() {
    let loader = set_up();
    let config = make_config(vec![
        (BStr::new("name"), s(BStr::new("acme/package"))),
        (BStr::new("version"), s(BStr::new("dev-1"))),
        (
            BStr::new("replace"),
            map(vec![(
                BStr::new("coyote/package"),
                s(BStr::new("self.version")),
            )]),
        ),
    ]);

    let package = loader.load(config, None).unwrap();
    assert!(
        package
            .get_replaces()
            .contains_key(bstr::B("coyote/package"))
    );
    assert_eq!(
        "dev-1",
        package
            .get_replaces()
            .get(bstr::B("coyote/package"))
            .unwrap()
            .get_constraint()
            .get_pretty_string()
    );
}

#[test]
fn test_package_links_replace_invalid() {
    let loader = set_up();
    let config = make_config(vec![
        (BStr::new("name"), s(BStr::new("acme/package"))),
        (BStr::new("version"), s(BStr::new("dev-1"))),
        (BStr::new("replace"), s(BStr::new("coyote/package"))),
    ]);

    let package = loader.load(config, None).unwrap();
    assert_eq!(0, package.get_replaces().len());
}

#[test]
fn test_support_string_value() {
    let loader = set_up();
    let config = make_config(vec![
        (BStr::new("name"), s(BStr::new("acme/package"))),
        (BStr::new("version"), s(BStr::new("dev-1"))),
        (BStr::new("support"), s(BStr::new("https://example.org"))),
    ]);

    let package = loader.load(config, None).unwrap();
    let package = package.as_complete().unwrap();
    assert_eq!(0, package.get_support().len());
}

#[test]
fn test_invalid_version() {
    let loader = set_up();
    let config = make_config(vec![
        (BStr::new("name"), s(BStr::new("acme/package"))),
        (BStr::new("version"), s(BStr::new("AA"))),
    ]);

    let err = loader.load(config, None).unwrap_err();
    assert_eq!(
        "Failed to normalize version for package \"acme/package\": Invalid version string \"AA\"",
        error_message(&err)
    );
}
