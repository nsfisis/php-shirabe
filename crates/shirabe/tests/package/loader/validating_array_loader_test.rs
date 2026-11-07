//! ref: composer/tests/Composer/Test/Package/Loader/ValidatingArrayLoaderTest.php sha1:c27941010b8febd9d121a330b08dd1ce19ddb3d0

use crate::test_case;
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe::package::PackageInterfaceHandle;
use shirabe::package::loader::{InvalidPackageException, LoaderInterface, ValidatingArrayLoader};
use shirabe_php_shim::{Catch as _, PHP_EOL, PhpMixed, bconcat};

fn s(v: &BStr) -> PhpMixed {
    PhpMixed::string(bconcat!(v))
}

fn i(v: i64) -> PhpMixed {
    PhpMixed::Int(v)
}

fn b(v: bool) -> PhpMixed {
    PhpMixed::Bool(v)
}

/// Build a PHP assoc array (string-keyed). The impl reads every PHP `array`,
/// list or assoc, via `PhpMixed::as_array`, so both shapes are `PhpMixed::Array`.
fn arr(entries: Vec<(&BStr, PhpMixed)>) -> PhpMixed {
    let mut m = IndexMap::new();
    for (k, v) in entries {
        m.insert(k.into(), v);
    }
    PhpMixed::Array(m)
}

/// Build a PHP list array as a `PhpMixed::Array` with sequential string keys.
fn list(items: Vec<PhpMixed>) -> PhpMixed {
    let mut m = IndexMap::new();
    for (idx, v) in items.into_iter().enumerate() {
        m.insert(bconcat!(idx), v);
    }
    PhpMixed::Array(m)
}

fn config(entries: Vec<(&BStr, PhpMixed)>) -> IndexMap<BString, PhpMixed> {
    let mut m = IndexMap::new();
    for (k, v) in entries {
        m.insert(k.into(), v);
    }
    m
}

// PHP mocks `Composer\Package\Loader\LoaderInterface` with getMockBuilder.
#[expect(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    reason = "the code mockall generates"
)]
mod mock_loader {
    use super::*;

    mockall::mock! {
        #[derive(Debug)]
        pub Loader {}
        impl LoaderInterface for Loader {
            fn load(
                &self,
                config: IndexMap<BString, PhpMixed>,
                class: Option<BString>,
            ) -> anyhow::Result<PackageInterfaceHandle>;
            fn as_any(&self) -> &dyn std::any::Any;
        }
    }
}
use mock_loader::MockLoader;

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

fn invalid_naming_error(name: &BStr) -> Vec<BString> {
    vec![bconcat!(
        "name : ",
        name,
        " is invalid, it should have a vendor name, a forward slash, and a package name. The vendor and package name can be words separated by -, . or _. The complete name should match \"^[a-z0-9]([_.-]?[a-z0-9]+)*/[a-z0-9](([_.]?|-{0,2})[a-z0-9]+)*$\"."
    )]
}

/// ref: ValidatingArrayLoaderTest::successProvider
fn success_provider() -> Vec<IndexMap<BString, PhpMixed>> {
    vec![
        // minimal
        config(vec![(BStr::new("name"), s(BStr::new("foo/bar")))]),
        // complete
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("description"), s(BStr::new("Foo bar"))),
            (BStr::new("version"), s(BStr::new("1.0.0"))),
            (BStr::new("type"), s(BStr::new("library"))),
            (
                BStr::new("keywords"),
                list(vec![
                    s(BStr::new("a")),
                    s(BStr::new("b_c")),
                    s(BStr::new("D E")),
                    s(BStr::new("éîüø")),
                    s(BStr::new("微信")),
                ]),
            ),
            (BStr::new("homepage"), s(BStr::new("https://foo.com"))),
            (BStr::new("time"), s(BStr::new("2010-10-10T10:10:10+00:00"))),
            (
                BStr::new("license"),
                list(vec![s(BStr::new("MIT")), s(BStr::new("WTFPL"))]),
            ),
            (
                BStr::new("authors"),
                list(vec![
                    arr(vec![
                        (BStr::new("name"), s(BStr::new("Alice"))),
                        (BStr::new("email"), s(BStr::new("alice@example.org"))),
                        (BStr::new("role"), s(BStr::new("Lead"))),
                        (BStr::new("homepage"), s(BStr::new("http://example.org"))),
                    ]),
                    arr(vec![
                        (BStr::new("name"), s(BStr::new("Bob"))),
                        (BStr::new("homepage"), s(BStr::new(""))),
                    ]),
                ]),
            ),
            (
                BStr::new("support"),
                arr(vec![
                    (BStr::new("email"), s(BStr::new("mail@example.org"))),
                    (BStr::new("issues"), s(BStr::new("http://example.org/"))),
                    (BStr::new("forum"), s(BStr::new("http://example.org/"))),
                    (BStr::new("wiki"), s(BStr::new("http://example.org/"))),
                    (BStr::new("source"), s(BStr::new("http://example.org/"))),
                    (BStr::new("irc"), s(BStr::new("irc://example.org/example"))),
                    (BStr::new("rss"), s(BStr::new("http://example.org/rss"))),
                    (BStr::new("chat"), s(BStr::new("http://example.org/chat"))),
                    (
                        BStr::new("security"),
                        s(BStr::new("https://example.org/security")),
                    ),
                ]),
            ),
            (
                BStr::new("funding"),
                list(vec![
                    arr(vec![
                        (BStr::new("type"), s(BStr::new("example"))),
                        (BStr::new("url"), s(BStr::new("https://example.org/fund"))),
                    ]),
                    arr(vec![(
                        BStr::new("url"),
                        s(BStr::new("https://example.org/fund")),
                    )]),
                ]),
            ),
            (
                BStr::new("require"),
                arr(vec![
                    (BStr::new("a/b"), s(BStr::new("1.*"))),
                    (BStr::new("b/c"), s(BStr::new("~2"))),
                    (BStr::new("example/pkg"), s(BStr::new(">2.0-dev,<2.4-dev"))),
                    (BStr::new("composer-runtime-api"), s(BStr::new("*"))),
                ]),
            ),
            (
                BStr::new("require-dev"),
                arr(vec![
                    (BStr::new("a/b"), s(BStr::new("1.*"))),
                    (BStr::new("b/c"), s(BStr::new("*"))),
                    (BStr::new("example/pkg"), s(BStr::new(">2.0-dev,<2.4-dev"))),
                ]),
            ),
            (
                BStr::new("conflict"),
                arr(vec![
                    (BStr::new("a/bx"), s(BStr::new("1.*"))),
                    (BStr::new("b/cx"), s(BStr::new(">2.7"))),
                    (BStr::new("example/pkgx"), s(BStr::new(">2.0-dev,<2.4-dev"))),
                ]),
            ),
            (
                BStr::new("replace"),
                arr(vec![
                    (BStr::new("a/b"), s(BStr::new("1.*"))),
                    (BStr::new("example/pkg"), s(BStr::new(">2.0-dev,<2.4-dev"))),
                ]),
            ),
            (
                BStr::new("provide"),
                arr(vec![
                    (BStr::new("a/b"), s(BStr::new("1.*"))),
                    (BStr::new("example/pkg"), s(BStr::new(">2.0-dev,<2.4-dev"))),
                ]),
            ),
            (
                BStr::new("suggest"),
                arr(vec![(
                    BStr::new("foo/bar"),
                    s(BStr::new("Foo bar is very useful")),
                )]),
            ),
            (
                BStr::new("autoload"),
                arr(vec![
                    (
                        BStr::new("psr-0"),
                        arr(vec![
                            (BStr::new("Foo\\Bar"), s(BStr::new("src/"))),
                            (BStr::new(""), s(BStr::new("fallback/libs/"))),
                        ]),
                    ),
                    (
                        BStr::new("classmap"),
                        list(vec![s(BStr::new("dir/")), s(BStr::new("dir2/file.php"))]),
                    ),
                    (
                        BStr::new("files"),
                        list(vec![s(BStr::new("functions.php"))]),
                    ),
                ]),
            ),
            (BStr::new("include-path"), list(vec![s(BStr::new("lib/"))])),
            (BStr::new("target-dir"), s(BStr::new("Foo/Bar"))),
            (BStr::new("minimum-stability"), s(BStr::new("dev"))),
            (
                BStr::new("repositories"),
                list(vec![arr(vec![
                    (BStr::new("type"), s(BStr::new("composer"))),
                    (
                        BStr::new("url"),
                        s(BStr::new("https://repo.packagist.org/")),
                    ),
                ])]),
            ),
            (
                BStr::new("config"),
                arr(vec![
                    (BStr::new("bin-dir"), s(BStr::new("bin"))),
                    (BStr::new("vendor-dir"), s(BStr::new("vendor"))),
                    (BStr::new("process-timeout"), i(10000)),
                ]),
            ),
            (
                BStr::new("archive"),
                arr(vec![(
                    BStr::new("exclude"),
                    list(vec![
                        s(BStr::new("/foo/bar")),
                        s(BStr::new("baz")),
                        s(BStr::new("!/foo/bar/baz")),
                    ]),
                )]),
            ),
            (
                BStr::new("scripts"),
                arr(vec![
                    (
                        BStr::new("post-update-cmd"),
                        s(BStr::new("Foo\\Bar\\Baz::doSomething")),
                    ),
                    (
                        BStr::new("post-install-cmd"),
                        list(vec![s(BStr::new("Foo\\Bar\\Baz::doSomething"))]),
                    ),
                ]),
            ),
            (
                BStr::new("extra"),
                arr(vec![
                    (
                        BStr::new("random"),
                        arr(vec![(
                            BStr::new("stuff"),
                            arr(vec![(BStr::new("deeply"), s(BStr::new("nested")))]),
                        )]),
                    ),
                    (
                        BStr::new("branch-alias"),
                        arr(vec![
                            (BStr::new("dev-master"), s(BStr::new("2.0-dev"))),
                            (BStr::new("dev-old"), s(BStr::new("1.0.x-dev"))),
                            (BStr::new("3.x-dev"), s(BStr::new("3.1.x-dev"))),
                        ]),
                    ),
                ]),
            ),
            (
                BStr::new("bin"),
                list(vec![s(BStr::new("bin/foo")), s(BStr::new("bin/bar"))]),
            ),
            (
                BStr::new("transport-options"),
                arr(vec![(
                    BStr::new("ssl"),
                    arr(vec![(
                        BStr::new("local_cert"),
                        s(BStr::new("/opt/certs/test.pem")),
                    )]),
                )]),
            ),
        ]),
        // test bin as string
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("bin"), s(BStr::new("bin1"))),
        ]),
        // package name with dashes
        config(vec![(BStr::new("name"), s(BStr::new("foo/bar-baz")))]),
        config(vec![(BStr::new("name"), s(BStr::new("foo/bar--baz")))]),
        config(vec![(BStr::new("name"), s(BStr::new("foo/b-ar--ba-z")))]),
        config(vec![(
            BStr::new("name"),
            s(BStr::new("npm-asset/angular--core")),
        )]),
        // refs as int or string
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (
                BStr::new("source"),
                arr(vec![
                    (BStr::new("url"), s(BStr::new("https://example.org"))),
                    (BStr::new("reference"), i(1234)),
                    (BStr::new("type"), s(BStr::new("baz"))),
                ]),
            ),
            (
                BStr::new("dist"),
                arr(vec![
                    (BStr::new("url"), s(BStr::new("https://example.org"))),
                    (BStr::new("reference"), s(BStr::new("foobar"))),
                    (BStr::new("type"), s(BStr::new("baz"))),
                ]),
            ),
        ]),
        // valid php-ext configuration
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![
                    (BStr::new("extension-name"), s(BStr::new("ext-xdebug"))),
                    (BStr::new("priority"), i(80)),
                    (BStr::new("support-zts"), b(true)),
                    (BStr::new("support-nts"), b(false)),
                    (BStr::new("build-path"), s(BStr::new("my-extension-source"))),
                    (
                        BStr::new("download-url-method"),
                        s(BStr::new("composer-default")),
                    ),
                    (
                        BStr::new("os-families"),
                        list(vec![s(BStr::new("linux")), s(BStr::new("darwin"))]),
                    ),
                    (
                        BStr::new("configure-options"),
                        list(vec![
                            arr(vec![
                                (BStr::new("name"), s(BStr::new("enable-xdebug"))),
                                (BStr::new("needs-value"), b(false)),
                                (
                                    BStr::new("description"),
                                    s(BStr::new("Enable xdebug support")),
                                ),
                            ]),
                            arr(vec![
                                (BStr::new("name"), s(BStr::new("with-xdebug-path"))),
                                (BStr::new("needs-value"), b(true)),
                            ]),
                        ]),
                    ),
                ]),
            ),
        ]),
        // valid php-ext with os-families-exclude
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext-zend"))),
            (
                BStr::new("php-ext"),
                arr(vec![(
                    BStr::new("os-families-exclude"),
                    list(vec![s(BStr::new("windows"))]),
                )]),
            ),
        ]),
        // valid php-ext with null build-path
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![(BStr::new("build-path"), PhpMixed::Null)]),
            ),
        ]),
        // valid php-ext with one download-url-method in a list
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![(
                    BStr::new("download-url-method"),
                    list(vec![s(BStr::new("pre-packaged-binary"))]),
                )]),
            ),
        ]),
        // valid php-ext with multiple download-url-methods
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![(
                    BStr::new("download-url-method"),
                    list(vec![
                        s(BStr::new("pre-packaged-binary")),
                        s(BStr::new("pre-packaged-source")),
                        s(BStr::new("composer-default")),
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
        let loader = ValidatingArrayLoader::new(
            Box::new(internal_loader),
            true,
            None,
            ValidatingArrayLoader::CHECK_ALL,
        );
        loader
            .load(
                cfg,
                Some(BString::from("Composer\\Package\\CompletePackage")),
            )
            .unwrap();
    }
}

/// ref: ValidatingArrayLoaderTest::errorProvider
fn error_provider() -> Vec<(IndexMap<BString, PhpMixed>, Vec<BString>)> {
    let mut data: Vec<(IndexMap<BString, PhpMixed>, Vec<BString>)> = Vec::new();

    for invalid_name in [
        BStr::new("foo"),
        BStr::new("foo/-bar-"),
        BStr::new("foo/-bar"),
    ] {
        data.push((
            config(vec![(BStr::new("name"), s(invalid_name))]),
            invalid_naming_error(invalid_name),
        ));
    }
    for invalid_name in [
        BStr::new("fo--oo/bar"),
        BStr::new("fo-oo/bar__baz"),
        BStr::new("fo-oo/bar_.baz"),
        BStr::new("foo/bar---baz"),
    ] {
        data.push((
            config(vec![(BStr::new("name"), s(invalid_name))]),
            invalid_naming_error(invalid_name),
        ));
    }

    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("homepage"), i(43)),
        ]),
        vec![BString::from("homepage : should be a string, int given")],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (
                BStr::new("support"),
                arr(vec![(BStr::new("source"), list(vec![]))]),
            ),
        ]),
        vec![BString::from(
            "support.source : invalid value, must be a string",
        )],
    ));
    data.push((
        config(vec![(BStr::new("name"), s(BStr::new("foo/bar.json")))]),
        vec![BString::from("name : foo/bar.json is invalid, package names can not end in .json, consider renaming it or perhaps using a -json suffix instead.")],
    ));
    data.push((
        config(vec![(BStr::new("name"), s(BStr::new("com1/foo")))]),
        vec![BString::from("name : com1/foo is reserved, package and vendor names can not match any of: nul, con, prn, aux, com1, com2, com3, com4, com5, com6, com7, com8, com9, lpt1, lpt2, lpt3, lpt4, lpt5, lpt6, lpt7, lpt8, lpt9.")],
    ));
    data.push((
        config(vec![(BStr::new("name"), s(BStr::new("Foo/Bar")))]),
        vec![BString::from("name : Foo/Bar is invalid, it should not contain uppercase characters. We suggest using foo/bar instead.")],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("autoload"), s(BStr::new("strings"))),
        ]),
        vec![BString::from("autoload : should be an array, string given")],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("autoload"), arr(vec![(BStr::new("psr0"), arr(vec![(BStr::new("foo"), s(BStr::new("src")))]))])),
        ]),
        vec![BString::from("autoload : invalid value (psr0), must be one of psr-0, psr-4, classmap, files, exclude-from-classmap")],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("transport-options"), s(BStr::new("test"))),
        ]),
        vec![BString::from(
            "transport-options : should be an array, string given",
        )],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (
                BStr::new("source"),
                arr(vec![
                    (BStr::new("url"), s(BStr::new("--foo"))),
                    (BStr::new("reference"), s(BStr::new(" --bar"))),
                    (BStr::new("type"), s(BStr::new("baz"))),
                ]),
            ),
            (
                BStr::new("dist"),
                arr(vec![
                    (BStr::new("url"), s(BStr::new(" --foox"))),
                    (BStr::new("reference"), s(BStr::new("--barx"))),
                    (BStr::new("type"), s(BStr::new("baz"))),
                ]),
            ),
        ]),
        vec![
            BString::from("dist.reference : must not start with a \"-\", \"--barx\" given"),
            BString::from("dist.url : must not start with a \"-\", \" --foox\" given"),
            BString::from("source.reference : must not start with a \"-\", \" --bar\" given"),
            BString::from("source.url : must not start with a \"-\", \"--foo\" given"),
        ],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (
                BStr::new("require"),
                arr(vec![(BStr::new("foo/Bar"), s(BStr::new("1.*")))]),
            ),
        ]),
        vec![BString::from(
            "require.foo/Bar : a package cannot set a require on itself",
        )],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("source"), arr(vec![(BStr::new("url"), i(1))])),
            (
                BStr::new("dist"),
                arr(vec![(BStr::new("url"), PhpMixed::Null)]),
            ),
        ]),
        vec![
            BString::from("source.type : must be present"),
            BString::from("source.url : should be a string, int given"),
            BString::from("source.reference : must be present"),
            BString::from("dist.type : must be present"),
            BString::from("dist.url : must be present"),
        ],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("replace"), list(vec![s(BStr::new("acme/bar"))])),
        ]),
        vec![BString::from("replace.0 : invalid version constraint (Could not parse version constraint acme/bar: Invalid version string \"acme/bar\")")],
    ));
    data.push((
        config(vec![(
            BStr::new("require"),
            arr(vec![(BStr::new("acme/bar"), s(BStr::new("^1.0")))]),
        )]),
        vec![BString::from("name : must be present")],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("library"))),
            (BStr::new("php-ext"), arr(vec![(BStr::new("extension-name"), s(BStr::new("ext-foobar")))])),
        ]),
        vec![BString::from("php-ext can only be set by packages of type \"php-ext\" or \"php-ext-zend\" which must be C extensions")],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![(BStr::new("extension-name"), i(123))]),
            ),
        ]),
        vec![BString::from(
            "php-ext.extension-name : should be a string, int given",
        )],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![(BStr::new("priority"), s(BStr::new("invalid")))]),
            ),
        ]),
        vec![BString::from(
            "php-ext.priority : should be an integer, string given",
        )],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![(BStr::new("support-zts"), s(BStr::new("yes")))]),
            ),
        ]),
        vec![BString::from(
            "php-ext.support-zts : should be a boolean, string given",
        )],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![(BStr::new("support-nts"), i(1))]),
            ),
        ]),
        vec![BString::from(
            "php-ext.support-nts : should be a boolean, int given",
        )],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![(BStr::new("build-path"), i(123))]),
            ),
        ]),
        vec![BString::from(
            "php-ext.build-path : should be a string or null, int given",
        )],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![(BStr::new("download-url-method"), i(123))]),
            ),
        ]),
        vec![BString::from(
            "php-ext.download-url-method : should be an array or a string, int given",
        )],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![(BStr::new("download-url-method"), s(BStr::new("invalid-method")))]),
            ),
        ]),
        vec![BString::from("php-ext.download-url-method.0 : invalid value (invalid-method), must be one of composer-default, pre-packaged-source, pre-packaged-binary")],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![(BStr::new("download-url-method"), list(vec![]))]),
            ),
        ]),
        vec![BString::from(
            "php-ext.download-url-method : must contain at least one element",
        )],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![(
                    BStr::new("download-url-method"),
                    list(vec![i(1), b(true), list(vec![])]),
                )]),
            ),
        ]),
        vec![
            BString::from("php-ext.download-url-method.0 : should be a string, int given"),
            BString::from("php-ext.download-url-method.1 : should be a string, bool given"),
            BString::from("php-ext.download-url-method.2 : should be a string, array given"),
        ],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![(
                    BStr::new("download-url-method"),
                    list(vec![s(BStr::new("invalid-method")), s(BStr::new("composer-default"))]),
                )]),
            ),
        ]),
        vec![BString::from("php-ext.download-url-method.0 : invalid value (invalid-method), must be one of composer-default, pre-packaged-source, pre-packaged-binary")],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![(
                    BStr::new("download-url-method"),
                    list(vec![s(BStr::new("invalid-method")), s(BStr::new("another-invalid-method"))]),
                )]),
            ),
        ]),
        vec![
            BString::from("php-ext.download-url-method.0 : invalid value (invalid-method), must be one of composer-default, pre-packaged-source, pre-packaged-binary"),
            BString::from("php-ext.download-url-method.1 : invalid value (another-invalid-method), must be one of composer-default, pre-packaged-source, pre-packaged-binary"),
        ],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![
                    (BStr::new("os-families"), list(vec![s(BStr::new("linux"))])),
                    (
                        BStr::new("os-families-exclude"),
                        list(vec![s(BStr::new("windows"))]),
                    ),
                ]),
            ),
        ]),
        vec![BString::from(
            "php-ext : os-families and os-families-exclude cannot both be specified",
        )],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![(BStr::new("os-families"), s(BStr::new("linux")))]),
            ),
        ]),
        vec![BString::from(
            "php-ext.os-families : should be an array, string given",
        )],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![(BStr::new("os-families"), list(vec![]))]),
            ),
        ]),
        vec![BString::from(
            "php-ext.os-families : must contain at least one element",
        )],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![(BStr::new("os-families"), list(vec![s(BStr::new("invalid-os")), s(BStr::new("linux"))]))]),
            ),
        ]),
        vec![BString::from("php-ext.os-families.0 : invalid value (invalid-os), must be one of windows, bsd, darwin, solaris, linux, unknown")],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![(BStr::new("os-families"), list(vec![i(123)]))]),
            ),
        ]),
        vec![BString::from(
            "php-ext.os-families.0 : should be a string, int given",
        )],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![(
                    BStr::new("os-families-exclude"),
                    s(BStr::new("windows")),
                )]),
            ),
        ]),
        vec![BString::from(
            "php-ext.os-families-exclude : should be an array, string given",
        )],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![(BStr::new("os-families-exclude"), list(vec![]))]),
            ),
        ]),
        vec![BString::from(
            "php-ext.os-families-exclude : must contain at least one element",
        )],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![(BStr::new("os-families-exclude"), list(vec![s(BStr::new("invalid"))]))]),
            ),
        ]),
        vec![BString::from("php-ext.os-families-exclude.0 : invalid value (invalid), must be one of windows, bsd, darwin, solaris, linux, unknown")],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![(
                    BStr::new("configure-options"),
                    s(BStr::new("invalid")),
                )]),
            ),
        ]),
        vec![BString::from(
            "php-ext.configure-options : should be an array, string given",
        )],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![(
                    BStr::new("configure-options"),
                    list(vec![s(BStr::new("invalid"))]),
                )]),
            ),
        ]),
        vec![BString::from(
            "php-ext.configure-options.0 : should be an array, string given",
        )],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![(
                    BStr::new("configure-options"),
                    list(vec![arr(vec![(
                        BStr::new("description"),
                        s(BStr::new("test")),
                    )])]),
                )]),
            ),
        ]),
        vec![BString::from(
            "php-ext.configure-options.0.name : must be present",
        )],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![(
                    BStr::new("configure-options"),
                    list(vec![arr(vec![(BStr::new("name"), i(123))])]),
                )]),
            ),
        ]),
        vec![BString::from(
            "php-ext.configure-options.0.name : should be a string, int given",
        )],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![(
                    BStr::new("configure-options"),
                    list(vec![arr(vec![
                        (BStr::new("name"), s(BStr::new("valid-name"))),
                        (BStr::new("needs-value"), s(BStr::new("yes"))),
                    ])]),
                )]),
            ),
        ]),
        vec![BString::from(
            "php-ext.configure-options.0.needs-value : should be a boolean, string given",
        )],
    ));
    data.push((
        config(vec![
            (BStr::new("name"), s(BStr::new("foo/bar"))),
            (BStr::new("type"), s(BStr::new("php-ext"))),
            (
                BStr::new("php-ext"),
                arr(vec![(
                    BStr::new("configure-options"),
                    list(vec![arr(vec![
                        (BStr::new("name"), s(BStr::new("valid-name"))),
                        (BStr::new("description"), i(123)),
                    ])]),
                )]),
            ),
        ]),
        vec![BString::from(
            "php-ext.configure-options.0.description : should be a string, int given",
        )],
    ));

    data
}

/// ref: ValidatingArrayLoaderTest::testLoadFailureThrowsException
#[test]
fn test_load_failure_throws_exception() {
    for (cfg, expected_errors) in error_provider() {
        let mut expected_errors: Vec<BString> = expected_errors.into_iter().collect();
        let internal_loader = mock_loader();
        let loader = ValidatingArrayLoader::new(
            Box::new(internal_loader),
            true,
            None,
            ValidatingArrayLoader::CHECK_ALL,
        );
        match loader.load(
            cfg,
            Some(BString::from("Composer\\Package\\CompletePackage")),
        ) {
            Ok(_) => panic!("Expected exception to be thrown"),
            Err(e) => {
                let exception = e
                    .catch::<InvalidPackageException>()
                    .expect("Expected InvalidPackageException");
                let mut errors: Vec<BString> = exception.get_errors().to_vec();
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
    IndexMap<BString, PhpMixed>,
    Vec<BString>,
    bool,
    Option<IndexMap<BString, PhpMixed>>,
)> {
    vec![
        (
            config(vec![(BStr::new("name"), s(BStr::new("foo/bar"))), (BStr::new("homepage"), s(BStr::new("foo:bar")))]),
            vec![BString::from("homepage : invalid value (foo:bar), must be an http/https URL")],
            true,
            None,
        ),
        (
            config(vec![
                (BStr::new("name"), s(BStr::new("foo/bar"))),
                (
                    BStr::new("support"),
                    arr(vec![
                        (BStr::new("source"), s(BStr::new("foo:bar"))),
                        (BStr::new("forum"), s(BStr::new("foo:bar"))),
                        (BStr::new("issues"), s(BStr::new("foo:bar"))),
                        (BStr::new("wiki"), s(BStr::new("foo:bar"))),
                        (BStr::new("chat"), s(BStr::new("foo:bar"))),
                        (BStr::new("security"), s(BStr::new("foo:bar"))),
                    ]),
                ),
            ]),
            vec![
                BString::from("support.source : invalid value (foo:bar), must be an http/https URL"),
                BString::from("support.forum : invalid value (foo:bar), must be an http/https URL"),
                BString::from("support.issues : invalid value (foo:bar), must be an http/https URL"),
                BString::from("support.wiki : invalid value (foo:bar), must be an http/https URL"),
                BString::from("support.chat : invalid value (foo:bar), must be an http/https URL"),
                BString::from("support.security : invalid value (foo:bar), must be an http/https URL"),
            ],
            true,
            None,
        ),
        (
            config(vec![
                (BStr::new("name"), s(BStr::new("foo/bar"))),
                (
                    BStr::new("require"),
                    arr(vec![
                        (BStr::new("foo/baz"), s(BStr::new("*"))),
                        (BStr::new("bar/baz"), s(BStr::new(">=1.0"))),
                        (BStr::new("bar/hacked"), s(BStr::new("@stable"))),
                        (BStr::new("bar/woo"), s(BStr::new("1.0.0"))),
                    ]),
                ),
            ]),
            vec![
                BString::from("require.foo/baz : unbound version constraints (*) should be avoided"),
                BString::from("require.bar/baz : unbound version constraints (>=1.0) should be avoided"),
                BString::from("require.bar/hacked : unbound version constraints (@stable) should be avoided"),
                BString::from("require.bar/woo : exact version constraints (1.0.0) should be avoided if the package follows semantic versioning"),
            ],
            false,
            None,
        ),
        (
            config(vec![
                (BStr::new("name"), s(BStr::new("foo/bar"))),
                (
                    BStr::new("require"),
                    arr(vec![
                        (BStr::new("foo/baz"), s(BStr::new(">1, <0.5"))),
                        (BStr::new("bar/baz"), s(BStr::new("dev-main, >0.5"))),
                    ]),
                ),
            ]),
            vec![
                BString::from("require.foo/baz : this version constraint cannot possibly match anything (>1, <0.5)"),
                BString::from("require.bar/baz : this version constraint cannot possibly match anything (dev-main, >0.5)"),
            ],
            false,
            None,
        ),
        (
            config(vec![
                (BStr::new("name"), s(BStr::new("foo/bar"))),
                (BStr::new("require"), arr(vec![(BStr::new("bar/unstable"), s(BStr::new("0.3.0")))])),
            ]),
            vec![],
            false,
            None,
        ),
        (
            config(vec![
                (BStr::new("name"), s(BStr::new("foo/bar"))),
                (
                    BStr::new("extra"),
                    arr(vec![(
                        BStr::new("branch-alias"),
                        arr(vec![(BStr::new("5.x-dev"), s(BStr::new("3.1.x-dev")))]),
                    )]),
                ),
            ]),
            vec![BString::from("extra.branch-alias.5.x-dev : the target branch (3.1.x-dev) is not a valid numeric alias for this version")],
            false,
            None,
        ),
        (
            config(vec![
                (BStr::new("name"), s(BStr::new("foo/bar"))),
                (
                    BStr::new("extra"),
                    arr(vec![(BStr::new("branch-alias"), arr(vec![(BStr::new("5.x-dev"), s(BStr::new("3.1-dev")))]))]),
                ),
            ]),
            vec![BString::from("extra.branch-alias.5.x-dev : the target branch (3.1-dev) is not a valid numeric alias for this version")],
            false,
            None,
        ),
        (
            config(vec![
                (BStr::new("name"), s(BStr::new("foo/bar"))),
                (BStr::new("require"), arr(vec![(BStr::new("Foo/Baz"), s(BStr::new("^1.0")))])),
            ]),
            vec![BString::from("require.Foo/Baz is invalid, it should not contain uppercase characters. Please use foo/baz instead.")],
            false,
            None,
        ),
        (
            config(vec![(BStr::new("name"), s(BStr::new("a/b"))), (BStr::new("license"), s(BStr::new("XXXXX")))]),
            vec![bconcat!("License \"XXXXX\" is not a valid SPDX license identifier, see https://spdx.org/licenses/ if you use an open license.", PHP_EOL, "If the software is closed-source, you may use \"proprietary\" as license.").to_owned()],
            true,
            Some(config(vec![
                (BStr::new("name"), s(BStr::new("a/b"))),
                (BStr::new("license"), list(vec![s(BStr::new("XXXXX"))])),
            ])),
        ),
        (
            config(vec![
                (BStr::new("name"), s(BStr::new("a/b"))),
                (BStr::new("license"), list(vec![arr(vec![(BStr::new("author"), s(BStr::new("bar")))]), s(BStr::new("MIT"))])),
            ]),
            vec![BString::from("License {\"author\":\"bar\"} should be a string.")],
            true,
            Some(config(vec![
                (BStr::new("name"), s(BStr::new("a/b"))),
                (BStr::new("license"), list(vec![s(BStr::new("MIT"))])),
            ])),
        ),
    ]
}

/// ref: ValidatingArrayLoaderTest::testLoadWarnings
#[test]
fn test_load_warnings() {
    for (cfg, expected_warnings, _must_check, _expected_array) in warning_provider() {
        let mut expected_warnings: Vec<BString> = expected_warnings.into_iter().collect();
        let internal_loader = mock_loader();
        let loader = ValidatingArrayLoader::new(
            Box::new(internal_loader),
            true,
            None,
            ValidatingArrayLoader::CHECK_ALL,
        );
        loader
            .load(
                cfg,
                Some(BString::from("Composer\\Package\\CompletePackage")),
            )
            .unwrap();
        let mut warnings: Vec<BString> = loader.get_warnings();
        expected_warnings.sort();
        warnings.sort();
        assert_eq!(expected_warnings, warnings);
    }
}

/// ref: ValidatingArrayLoaderTest::testLoadSkipsWarningDataWhenIgnoringErrors
#[test]
fn test_load_skips_warning_data_when_ignoring_errors() {
    for (mut cfg, _expected_warnings, must_check, expected_array) in warning_provider() {
        if !must_check {
            continue;
        }
        let expected = expected_array
            .unwrap_or_else(|| config(vec![(BStr::new("name"), s(BStr::new("a/b")))]));

        // The inner loader is called exactly once with the (post-validation) config.
        let mut internal_loader = MockLoader::new();
        internal_loader
            .expect_load()
            .times(1)
            .withf(move |cfg, _class| *cfg == expected)
            .returning(|_, _| Ok(test_case::get_package("mock/mock", "1.0.0")));

        let loader = ValidatingArrayLoader::new(
            Box::new(internal_loader),
            true,
            None,
            ValidatingArrayLoader::CHECK_ALL,
        );
        cfg.insert("name".into(), s(BStr::new("a/b")));
        loader
            .load(
                cfg,
                Some(BString::from("Composer\\Package\\CompletePackage")),
            )
            .unwrap();
    }
}
