//! ref: composer/tests/Composer/Test/Package/Loader/ValidatingArrayLoaderTest.php

use crate::test_case;
use indexmap::IndexMap;
use shirabe::package::handle::PackageInterfaceHandle;
use shirabe::package::loader::{InvalidPackageException, LoaderInterface, ValidatingArrayLoader};
use shirabe_php_shim::PhpMixed;

fn s(v: &str) -> PhpMixed {
    PhpMixed::String(v.to_string())
}

fn i(v: i64) -> PhpMixed {
    PhpMixed::Int(v)
}

fn b(v: bool) -> PhpMixed {
    PhpMixed::Bool(v)
}

/// Build a PHP assoc array (string-keyed). The impl reads every PHP `array`,
/// list or assoc, via `PhpMixed::as_array`, so both shapes are `PhpMixed::Array`.
fn arr(entries: Vec<(&str, PhpMixed)>) -> PhpMixed {
    let mut m = IndexMap::new();
    for (k, v) in entries {
        m.insert(k.to_string(), v);
    }
    PhpMixed::Array(m)
}

/// Build a PHP list array as a `PhpMixed::Array` with sequential string keys.
fn list(items: Vec<PhpMixed>) -> PhpMixed {
    let mut m = IndexMap::new();
    for (idx, v) in items.into_iter().enumerate() {
        m.insert(idx.to_string(), v);
    }
    PhpMixed::Array(m)
}

fn config(entries: Vec<(&str, PhpMixed)>) -> IndexMap<String, PhpMixed> {
    let mut m = IndexMap::new();
    for (k, v) in entries {
        m.insert(k.to_string(), v);
    }
    m
}

// PHP mocks `Composer\Package\Loader\LoaderInterface` with getMockBuilder.
mockall::mock! {
    #[derive(Debug)]
    Loader {}
    impl LoaderInterface for Loader {
        fn load(
            &self,
            config: IndexMap<String, PhpMixed>,
            class: Option<String>,
        ) -> anyhow::Result<PackageInterfaceHandle>;
        fn as_any(&self) -> &dyn std::any::Any;
    }
}

/// Build a mock inner loader whose `load` returns a dummy package, mirroring
/// PHPUnit's `getMockBuilder(LoaderInterface::class)->getMock()` with no
/// configured expectations.
fn mock_loader() -> MockLoader {
    let mut loader = MockLoader::new();
    loader
        .expect_load()
        .returning(|_, _| Ok(test_case::get_package("mock/mock", "1.0.0")));
    loader
}

fn invalid_naming_error(name: &str) -> Vec<String> {
    vec![format!(
        "name : {} is invalid, it should have a vendor name, a forward slash, and a package name. The vendor and package name can be words separated by -, . or _. The complete name should match \"^[a-z0-9]([_.-]?[a-z0-9]+)*/[a-z0-9](([_.]?|-{{0,2}})[a-z0-9]+)*$\".",
        name
    )]
}

/// ref: ValidatingArrayLoaderTest::successProvider
fn success_provider() -> Vec<IndexMap<String, PhpMixed>> {
    vec![
        // minimal
        config(vec![("name", s("foo/bar"))]),
        // complete
        config(vec![
            ("name", s("foo/bar")),
            ("description", s("Foo bar")),
            ("version", s("1.0.0")),
            ("type", s("library")),
            (
                "keywords",
                list(vec![s("a"), s("b_c"), s("D E"), s("éîüø"), s("微信")]),
            ),
            ("homepage", s("https://foo.com")),
            ("time", s("2010-10-10T10:10:10+00:00")),
            ("license", list(vec![s("MIT"), s("WTFPL")])),
            (
                "authors",
                list(vec![
                    arr(vec![
                        ("name", s("Alice")),
                        ("email", s("alice@example.org")),
                        ("role", s("Lead")),
                        ("homepage", s("http://example.org")),
                    ]),
                    arr(vec![("name", s("Bob")), ("homepage", s(""))]),
                ]),
            ),
            (
                "support",
                arr(vec![
                    ("email", s("mail@example.org")),
                    ("issues", s("http://example.org/")),
                    ("forum", s("http://example.org/")),
                    ("wiki", s("http://example.org/")),
                    ("source", s("http://example.org/")),
                    ("irc", s("irc://example.org/example")),
                    ("rss", s("http://example.org/rss")),
                    ("chat", s("http://example.org/chat")),
                    ("security", s("https://example.org/security")),
                ]),
            ),
            (
                "funding",
                list(vec![
                    arr(vec![
                        ("type", s("example")),
                        ("url", s("https://example.org/fund")),
                    ]),
                    arr(vec![("url", s("https://example.org/fund"))]),
                ]),
            ),
            (
                "require",
                arr(vec![
                    ("a/b", s("1.*")),
                    ("b/c", s("~2")),
                    ("example/pkg", s(">2.0-dev,<2.4-dev")),
                    ("composer-runtime-api", s("*")),
                ]),
            ),
            (
                "require-dev",
                arr(vec![
                    ("a/b", s("1.*")),
                    ("b/c", s("*")),
                    ("example/pkg", s(">2.0-dev,<2.4-dev")),
                ]),
            ),
            (
                "conflict",
                arr(vec![
                    ("a/bx", s("1.*")),
                    ("b/cx", s(">2.7")),
                    ("example/pkgx", s(">2.0-dev,<2.4-dev")),
                ]),
            ),
            (
                "replace",
                arr(vec![
                    ("a/b", s("1.*")),
                    ("example/pkg", s(">2.0-dev,<2.4-dev")),
                ]),
            ),
            (
                "provide",
                arr(vec![
                    ("a/b", s("1.*")),
                    ("example/pkg", s(">2.0-dev,<2.4-dev")),
                ]),
            ),
            (
                "suggest",
                arr(vec![("foo/bar", s("Foo bar is very useful"))]),
            ),
            (
                "autoload",
                arr(vec![
                    (
                        "psr-0",
                        arr(vec![("Foo\\Bar", s("src/")), ("", s("fallback/libs/"))]),
                    ),
                    ("classmap", list(vec![s("dir/"), s("dir2/file.php")])),
                    ("files", list(vec![s("functions.php")])),
                ]),
            ),
            ("include-path", list(vec![s("lib/")])),
            ("target-dir", s("Foo/Bar")),
            ("minimum-stability", s("dev")),
            (
                "repositories",
                list(vec![arr(vec![
                    ("type", s("composer")),
                    ("url", s("https://repo.packagist.org/")),
                ])]),
            ),
            (
                "config",
                arr(vec![
                    ("bin-dir", s("bin")),
                    ("vendor-dir", s("vendor")),
                    ("process-timeout", i(10000)),
                ]),
            ),
            (
                "archive",
                arr(vec![(
                    "exclude",
                    list(vec![s("/foo/bar"), s("baz"), s("!/foo/bar/baz")]),
                )]),
            ),
            (
                "scripts",
                arr(vec![
                    ("post-update-cmd", s("Foo\\Bar\\Baz::doSomething")),
                    (
                        "post-install-cmd",
                        list(vec![s("Foo\\Bar\\Baz::doSomething")]),
                    ),
                ]),
            ),
            (
                "extra",
                arr(vec![
                    (
                        "random",
                        arr(vec![("stuff", arr(vec![("deeply", s("nested"))]))]),
                    ),
                    (
                        "branch-alias",
                        arr(vec![
                            ("dev-master", s("2.0-dev")),
                            ("dev-old", s("1.0.x-dev")),
                            ("3.x-dev", s("3.1.x-dev")),
                        ]),
                    ),
                ]),
            ),
            ("bin", list(vec![s("bin/foo"), s("bin/bar")])),
            (
                "transport-options",
                arr(vec![(
                    "ssl",
                    arr(vec![("local_cert", s("/opt/certs/test.pem"))]),
                )]),
            ),
        ]),
        // test bin as string
        config(vec![("name", s("foo/bar")), ("bin", s("bin1"))]),
        // package name with dashes
        config(vec![("name", s("foo/bar-baz"))]),
        config(vec![("name", s("foo/bar--baz"))]),
        config(vec![("name", s("foo/b-ar--ba-z"))]),
        config(vec![("name", s("npm-asset/angular--core"))]),
        // refs as int or string
        config(vec![
            ("name", s("foo/bar")),
            (
                "source",
                arr(vec![
                    ("url", s("https://example.org")),
                    ("reference", i(1234)),
                    ("type", s("baz")),
                ]),
            ),
            (
                "dist",
                arr(vec![
                    ("url", s("https://example.org")),
                    ("reference", s("foobar")),
                    ("type", s("baz")),
                ]),
            ),
        ]),
        // valid php-ext configuration
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            (
                "php-ext",
                arr(vec![
                    ("extension-name", s("ext-xdebug")),
                    ("priority", i(80)),
                    ("support-zts", b(true)),
                    ("support-nts", b(false)),
                    ("build-path", s("my-extension-source")),
                    ("download-url-method", s("composer-default")),
                    ("os-families", list(vec![s("linux"), s("darwin")])),
                    (
                        "configure-options",
                        list(vec![
                            arr(vec![
                                ("name", s("enable-xdebug")),
                                ("needs-value", b(false)),
                                ("description", s("Enable xdebug support")),
                            ]),
                            arr(vec![
                                ("name", s("with-xdebug-path")),
                                ("needs-value", b(true)),
                            ]),
                        ]),
                    ),
                ]),
            ),
        ]),
        // valid php-ext with os-families-exclude
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext-zend")),
            (
                "php-ext",
                arr(vec![("os-families-exclude", list(vec![s("windows")]))]),
            ),
        ]),
        // valid php-ext with null build-path
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            ("php-ext", arr(vec![("build-path", PhpMixed::Null)])),
        ]),
        // valid php-ext with one download-url-method in a list
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            (
                "php-ext",
                arr(vec![(
                    "download-url-method",
                    list(vec![s("pre-packaged-binary")]),
                )]),
            ),
        ]),
        // valid php-ext with multiple download-url-methods
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            (
                "php-ext",
                arr(vec![(
                    "download-url-method",
                    list(vec![
                        s("pre-packaged-binary"),
                        s("pre-packaged-source"),
                        s("composer-default"),
                    ]),
                )]),
            ),
        ]),
    ]
}

/// ref: ValidatingArrayLoaderTest::testLoadSuccess
#[test]
fn test_load_success() {
    for cfg in success_provider() {
        let internal_loader = mock_loader();
        let mut loader = ValidatingArrayLoader::new(
            Box::new(internal_loader),
            true,
            None,
            ValidatingArrayLoader::CHECK_ALL,
        );
        loader
            .load(cfg, "Composer\\Package\\CompletePackage")
            .unwrap();
    }
}

/// ref: ValidatingArrayLoaderTest::errorProvider
fn error_provider() -> Vec<(IndexMap<String, PhpMixed>, Vec<String>)> {
    let mut data: Vec<(IndexMap<String, PhpMixed>, Vec<String>)> = Vec::new();

    for invalid_name in ["foo", "foo/-bar-", "foo/-bar"] {
        data.push((
            config(vec![("name", s(invalid_name))]),
            invalid_naming_error(invalid_name),
        ));
    }
    for invalid_name in [
        "fo--oo/bar",
        "fo-oo/bar__baz",
        "fo-oo/bar_.baz",
        "foo/bar---baz",
    ] {
        data.push((
            config(vec![("name", s(invalid_name))]),
            invalid_naming_error(invalid_name),
        ));
    }

    data.push((
        config(vec![("name", s("foo/bar")), ("homepage", i(43))]),
        vec!["homepage : should be a string, int given".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("support", arr(vec![("source", list(vec![]))])),
        ]),
        vec!["support.source : invalid value, must be a string".to_string()],
    ));
    data.push((
        config(vec![("name", s("foo/bar.json"))]),
        vec!["name : foo/bar.json is invalid, package names can not end in .json, consider renaming it or perhaps using a -json suffix instead.".to_string()],
    ));
    data.push((
        config(vec![("name", s("com1/foo"))]),
        vec!["name : com1/foo is reserved, package and vendor names can not match any of: nul, con, prn, aux, com1, com2, com3, com4, com5, com6, com7, com8, com9, lpt1, lpt2, lpt3, lpt4, lpt5, lpt6, lpt7, lpt8, lpt9.".to_string()],
    ));
    data.push((
        config(vec![("name", s("Foo/Bar"))]),
        vec!["name : Foo/Bar is invalid, it should not contain uppercase characters. We suggest using foo/bar instead.".to_string()],
    ));
    data.push((
        config(vec![("name", s("foo/bar")), ("autoload", s("strings"))]),
        vec!["autoload : should be an array, string given".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("autoload", arr(vec![("psr0", arr(vec![("foo", s("src"))]))])),
        ]),
        vec!["autoload : invalid value (psr0), must be one of psr-0, psr-4, classmap, files, exclude-from-classmap".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("transport-options", s("test")),
        ]),
        vec!["transport-options : should be an array, string given".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            (
                "source",
                arr(vec![
                    ("url", s("--foo")),
                    ("reference", s(" --bar")),
                    ("type", s("baz")),
                ]),
            ),
            (
                "dist",
                arr(vec![
                    ("url", s(" --foox")),
                    ("reference", s("--barx")),
                    ("type", s("baz")),
                ]),
            ),
        ]),
        vec![
            "dist.reference : must not start with a \"-\", \"--barx\" given".to_string(),
            "dist.url : must not start with a \"-\", \" --foox\" given".to_string(),
            "source.reference : must not start with a \"-\", \" --bar\" given".to_string(),
            "source.url : must not start with a \"-\", \"--foo\" given".to_string(),
        ],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("require", arr(vec![("foo/Bar", s("1.*"))])),
        ]),
        vec!["require.foo/Bar : a package cannot set a require on itself".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("source", arr(vec![("url", i(1))])),
            ("dist", arr(vec![("url", PhpMixed::Null)])),
        ]),
        vec![
            "source.type : must be present".to_string(),
            "source.url : should be a string, int given".to_string(),
            "source.reference : must be present".to_string(),
            "dist.type : must be present".to_string(),
            "dist.url : must be present".to_string(),
        ],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("replace", list(vec![s("acme/bar")])),
        ]),
        vec!["replace.0 : invalid version constraint (Could not parse version constraint acme/bar: Invalid version string \"acme/bar\")".to_string()],
    ));
    data.push((
        config(vec![("require", arr(vec![("acme/bar", s("^1.0"))]))]),
        vec!["name : must be present".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("library")),
            ("php-ext", arr(vec![("extension-name", s("ext-foobar"))])),
        ]),
        vec!["php-ext can only be set by packages of type \"php-ext\" or \"php-ext-zend\" which must be C extensions".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            ("php-ext", arr(vec![("extension-name", i(123))])),
        ]),
        vec!["php-ext.extension-name : should be a string, int given".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            ("php-ext", arr(vec![("priority", s("invalid"))])),
        ]),
        vec!["php-ext.priority : should be an integer, string given".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            ("php-ext", arr(vec![("support-zts", s("yes"))])),
        ]),
        vec!["php-ext.support-zts : should be a boolean, string given".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            ("php-ext", arr(vec![("support-nts", i(1))])),
        ]),
        vec!["php-ext.support-nts : should be a boolean, int given".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            ("php-ext", arr(vec![("build-path", i(123))])),
        ]),
        vec!["php-ext.build-path : should be a string or null, int given".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            ("php-ext", arr(vec![("download-url-method", i(123))])),
        ]),
        vec!["php-ext.download-url-method : should be an array or a string, int given".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            (
                "php-ext",
                arr(vec![("download-url-method", s("invalid-method"))]),
            ),
        ]),
        vec!["php-ext.download-url-method.0 : invalid value (invalid-method), must be one of composer-default, pre-packaged-source, pre-packaged-binary".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            ("php-ext", arr(vec![("download-url-method", list(vec![]))])),
        ]),
        vec!["php-ext.download-url-method : must contain at least one element".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            (
                "php-ext",
                arr(vec![(
                    "download-url-method",
                    list(vec![i(1), b(true), list(vec![])]),
                )]),
            ),
        ]),
        vec![
            "php-ext.download-url-method.0 : should be a string, int given".to_string(),
            "php-ext.download-url-method.1 : should be a string, bool given".to_string(),
            "php-ext.download-url-method.2 : should be a string, array given".to_string(),
        ],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            (
                "php-ext",
                arr(vec![(
                    "download-url-method",
                    list(vec![s("invalid-method"), s("composer-default")]),
                )]),
            ),
        ]),
        vec!["php-ext.download-url-method.0 : invalid value (invalid-method), must be one of composer-default, pre-packaged-source, pre-packaged-binary".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            (
                "php-ext",
                arr(vec![(
                    "download-url-method",
                    list(vec![s("invalid-method"), s("another-invalid-method")]),
                )]),
            ),
        ]),
        vec![
            "php-ext.download-url-method.0 : invalid value (invalid-method), must be one of composer-default, pre-packaged-source, pre-packaged-binary".to_string(),
            "php-ext.download-url-method.1 : invalid value (another-invalid-method), must be one of composer-default, pre-packaged-source, pre-packaged-binary".to_string(),
        ],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            (
                "php-ext",
                arr(vec![
                    ("os-families", list(vec![s("linux")])),
                    ("os-families-exclude", list(vec![s("windows")])),
                ]),
            ),
        ]),
        vec!["php-ext : os-families and os-families-exclude cannot both be specified".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            ("php-ext", arr(vec![("os-families", s("linux"))])),
        ]),
        vec!["php-ext.os-families : should be an array, string given".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            ("php-ext", arr(vec![("os-families", list(vec![]))])),
        ]),
        vec!["php-ext.os-families : must contain at least one element".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            (
                "php-ext",
                arr(vec![("os-families", list(vec![s("invalid-os"), s("linux")]))]),
            ),
        ]),
        vec!["php-ext.os-families.0 : invalid value (invalid-os), must be one of windows, bsd, darwin, solaris, linux, unknown".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            ("php-ext", arr(vec![("os-families", list(vec![i(123)]))])),
        ]),
        vec!["php-ext.os-families.0 : should be a string, int given".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            ("php-ext", arr(vec![("os-families-exclude", s("windows"))])),
        ]),
        vec!["php-ext.os-families-exclude : should be an array, string given".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            ("php-ext", arr(vec![("os-families-exclude", list(vec![]))])),
        ]),
        vec!["php-ext.os-families-exclude : must contain at least one element".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            (
                "php-ext",
                arr(vec![("os-families-exclude", list(vec![s("invalid")]))]),
            ),
        ]),
        vec!["php-ext.os-families-exclude.0 : invalid value (invalid), must be one of windows, bsd, darwin, solaris, linux, unknown".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            ("php-ext", arr(vec![("configure-options", s("invalid"))])),
        ]),
        vec!["php-ext.configure-options : should be an array, string given".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            (
                "php-ext",
                arr(vec![("configure-options", list(vec![s("invalid")]))]),
            ),
        ]),
        vec!["php-ext.configure-options.0 : should be an array, string given".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            (
                "php-ext",
                arr(vec![(
                    "configure-options",
                    list(vec![arr(vec![("description", s("test"))])]),
                )]),
            ),
        ]),
        vec!["php-ext.configure-options.0.name : must be present".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            (
                "php-ext",
                arr(vec![(
                    "configure-options",
                    list(vec![arr(vec![("name", i(123))])]),
                )]),
            ),
        ]),
        vec!["php-ext.configure-options.0.name : should be a string, int given".to_string()],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            (
                "php-ext",
                arr(vec![(
                    "configure-options",
                    list(vec![arr(vec![
                        ("name", s("valid-name")),
                        ("needs-value", s("yes")),
                    ])]),
                )]),
            ),
        ]),
        vec![
            "php-ext.configure-options.0.needs-value : should be a boolean, string given"
                .to_string(),
        ],
    ));
    data.push((
        config(vec![
            ("name", s("foo/bar")),
            ("type", s("php-ext")),
            (
                "php-ext",
                arr(vec![(
                    "configure-options",
                    list(vec![arr(vec![
                        ("name", s("valid-name")),
                        ("description", i(123)),
                    ])]),
                )]),
            ),
        ]),
        vec!["php-ext.configure-options.0.description : should be a string, int given".to_string()],
    ));

    data
}

/// ref: ValidatingArrayLoaderTest::testLoadFailureThrowsException
#[test]
fn test_load_failure_throws_exception() {
    for (cfg, mut expected_errors) in error_provider() {
        let internal_loader = mock_loader();
        let mut loader = ValidatingArrayLoader::new(
            Box::new(internal_loader),
            true,
            None,
            ValidatingArrayLoader::CHECK_ALL,
        );
        match loader.load(cfg, "Composer\\Package\\CompletePackage") {
            Ok(_) => panic!("Expected exception to be thrown"),
            Err(e) => {
                let exception = e
                    .downcast_ref::<InvalidPackageException>()
                    .expect("Expected InvalidPackageException");
                let mut errors: Vec<String> = exception.get_errors().to_vec();
                expected_errors.sort();
                errors.sort();
                assert_eq!(expected_errors, errors);
            }
        }
    }
}

/// ref: ValidatingArrayLoaderTest::warningProvider
/// Returns (config, expected_warnings, must_check, expected_array).
fn warning_provider() -> Vec<(
    IndexMap<String, PhpMixed>,
    Vec<String>,
    bool,
    Option<IndexMap<String, PhpMixed>>,
)> {
    vec![
        (
            config(vec![("name", s("foo/bar")), ("homepage", s("foo:bar"))]),
            vec!["homepage : invalid value (foo:bar), must be an http/https URL".to_string()],
            true,
            None,
        ),
        (
            config(vec![
                ("name", s("foo/bar")),
                (
                    "support",
                    arr(vec![
                        ("source", s("foo:bar")),
                        ("forum", s("foo:bar")),
                        ("issues", s("foo:bar")),
                        ("wiki", s("foo:bar")),
                        ("chat", s("foo:bar")),
                        ("security", s("foo:bar")),
                    ]),
                ),
            ]),
            vec![
                "support.source : invalid value (foo:bar), must be an http/https URL".to_string(),
                "support.forum : invalid value (foo:bar), must be an http/https URL".to_string(),
                "support.issues : invalid value (foo:bar), must be an http/https URL".to_string(),
                "support.wiki : invalid value (foo:bar), must be an http/https URL".to_string(),
                "support.chat : invalid value (foo:bar), must be an http/https URL".to_string(),
                "support.security : invalid value (foo:bar), must be an http/https URL".to_string(),
            ],
            true,
            None,
        ),
        (
            config(vec![
                ("name", s("foo/bar")),
                (
                    "require",
                    arr(vec![
                        ("foo/baz", s("*")),
                        ("bar/baz", s(">=1.0")),
                        ("bar/hacked", s("@stable")),
                        ("bar/woo", s("1.0.0")),
                    ]),
                ),
            ]),
            vec![
                "require.foo/baz : unbound version constraints (*) should be avoided".to_string(),
                "require.bar/baz : unbound version constraints (>=1.0) should be avoided"
                    .to_string(),
                "require.bar/hacked : unbound version constraints (@stable) should be avoided"
                    .to_string(),
                "require.bar/woo : exact version constraints (1.0.0) should be avoided if the package follows semantic versioning".to_string(),
            ],
            false,
            None,
        ),
        (
            config(vec![
                ("name", s("foo/bar")),
                (
                    "require",
                    arr(vec![
                        ("foo/baz", s(">1, <0.5")),
                        ("bar/baz", s("dev-main, >0.5")),
                    ]),
                ),
            ]),
            vec![
                "require.foo/baz : this version constraint cannot possibly match anything (>1, <0.5)".to_string(),
                "require.bar/baz : this version constraint cannot possibly match anything (dev-main, >0.5)".to_string(),
            ],
            false,
            None,
        ),
        (
            config(vec![
                ("name", s("foo/bar")),
                ("require", arr(vec![("bar/unstable", s("0.3.0"))])),
            ]),
            vec![],
            false,
            None,
        ),
        (
            config(vec![
                ("name", s("foo/bar")),
                (
                    "extra",
                    arr(vec![(
                        "branch-alias",
                        arr(vec![("5.x-dev", s("3.1.x-dev"))]),
                    )]),
                ),
            ]),
            vec!["extra.branch-alias.5.x-dev : the target branch (3.1.x-dev) is not a valid numeric alias for this version".to_string()],
            false,
            None,
        ),
        (
            config(vec![
                ("name", s("foo/bar")),
                (
                    "extra",
                    arr(vec![("branch-alias", arr(vec![("5.x-dev", s("3.1-dev"))]))]),
                ),
            ]),
            vec!["extra.branch-alias.5.x-dev : the target branch (3.1-dev) is not a valid numeric alias for this version".to_string()],
            false,
            None,
        ),
        (
            config(vec![
                ("name", s("foo/bar")),
                ("require", arr(vec![("Foo/Baz", s("^1.0"))])),
            ]),
            vec!["require.Foo/Baz is invalid, it should not contain uppercase characters. Please use foo/baz instead.".to_string()],
            false,
            None,
        ),
        (
            config(vec![("name", s("a/b")), ("license", s("XXXXX"))]),
            vec![format!(
                "License \"XXXXX\" is not a valid SPDX license identifier, see https://spdx.org/licenses/ if you use an open license.{}If the software is closed-source, you may use \"proprietary\" as license.",
                shirabe_php_shim::PHP_EOL
            )],
            true,
            Some(config(vec![
                ("name", s("a/b")),
                ("license", list(vec![s("XXXXX")])),
            ])),
        ),
        (
            config(vec![
                ("name", s("a/b")),
                ("license", list(vec![arr(vec![("author", s("bar"))]), s("MIT")])),
            ]),
            vec!["License {\"author\":\"bar\"} should be a string.".to_string()],
            true,
            Some(config(vec![
                ("name", s("a/b")),
                ("license", list(vec![s("MIT")])),
            ])),
        ),
    ]
}

/// ref: ValidatingArrayLoaderTest::testLoadWarnings
#[ignore = "license warning cases need the SPDX license-expression grammar (recursive PCRE), not yet ported: spdx_licenses todo!()"]
#[test]
fn test_load_warnings() {
    for (cfg, mut expected_warnings, _must_check, _expected_array) in warning_provider() {
        let internal_loader = mock_loader();
        let mut loader = ValidatingArrayLoader::new(
            Box::new(internal_loader),
            true,
            None,
            ValidatingArrayLoader::CHECK_ALL,
        );
        loader
            .load(cfg, "Composer\\Package\\CompletePackage")
            .unwrap();
        let mut warnings: Vec<String> = loader.get_warnings().to_vec();
        expected_warnings.sort();
        warnings.sort();
        assert_eq!(expected_warnings, warnings);
    }
}

/// ref: ValidatingArrayLoaderTest::testLoadSkipsWarningDataWhenIgnoringErrors
#[ignore = "must_check license cases need the SPDX license-expression grammar (recursive PCRE), not yet ported: spdx_licenses todo!()"]
#[test]
fn test_load_skips_warning_data_when_ignoring_errors() {
    for (mut cfg, _expected_warnings, must_check, expected_array) in warning_provider() {
        if !must_check {
            continue;
        }
        let expected = expected_array.unwrap_or_else(|| config(vec![("name", s("a/b"))]));

        // The inner loader is called exactly once with the (post-validation) config.
        let mut internal_loader = MockLoader::new();
        internal_loader
            .expect_load()
            .times(1)
            .withf(move |cfg, _class| *cfg == expected)
            .returning(|_, _| Ok(test_case::get_package("mock/mock", "1.0.0")));

        let mut loader = ValidatingArrayLoader::new(
            Box::new(internal_loader),
            true,
            None,
            ValidatingArrayLoader::CHECK_ALL,
        );
        cfg.insert("name".to_string(), s("a/b"));
        loader
            .load(cfg, "Composer\\Package\\CompletePackage")
            .unwrap();
    }
}
