//! ref: composer/tests/Composer/Test/InstalledVersionsTest.php sha1:dff7aafa70bbd5c37a4bdb35619697fa5d6cdddd
//!
//! `InstalledVersions` has no Rust port: its readers are plugins and project code, which run in
//! the PHP worker against the copy `FilesystemRepository::write` dumps. These tests drive the real
//! PHP class there, autoloaded from the Composer checkout's vendor directory just as PHPUnit
//! autoloads it upstream, so `$selfDir` and the registered `ClassLoader` match the upstream run.

#[path = "common/bootstrap.rs"]
mod bootstrap;
#[path = "common/php_worker.rs"]
mod php_worker;
#[path = "common/test_case.rs"]
mod test_case;

use crate::test_case::get_unique_tmp_directory;
use bstr::{BStr, BString};
use indexmap::IndexMap;
use php_worker::{load_composer_php_runtime, lock_php_worker, php_call_static, php_eval};
use shirabe::php_worker::call::PhpCall;
use shirabe_php_rpc::{PluginValue, ToWire};
use shirabe_php_shim::{
    PhpMixed, bformat, path_from_bytes, path_to_bytes, php_str, realpath, var_export_str,
};
use tempfile::TempDir;

const INSTALLED_VERSIONS: &BStr = php_str(b"Composer\\InstalledVersions");

/// `$this->root` of the upstream test class, kept alive for the duration of one test.
struct SetUp {
    root: TempDir,
}

impl SetUp {
    fn root(&self) -> &std::path::Path {
        self.root.path()
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

    let root = get_unique_tmp_directory();
    let data = fixture_data(root.path());
    call_static(BStr::new("reload"), vec![data]);
    SetUp { root }
}

/// `require __DIR__.'/Repository/Fixtures/installed_relative.php'` with `$dir` bound, evaluated in
/// the worker so the upstream fixture file is used as-is.
fn fixture_data(dir: &std::path::Path) -> PluginValue {
    php_eval(bformat!(
        "$dir = {};\nreturn require {};",
        var_export_str(path_to_bytes(dir), true),
        var_export_str(path_to_bytes(&fixture_path("installed_relative.php")), true)
    ))
}

fn fixture_path(name: impl AsRef<std::path::Path>) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../composer/tests/Composer/Test/Repository/Fixtures")
        .join(name)
        .canonicalize()
        .expect("the Composer checkout must provide the repository fixtures")
}

fn call_static(method: &BStr, args: Vec<PluginValue>) -> PluginValue {
    php_call_static(INSTALLED_VERSIONS, method, args)
}

fn string_of(value: &PluginValue) -> bstr::BString {
    match value {
        PluginValue::String(bytes) => bytes.clone(),
        other => panic!("expected a string, got {other:?}"),
    }
}

fn string_list(names: &[impl AsRef<[u8]>]) -> PluginValue {
    PluginValue::List(
        names
            .iter()
            .map(|name| PluginValue::string(name.as_ref()))
            .collect(),
    )
}

#[test]
fn test_get_installed_packages() {
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
        call_static(BStr::new("getInstalledPackages"), vec![])
    );
}

#[test]
fn test_is_installed() {
    let _worker = lock_php_worker();
    let _set_up = set_up();

    for (expected, name, include_dev_requirements) in is_installed_provider() {
        assert_eq!(
            PluginValue::Bool(expected),
            call_static(
                BStr::new("isInstalled"),
                vec![
                    PluginValue::string(name),
                    PluginValue::Bool(include_dev_requirements)
                ]
            ),
            "isInstalled({name}, {include_dev_requirements})",
            name = name.display_lossy(),
        );
    }
}

fn is_installed_provider() -> Vec<(bool, &'static BStr, bool)> {
    vec![
        (true, BStr::new("foo/impl"), true),
        (true, BStr::new("foo/replaced"), true),
        (true, BStr::new("c/c"), true),
        (false, BStr::new("c/c"), false),
        (true, BStr::new("__root__"), true),
        (true, BStr::new("b/replacer"), true),
        (false, BStr::new("not/there"), true),
        (true, BStr::new("meta/package"), true),
    ]
}

#[test]
fn test_satisfies() {
    let _worker = lock_php_worker();
    let _set_up = set_up();

    for (expected, name, constraint) in satisfies_provider() {
        assert_eq!(
            PluginValue::Bool(expected),
            call_static(
                BStr::new("satisfies"),
                vec![
                    new_version_parser(),
                    PluginValue::string(name),
                    PluginValue::string(constraint)
                ]
            ),
            "satisfies({name}, {constraint})",
            name = name.display_lossy(),
            constraint = constraint.display_lossy(),
        );
    }
}

fn new_version_parser() -> PluginValue {
    PhpCall::New(BStr::new("Composer\\Semver\\VersionParser"))
        .try_run::<PluginValue>(vec![])
        .expect("VersionParser request failed")
        .expect("VersionParser threw")
}

fn satisfies_provider() -> Vec<(bool, &'static BStr, &'static BStr)> {
    vec![
        (true, BStr::new("foo/impl"), BStr::new("1.5")),
        (true, BStr::new("foo/impl"), BStr::new("1.2")),
        (true, BStr::new("foo/impl"), BStr::new("^1.0")),
        (true, BStr::new("foo/impl"), BStr::new("^3 || ^2")),
        (false, BStr::new("foo/impl"), BStr::new("^3")),
        (true, BStr::new("foo/replaced"), BStr::new("3.5")),
        (true, BStr::new("foo/replaced"), BStr::new("^3.2")),
        (false, BStr::new("foo/replaced"), BStr::new("4.0")),
        (true, BStr::new("c/c"), BStr::new("3.0.0")),
        (true, BStr::new("c/c"), BStr::new("^3")),
        (false, BStr::new("c/c"), BStr::new("^3.1")),
        (true, BStr::new("__root__"), BStr::new("dev-master")),
        (true, BStr::new("__root__"), BStr::new("^1.10")),
        (false, BStr::new("__root__"), BStr::new("^2")),
        (true, BStr::new("b/replacer"), BStr::new("^2.1")),
        (false, BStr::new("b/replacer"), BStr::new("^2.3")),
        (true, BStr::new("a/provider2"), BStr::new("^1.2")),
        (true, BStr::new("a/provider2"), BStr::new("^1.4")),
        (false, BStr::new("a/provider2"), BStr::new("^1.5")),
    ]
}

#[test]
fn test_get_version_ranges() {
    let _worker = lock_php_worker();
    let _set_up = set_up();

    for (expected, name) in get_version_ranges_provider() {
        assert_eq!(
            PluginValue::string(expected),
            call_static(
                BStr::new("getVersionRanges"),
                vec![PluginValue::string(name)]
            ),
            "getVersionRanges({name})",
            name = name.display_lossy(),
        );
    }
}

fn get_version_ranges_provider() -> Vec<(&'static BStr, &'static BStr)> {
    vec![
        (BStr::new("dev-master || 1.10.x-dev"), BStr::new("__root__")),
        (
            BStr::new("^1.1 || 1.2 || 1.4 || 2.0"),
            BStr::new("foo/impl"),
        ),
        (BStr::new("2.2 || 2.0"), BStr::new("foo/impl2")),
        (BStr::new("^3.0"), BStr::new("foo/replaced")),
        (BStr::new("1.1"), BStr::new("a/provider")),
        (BStr::new("1.2 || 1.4"), BStr::new("a/provider2")),
        (BStr::new("2.2"), BStr::new("b/replacer")),
        (BStr::new("3.0"), BStr::new("c/c")),
    ]
}

#[test]
fn test_get_version() {
    let _worker = lock_php_worker();
    let _set_up = set_up();

    for (expected, name) in get_version_provider() {
        assert_eq!(
            optional_string(expected),
            call_static(BStr::new("getVersion"), vec![PluginValue::string(name)]),
            "getVersion({name})",
            name = name.display_lossy(),
        );
    }
}

fn get_version_provider() -> Vec<(Option<&'static BStr>, &'static BStr)> {
    vec![
        (Some(BStr::new("dev-master")), BStr::new("__root__")),
        (None, BStr::new("foo/impl")),
        (None, BStr::new("foo/impl2")),
        (None, BStr::new("foo/replaced")),
        (Some(BStr::new("1.1.0.0")), BStr::new("a/provider")),
        (Some(BStr::new("1.2.0.0")), BStr::new("a/provider2")),
        (Some(BStr::new("2.2.0.0")), BStr::new("b/replacer")),
        (Some(BStr::new("3.0.0.0")), BStr::new("c/c")),
    ]
}

#[test]
fn test_get_pretty_version() {
    let _worker = lock_php_worker();
    let _set_up = set_up();

    for (expected, name) in get_pretty_version_provider() {
        assert_eq!(
            optional_string(expected),
            call_static(
                BStr::new("getPrettyVersion"),
                vec![PluginValue::string(name)]
            ),
            "getPrettyVersion({name})",
            name = name.display_lossy(),
        );
    }
}

fn get_pretty_version_provider() -> Vec<(Option<&'static BStr>, &'static BStr)> {
    vec![
        (Some(BStr::new("dev-master")), BStr::new("__root__")),
        (None, BStr::new("foo/impl")),
        (None, BStr::new("foo/impl2")),
        (None, BStr::new("foo/replaced")),
        (Some(BStr::new("1.1")), BStr::new("a/provider")),
        (Some(BStr::new("1.2")), BStr::new("a/provider2")),
        (Some(BStr::new("2.2")), BStr::new("b/replacer")),
        (Some(BStr::new("3.0")), BStr::new("c/c")),
    ]
}

fn optional_string(value: Option<&BStr>) -> PluginValue {
    match value {
        Some(value) => PluginValue::string(value),
        None => PluginValue::Null,
    }
}

#[test]
fn test_get_version_out_of_bounds() {
    let _worker = lock_php_worker();
    let _set_up = set_up();

    let throw = PhpCall::StaticMethod(BStr::new(INSTALLED_VERSIONS), BStr::new("getVersion"))
        .try_run::<PluginValue>(vec![PluginValue::string("not/installed")])
        .expect("getVersion request failed")
        .expect_err("getVersion must throw for a package that is not installed");
    assert_eq!("OutOfBoundsException", throw.exception_class);
}

#[test]
fn test_get_root_package() {
    let _worker = lock_php_worker();
    let set_up = set_up();

    let expected = PhpMixed::Array(IndexMap::from([
        ("name".into(), PhpMixed::string(BString::from("__root__"))),
        (
            "pretty_version".into(),
            PhpMixed::string(BString::from("dev-master")),
        ),
        (
            "version".into(),
            PhpMixed::string(BString::from("dev-master")),
        ),
        (
            "reference".into(),
            PhpMixed::string(BString::from("sourceref-by-default")),
        ),
        ("type".into(), PhpMixed::string(BString::from("library"))),
        (
            "install_path".into(),
            PhpMixed::string(bformat!("{}/./", set_up.root())),
        ),
        (
            "aliases".into(),
            PhpMixed::List(vec![PhpMixed::string(BString::from("1.10.x-dev"))]),
        ),
        ("dev".into(), PhpMixed::Bool(true)),
    ]));

    assert_eq!(
        expected.clone().to_wire(),
        call_static(BStr::new("getRootPackage"), vec![])
    );
}

#[test]
fn test_get_raw_data() {
    let _worker = lock_php_worker();
    let set_up = set_up();

    assert_eq!(
        fixture_data(set_up.root()),
        call_static(BStr::new("getRawData"), vec![])
    );
}

#[test]
fn test_get_reference() {
    let _worker = lock_php_worker();
    let _set_up = set_up();

    for (expected, name) in get_reference_provider() {
        assert_eq!(
            optional_string(expected),
            call_static(BStr::new("getReference"), vec![PluginValue::string(name)]),
            "getReference({name})",
            name = name.display_lossy(),
        );
    }
}

fn get_reference_provider() -> Vec<(Option<&'static BStr>, &'static BStr)> {
    vec![
        (
            Some(BStr::new("sourceref-by-default")),
            BStr::new("__root__"),
        ),
        (None, BStr::new("foo/impl")),
        (None, BStr::new("foo/impl2")),
        (None, BStr::new("foo/replaced")),
        (
            Some(BStr::new("distref-as-no-source")),
            BStr::new("a/provider"),
        ),
        (
            Some(BStr::new("distref-as-installed-from-dist")),
            BStr::new("a/provider2"),
        ),
        (None, BStr::new("b/replacer")),
        (None, BStr::new("c/c")),
    ]
}

#[test]
fn test_get_installed_packages_by_type() {
    let _worker = lock_php_worker();
    let _set_up = set_up();

    let names = ["__root__", "a/provider", "a/provider2", "b/replacer", "c/c"];
    assert_eq!(
        string_list(&names),
        call_static(
            BStr::new("getInstalledPackagesByType"),
            vec![PluginValue::string("library")]
        )
    );
}

#[test]
fn test_get_install_path() {
    let _worker = lock_php_worker();
    let set_up = set_up();

    let root_install_path = string_of(&call_static(
        BStr::new("getInstallPath"),
        vec![PluginValue::string("__root__")],
    ));
    assert_eq!(
        realpath(set_up.root()).map(|p| bstr::BString::from(path_to_bytes(&p))),
        realpath(path_from_bytes(&root_install_path))
            .map(|p| bstr::BString::from(path_to_bytes(&p)))
    );
    assert_eq!(
        PluginValue::string("/foo/bar/vendor/c/c"),
        call_static(
            BStr::new("getInstallPath"),
            vec![PluginValue::string("c/c")]
        )
    );
    assert_eq!(
        PluginValue::Null,
        call_static(
            BStr::new("getInstallPath"),
            vec![PluginValue::string("foo/impl")]
        )
    );
}

#[test]
fn test_with_class_loader_loaded() {
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
        call_static(
            BStr::new("isInstalled"),
            vec![PluginValue::string("foo/bar")]
        )
    );
    let reloaded = PluginValue::Array(IndexMap::from([
        (
            "root".into(),
            call_static(BStr::new("getRootPackage"), vec![]),
        ),
        (
            "versions".into(),
            PluginValue::Array(IndexMap::from([(
                "foo/bar".into(),
                PhpMixed::Array(IndexMap::from([
                    ("version".into(), PhpMixed::string(BString::from("1.0.0"))),
                    ("dev_requirement".into(), PhpMixed::Bool(false)),
                ]))
                .to_wire(),
            )])),
        ),
    ]));
    call_static(BStr::new("reload"), vec![reloaded]);
    assert_eq!(
        PluginValue::Bool(true),
        call_static(
            BStr::new("isInstalled"),
            vec![PluginValue::string("foo/bar")]
        )
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
    let _worker = lock_php_worker();
    load_composer_php_runtime();

    // Read in the worker rather than from Rust: the file it autoloads lives inside the runtime
    // bundle, which only the PHP side has a stream wrapper for.
    let loaded = string_of(&php_eval(
        r"return (new \ReflectionClass(\Composer\InstalledVersions::class))->getFileName();",
    ));
    let contents = string_of(&php_eval(
        r"return file_get_contents(
            (new \ReflectionClass(\Composer\InstalledVersions::class))->getFileName()
        );",
    ));
    assert_eq!(
        include_str!("../../../composer/src/Composer/InstalledVersions.php"),
        contents,
        "the worker autoloads {}, which must match the file Shirabe dumps",
        loaded.display_lossy(),
    );
}
