//! ref: composer/tests/Composer/Test/InstalledVersionsTest.php
//!
//! `InstalledVersions` has no Rust port: its readers are plugins and project code, which run in
//! the PHP worker against the copy `FilesystemRepository::write` dumps. These tests drive the real
//! PHP class there, autoloaded from the Composer checkout's vendor directory just as PHPUnit
//! autoloads it upstream, so `$selfDir` and the registered `ClassLoader` match the upstream run.

#[path = "common/php_worker.rs"]
mod php_worker;

use indexmap::IndexMap;
use php_worker::{
    load_composer_php_runtime, lock_php_worker, php_call_static, php_eval, php_runtime_available,
    php_single_quote,
};
use shirabe_php_rpc::PluginValue;
use shirabe_php_shim::{PhpMixed, realpath};
use tempfile::TempDir;

const INSTALLED_VERSIONS: &str = "Composer\\InstalledVersions";

/// `$this->root` of the upstream test class, kept alive for the duration of one test.
struct SetUp {
    root: TempDir,
}

impl SetUp {
    fn root(&self) -> &str {
        self.root.path().to_str().unwrap()
    }
}

fn set_up() -> SetUp {
    load_composer_php_runtime();

    // setUpBeforeClass: disable the multiple-ClassLoader-based checks of InstalledVersions by
    // making it seem like no class loaders are registered. A ClassLoader cannot cross the wire,
    // so the snapshot the upstream tearDownAfterClass restores stays in the worker.
    php_eval(
        r"$prop = new \ReflectionProperty('Composer\Autoload\ClassLoader', 'registeredLoaders');
        (\PHP_VERSION_ID < 80100) and $prop->setAccessible(true);
        if (!array_key_exists('__shirabe_previous_registered_loaders', $GLOBALS)) {
            $GLOBALS['__shirabe_previous_registered_loaders'] = $prop->getValue();
        }
        $prop->setValue(null, []);
        return true;",
    );

    let root = TempDir::new().unwrap();
    let data = fixture_data(root.path().to_str().unwrap());
    call_static("reload", vec![data]);
    SetUp { root }
}

/// `require __DIR__.'/Repository/Fixtures/installed_relative.php'` with `$dir` bound, evaluated in
/// the worker so the upstream fixture file is used as-is.
fn fixture_data(dir: &str) -> PluginValue {
    php_eval(&format!(
        "$dir = {};\nreturn require {};",
        php_single_quote(dir),
        php_single_quote(&fixture_path("installed_relative.php")),
    ))
}

fn fixture_path(name: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../composer/tests/Composer/Test/Repository/Fixtures")
        .join(name)
        .canonicalize()
        .expect("the Composer checkout must provide the repository fixtures")
        .to_str()
        .unwrap()
        .to_string()
}

fn call_static(method: &str, args: Vec<PluginValue>) -> PluginValue {
    php_call_static(INSTALLED_VERSIONS, method, args)
}

fn string_of(value: &PluginValue) -> String {
    match value {
        PluginValue::String(bytes) => String::from_utf8(bytes.clone()).unwrap(),
        other => panic!("expected a string, got {other:?}"),
    }
}

fn string_list(names: &[&str]) -> PluginValue {
    PluginValue::List(
        names
            .iter()
            .map(|name| PluginValue::string(*name))
            .collect(),
    )
}

#[test]
fn test_get_installed_packages() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    let _set_up = set_up();

    let names = [
        "__root__",
        "a/provider",
        "a/provider2",
        "b/replacer",
        "c/c",
        "foo/impl",
        "foo/impl2",
        "foo/replaced",
        "meta/package",
    ];
    assert_eq!(
        string_list(&names),
        call_static("getInstalledPackages", vec![])
    );
}

#[test]
fn test_is_installed() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    let _set_up = set_up();

    for (expected, name, include_dev_requirements) in is_installed_provider() {
        assert_eq!(
            PluginValue::Bool(expected),
            call_static(
                "isInstalled",
                vec![
                    PluginValue::string(name),
                    PluginValue::Bool(include_dev_requirements)
                ]
            ),
            "isInstalled({name}, {include_dev_requirements})",
        );
    }
}

fn is_installed_provider() -> Vec<(bool, &'static str, bool)> {
    vec![
        (true, "foo/impl", true),
        (true, "foo/replaced", true),
        (true, "c/c", true),
        (false, "c/c", false),
        (true, "__root__", true),
        (true, "b/replacer", true),
        (false, "not/there", true),
        (true, "meta/package", true),
    ]
}

#[test]
fn test_satisfies() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    let _set_up = set_up();

    for (expected, name, constraint) in satisfies_provider() {
        assert_eq!(
            PluginValue::Bool(expected),
            call_static(
                "satisfies",
                vec![
                    new_version_parser(),
                    PluginValue::string(name),
                    PluginValue::string(constraint)
                ]
            ),
            "satisfies({name}, {constraint})",
        );
    }
}

fn new_version_parser() -> PluginValue {
    shirabe_php_rpc::new_object("Composer\\Semver\\VersionParser", vec![], None)
        .expect("VersionParser request failed")
        .expect("VersionParser threw")
}

fn satisfies_provider() -> Vec<(bool, &'static str, &'static str)> {
    vec![
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
    ]
}

#[test]
fn test_get_version_ranges() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    let _set_up = set_up();

    for (expected, name) in get_version_ranges_provider() {
        assert_eq!(
            PluginValue::string(expected),
            call_static("getVersionRanges", vec![PluginValue::string(name)]),
            "getVersionRanges({name})",
        );
    }
}

fn get_version_ranges_provider() -> Vec<(&'static str, &'static str)> {
    vec![
        ("dev-master || 1.10.x-dev", "__root__"),
        ("^1.1 || 1.2 || 1.4 || 2.0", "foo/impl"),
        ("2.2 || 2.0", "foo/impl2"),
        ("^3.0", "foo/replaced"),
        ("1.1", "a/provider"),
        ("1.2 || 1.4", "a/provider2"),
        ("2.2", "b/replacer"),
        ("3.0", "c/c"),
    ]
}

#[test]
fn test_get_version() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    let _set_up = set_up();

    for (expected, name) in get_version_provider() {
        assert_eq!(
            optional_string(expected),
            call_static("getVersion", vec![PluginValue::string(name)]),
            "getVersion({name})",
        );
    }
}

fn get_version_provider() -> Vec<(Option<&'static str>, &'static str)> {
    vec![
        (Some("dev-master"), "__root__"),
        (None, "foo/impl"),
        (None, "foo/impl2"),
        (None, "foo/replaced"),
        (Some("1.1.0.0"), "a/provider"),
        (Some("1.2.0.0"), "a/provider2"),
        (Some("2.2.0.0"), "b/replacer"),
        (Some("3.0.0.0"), "c/c"),
    ]
}

#[test]
fn test_get_pretty_version() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    let _set_up = set_up();

    for (expected, name) in get_pretty_version_provider() {
        assert_eq!(
            optional_string(expected),
            call_static("getPrettyVersion", vec![PluginValue::string(name)]),
            "getPrettyVersion({name})",
        );
    }
}

fn get_pretty_version_provider() -> Vec<(Option<&'static str>, &'static str)> {
    vec![
        (Some("dev-master"), "__root__"),
        (None, "foo/impl"),
        (None, "foo/impl2"),
        (None, "foo/replaced"),
        (Some("1.1"), "a/provider"),
        (Some("1.2"), "a/provider2"),
        (Some("2.2"), "b/replacer"),
        (Some("3.0"), "c/c"),
    ]
}

fn optional_string(value: Option<&str>) -> PluginValue {
    match value {
        Some(value) => PluginValue::string(value),
        None => PluginValue::Null,
    }
}

#[test]
fn test_get_version_out_of_bounds() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    let _set_up = set_up();

    let throw = shirabe_php_rpc::call_static_method(
        INSTALLED_VERSIONS,
        "getVersion",
        vec![PluginValue::string("not/installed")],
        None,
    )
    .expect("getVersion request failed")
    .expect_err("getVersion must throw for a package that is not installed");
    assert_eq!("OutOfBoundsException", throw.exception_class);
}

#[test]
fn test_get_root_package() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    let set_up = set_up();

    let expected = PhpMixed::Array(IndexMap::from([
        ("name".to_string(), PhpMixed::String("__root__".to_string())),
        (
            "pretty_version".to_string(),
            PhpMixed::String("dev-master".to_string()),
        ),
        (
            "version".to_string(),
            PhpMixed::String("dev-master".to_string()),
        ),
        (
            "reference".to_string(),
            PhpMixed::String("sourceref-by-default".to_string()),
        ),
        ("type".to_string(), PhpMixed::String("library".to_string())),
        (
            "install_path".to_string(),
            PhpMixed::String(format!("{}/./", set_up.root())),
        ),
        (
            "aliases".to_string(),
            PhpMixed::List(vec![PhpMixed::String("1.10.x-dev".to_string())]),
        ),
        ("dev".to_string(), PhpMixed::Bool(true)),
    ]));

    assert_eq!(
        PluginValue::from_php_mixed(&expected),
        call_static("getRootPackage", vec![])
    );
}

#[test]
fn test_get_raw_data() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    let set_up = set_up();

    assert_eq!(
        fixture_data(set_up.root()),
        call_static("getRawData", vec![])
    );
}

#[test]
fn test_get_reference() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    let _set_up = set_up();

    for (expected, name) in get_reference_provider() {
        assert_eq!(
            optional_string(expected),
            call_static("getReference", vec![PluginValue::string(name)]),
            "getReference({name})",
        );
    }
}

fn get_reference_provider() -> Vec<(Option<&'static str>, &'static str)> {
    vec![
        (Some("sourceref-by-default"), "__root__"),
        (None, "foo/impl"),
        (None, "foo/impl2"),
        (None, "foo/replaced"),
        (Some("distref-as-no-source"), "a/provider"),
        (Some("distref-as-installed-from-dist"), "a/provider2"),
        (None, "b/replacer"),
        (None, "c/c"),
    ]
}

#[test]
fn test_get_installed_packages_by_type() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    let _set_up = set_up();

    let names = ["__root__", "a/provider", "a/provider2", "b/replacer", "c/c"];
    assert_eq!(
        string_list(&names),
        call_static(
            "getInstalledPackagesByType",
            vec![PluginValue::string("library")]
        )
    );
}

#[test]
fn test_get_install_path() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    let set_up = set_up();

    let root_install_path = string_of(&call_static(
        "getInstallPath",
        vec![PluginValue::string("__root__")],
    ));
    assert_eq!(realpath(set_up.root()), realpath(root_install_path));
    assert_eq!(
        PluginValue::string("/foo/bar/vendor/c/c"),
        call_static("getInstallPath", vec![PluginValue::string("c/c")])
    );
    assert_eq!(
        PluginValue::Null,
        call_static("getInstallPath", vec![PluginValue::string("foo/impl")])
    );
}

#[test]
fn test_with_class_loader_loaded() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    let _set_up = set_up();

    // disable multiple-ClassLoader-based checks of InstalledVersions by making it seem like no
    // class loaders are registered
    php_eval(
        r"$prop = new \ReflectionProperty(\Composer\Autoload\ClassLoader::class, 'registeredLoaders');
        (\PHP_VERSION_ID < 80100) and $prop->setAccessible(true);
        $prop->setValue(null, array_slice($GLOBALS['__shirabe_previous_registered_loaders'], 0, 1, true));

        $prop2 = new \ReflectionProperty(\Composer\InstalledVersions::class, 'installedIsLocalDir');
        (\PHP_VERSION_ID < 80100) and $prop2->setAccessible(true);
        $prop2->setValue(null, true);
        return true;",
    );

    assert_eq!(
        PluginValue::Bool(false),
        call_static("isInstalled", vec![PluginValue::string("foo/bar")])
    );
    let reloaded = PluginValue::Array(IndexMap::from([
        (b"root".to_vec(), call_static("getRootPackage", vec![])),
        (
            b"versions".to_vec(),
            PluginValue::Array(IndexMap::from([(
                b"foo/bar".to_vec(),
                PluginValue::from_php_mixed(&PhpMixed::Array(IndexMap::from([
                    ("version".to_string(), PhpMixed::String("1.0.0".to_string())),
                    ("dev_requirement".to_string(), PhpMixed::Bool(false)),
                ]))),
            )])),
        ),
    ]));
    call_static("reload", vec![reloaded]);
    assert_eq!(
        PluginValue::Bool(true),
        call_static("isInstalled", vec![PluginValue::string("foo/bar")])
    );

    php_eval(
        r"$prop = new \ReflectionProperty(\Composer\Autoload\ClassLoader::class, 'registeredLoaders');
        (\PHP_VERSION_ID < 80100) and $prop->setAccessible(true);
        $prop->setValue(null, []);
        return true;",
    );
}

/// Not an upstream test: the class exercised above is the one Composer's own vendor directory
/// autoloads, while `FilesystemRepository::write` dumps the `include_str!`ed source file. The two
/// have to be the same bytes for the tests above to say anything about what Shirabe ships.
#[test]
fn test_worker_loads_the_installed_versions_file_shirabe_dumps() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    load_composer_php_runtime();

    let loaded = string_of(&php_eval(
        r"return (new \ReflectionClass(\Composer\InstalledVersions::class))->getFileName();",
    ));
    assert_eq!(
        include_str!("../../../composer/src/Composer/InstalledVersions.php"),
        std::fs::read_to_string(&loaded).unwrap(),
        "the worker autoloads {loaded}, which must match the file Shirabe dumps",
    );
}
