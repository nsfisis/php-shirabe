//! ref: composer/tests/Composer/Test/Repository/FilesystemRepositoryTest.php

use crate::php_worker::{load_composer_php_runtime, php_call_static, php_runtime_available};
use crate::test_case::{get_alias_package, get_package};
use bstr::BString;
use indexmap::IndexMap;
use serial_test::serial;
use shirabe::dependency_resolver::operation::AnyOperation;
use shirabe::installer::{InstallationManagerInterface, InstallerInterface};
use shirabe::io::IOInterface;
use shirabe::json::json_file::JsonFile;
use shirabe::package::loader::ArrayLoader;
use shirabe::package::{Link, PackageInterfaceHandle, RootAliasPackageHandle, RootPackageHandle};
use shirabe::repository::RepositoryInterface;
use shirabe::repository::filesystem_repository::FilesystemRepository;
use shirabe::util::filesystem::Filesystem;
use shirabe_php_rpc::PluginValue;
use shirabe_php_shim::{Catch as _, PhpMixed, legacy_lossy};
use shirabe_semver::VersionParser;

/// PHP mocks JsonFile::read()/exists(); without a mocking framework the canned read value is
/// materialized as a real temp file whose decoded JSON reproduces the mock return value exactly.
fn create_temp_json_file(contents: &str) -> String {
    let mut path = std::env::temp_dir();
    let unique = format!(
        "shirabe_filesystemrepositorytest_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    path.push(unique);
    std::fs::write(&path, contents.as_bytes()).unwrap();
    path.to_str().unwrap().to_string()
}

#[test]
fn test_repository_read() {
    let path = create_temp_json_file(
        r#"[{"name": "package1", "version": "1.0.0-beta", "type": "vendor"}]"#,
    );
    let json = JsonFile::new(path, None, None).unwrap();

    let mut repository = FilesystemRepository::new(json, false, None, None).unwrap();

    let packages = repository.get_packages().unwrap();

    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].get_name(), "package1");
    assert_eq!(packages[0].get_version(), "1.0.0.0-beta");
    assert_eq!(packages[0].get_type(), "vendor");
}

#[test]
fn test_corrupted_repository_file() {
    // PHP mocks read() to return the scalar string 'foo'; a real file containing the JSON string
    // "foo" decodes to the same value, which the repository rejects as a non-array package list.
    let path = create_temp_json_file(r#""foo""#);
    let json = JsonFile::new(path, None, None).unwrap();

    let mut repository = FilesystemRepository::new(json, false, None, None).unwrap();

    let result = repository.get_packages();
    let err = result.unwrap_err();
    assert!(
        err.is_instanceof::<shirabe::repository::InvalidRepositoryException>(),
        "expected InvalidRepositoryException, got: {err}"
    );
}

#[test]
fn test_unexistent_repository_file() {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "shirabe_filesystemrepositorytest_missing_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let json = JsonFile::new(path.to_str().unwrap().to_string(), None, None).unwrap();

    let mut repository = FilesystemRepository::new(json, false, None, None).unwrap();

    let packages = repository.get_packages().unwrap();
    assert_eq!(packages.len(), 0);
}

mockall::mock! {
    /// Mocks the PHPUnit `InstallationManager` mock built with `disableOriginalConstructor()`. Only
    /// `get_install_path` is reached by `FilesystemRepository::write`.
    #[derive(Debug)]
    pub InstallationManager {}
    #[async_trait::async_trait(?Send)]
    impl InstallationManagerInterface for InstallationManager {
        fn add_installer(&self, installer: std::rc::Rc<dyn InstallerInterface>);
        fn remove_installer(&self, installer: &dyn InstallerInterface);
        fn disable_plugins(&mut self);
        fn is_package_installed(
            &mut self,
            repo: &shirabe::repository::InstalledRepositoryInterfaceHandle,
            package: PackageInterfaceHandle,
        ) -> anyhow::Result<bool>;
        fn ensure_binaries_presence(&mut self, package: PackageInterfaceHandle) -> anyhow::Result<()>;
        async fn execute(
            &self,
            repo: &shirabe::repository::InstalledRepositoryInterfaceHandle,
            operations: Vec<std::rc::Rc<AnyOperation>>,
            dev_mode: bool,
            run_scripts: bool,
            download_only: bool,
        ) -> anyhow::Result<()>;
        fn get_install_path(&self, package: PackageInterfaceHandle) -> Option<String>;
        fn set_output_progress(&mut self, output_progress: bool);
        async fn notify_installs(&mut self, io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>);
    }
}

#[test]
fn test_repository_write() {
    // PHP mocks JsonFile::write/read/getPath; here a real JsonFile under a temp repo dir is written
    // and read back. write() never reads the file (it dumps the in-memory packages), so the mocked
    // read()/exists() return values are irrelevant to the result.
    let base = std::fs::canonicalize(std::env::temp_dir()).unwrap();
    let repo_dir = base.join(format!(
        "shirabe_repo_write_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut fs = Filesystem::new(None);
    fs.remove_directory(&repo_dir).ok();

    let json_path = repo_dir.join("vendor/composer/installed.json");
    let json = JsonFile::new(json_path.to_str().unwrap().to_string(), None, None).unwrap();
    let mut repository = FilesystemRepository::new(json, false, None, None).unwrap();

    let mut im = MockInstallationManager::new();
    let fixed_path = repo_dir
        .join("vendor/woop/woop")
        .to_str()
        .unwrap()
        .to_string();
    im.expect_get_install_path()
        .times(2)
        .returning(move |_| Some(fixed_path.clone()));

    repository.set_dev_package_names(vec![bstr::BString::from("mypkg2".to_string())]);
    repository
        .add_package(get_package("mypkg2", "1.2.3"))
        .unwrap();
    repository
        .add_package(get_package("mypkg", "0.1.10"))
        .unwrap();
    repository.write(true, &im).unwrap();

    let written = std::fs::read_to_string(&json_path).unwrap();
    let actual: serde_json::Value = serde_json::from_str(&written).unwrap();
    let expected = serde_json::json!({
        "packages": [
            {"name": "mypkg", "type": "library", "version": "0.1.10", "version_normalized": "0.1.10.0", "install-path": "../woop/woop"},
            {"name": "mypkg2", "type": "library", "version": "1.2.3", "version_normalized": "1.2.3.0", "install-path": "../woop/woop"},
        ],
        "dev": true,
        "dev-package-names": ["mypkg2"],
    });
    assert_eq!(actual, expected);

    fs.remove_directory(&repo_dir).ok();
}

/// chdir()s into a fresh temp dir on construction and restores the previous cwd on drop, mirroring
/// PHP's `getUniqueTmpDirectory()` + `chdir($dir)`. The cwd is restored before the temp tree is
/// removed so the directory (the cwd itself) can be deleted cleanly.
struct CwdGuard {
    temp: tempfile::TempDir,
    prev_cwd: std::path::PathBuf,
}

impl CwdGuard {
    fn new() -> Self {
        let temp = tempfile::TempDir::new().unwrap();
        let prev_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp.path()).unwrap();
        Self { temp, prev_cwd }
    }

    fn path(&self) -> std::path::PathBuf {
        self.temp.path().to_path_buf()
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.prev_cwd);
    }
}

/// ref: TestCase::configureLinks, inlined for the single package being configured.
fn configure_links(
    name: &str,
    pretty_version: &str,
    description: &str,
    links: &[(&str, &str)],
) -> IndexMap<BString, Link> {
    let mut map: IndexMap<BString, PhpMixed> = IndexMap::new();
    for (target, constraint) in links {
        map.insert(
            (*target).to_string().into(),
            PhpMixed::string((*constraint).to_string()),
        );
    }
    ArrayLoader::new(None, false)
        .parse_links(
            bstr::BStr::new(name),
            bstr::BStr::new(pretty_version),
            description,
            map,
        )
        .unwrap()
}

#[test]
// Serialized because CwdGuard changes the process-global cwd, which would race with the
// cwd-sensitive path_repository_test::test_url_remains_relative in this binary.
#[serial]
fn test_repository_writes_installed_php() {
    let guard = CwdGuard::new();
    let dir = guard.path();

    let json = JsonFile::new(
        dir.join("installed.json").to_str().unwrap().to_string(),
        None,
        None,
    )
    .unwrap();

    let root_package = RootPackageHandle::new(
        bstr::BString::from("__root__".to_string()),
        bstr::BString::from(&VersionParser.normalize("dev-master", None).unwrap()[..]),
        bstr::BString::from("dev-master".to_string()),
    );
    root_package.set_source_reference(Some(bstr::BString::from(
        "sourceref-by-default".to_string(),
    )));
    root_package.set_dist_reference(Some(bstr::BString::from("distref".to_string())));
    root_package.set_provides(configure_links(
        "__root__",
        "dev-master",
        Link::TYPE_PROVIDE,
        &[("foo/impl", "2.0")],
    ));
    let root_package = RootAliasPackageHandle::new(
        root_package,
        bstr::BString::from(&VersionParser.normalize("1.10.x-dev", None).unwrap()[..]),
        bstr::BString::from("1.10.x-dev".to_string()),
    );

    let mut repository =
        FilesystemRepository::new(json, true, Some(root_package.into()), None).unwrap();
    repository.set_dev_package_names(vec![bstr::BString::from("c/c".to_string())]);

    let pkg = get_package("a/provider", "1.1");
    pkg.__set_provides(configure_links(
        "a/provider",
        "1.1",
        Link::TYPE_PROVIDE,
        &[("foo/impl", "^1.1"), ("foo/impl2", "2.0")],
    ));
    pkg.set_dist_reference(Some(bstr::BString::from(
        "distref-as-no-source".to_string(),
    )));
    repository.add_package(pkg).unwrap();

    let pkg = get_package("a/provider2", "1.2");
    pkg.__set_provides(configure_links(
        "a/provider2",
        "1.2",
        Link::TYPE_PROVIDE,
        &[("foo/impl", "self.version"), ("foo/impl2", "2.0")],
    ));
    pkg.set_source_reference(Some(bstr::BString::from("sourceref".to_string())));
    pkg.set_dist_reference(Some(bstr::BString::from(
        "distref-as-installed-from-dist".to_string(),
    )));
    pkg.set_installation_source(Some(bstr::BString::from("dist".to_string())));
    repository.add_package(pkg.clone()).unwrap();

    repository
        .add_package(get_alias_package(&pkg, "1.4"))
        .unwrap();

    let pkg = get_package("b/replacer", "2.2");
    pkg.__set_replaces(configure_links(
        "b/replacer",
        "2.2",
        Link::TYPE_REPLACE,
        &[("foo/impl2", "self.version"), ("foo/replaced", "^3.0")],
    ));
    repository.add_package(pkg).unwrap();

    let pkg = get_package("c/c", "3.0");
    pkg.set_dist_reference(Some(bstr::BString::from(
        "{${passthru('bash -i')}} Foo\\Bar\n\ttab\u{0b}verticaltab\0".to_string(),
    )));
    repository.add_package(pkg).unwrap();

    let pkg = get_package("meta/package", "3.0");
    pkg.__set_type(bstr::BString::from("metapackage".to_string()));
    repository.add_package(pkg).unwrap();

    let mut im = MockInstallationManager::new();
    let cwd = dir.to_str().unwrap().to_string();
    im.expect_get_install_path().returning(move |package| {
        // check for empty paths handling
        if package.get_type() == "metapackage" {
            return Some(String::new());
        }
        if package.get_name() == "c/c" {
            // check for absolute paths
            return Some("/foo/bar/ven\\do{}r/c/c${}".to_string());
        }
        if package.get_name() == "a/provider" {
            return Some("vendor/{${passthru('bash -i')}}".to_string());
        }
        // check for cwd
        if package.as_root().is_some() {
            return Some(cwd.clone());
        }
        // check for relative paths
        Some(format!("vendor/{}", legacy_lossy(&package.get_name())))
    });

    repository.write(true, &im).unwrap();

    let expected = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../composer/tests/Composer/Test/Repository/Fixtures/installed.php"),
    )
    .unwrap();
    let actual = std::fs::read_to_string(dir.join("installed.php")).unwrap();
    assert_eq!(expected, actual);
}

/// The Rust `FilesystemRepository::safely_load_installed_versions` is a no-op stub, and
/// `InstalledVersions` has no Rust port at all; both live in the PHP worker, which is where the
/// upstream assertions are checked. See `crates/shirabe/tests/installed_versions_test.rs`.
#[test]
// Serialized because test_repository_writes_installed_php pushes InstalledVersions::reload into
// the same worker, which would replace the state asserted here.
#[serial]
fn test_safely_load_installed_versions() {
    if !php_runtime_available() {
        return;
    }
    load_composer_php_runtime();

    let fixtures_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../composer/tests/Composer/Test/Repository/Fixtures")
        .canonicalize()
        .expect("the Composer checkout must provide the repository fixtures")
        .to_str()
        .unwrap()
        .to_string();

    let result = php_call_static(
        "Composer\\Repository\\FilesystemRepository",
        "safelyLoadInstalledVersions",
        vec![PluginValue::string(
            std::path::Path::new(&fixtures_dir)
                .join("installed_complex.php")
                .to_str()
                .unwrap(),
        )],
    );
    assert_eq!(
        PluginValue::Bool(true),
        result,
        "The file should be considered valid"
    );

    let raw_data = php_call_static("Composer\\InstalledVersions", "getAllRawData", vec![]);
    let PluginValue::List(datasets) = raw_data else {
        panic!("getAllRawData must return a list, got {raw_data:?}")
    };
    let raw_data = datasets.last().cloned().unwrap();

    let root = PhpMixed::Array(IndexMap::from([
        (
            "install_path".into(),
            PhpMixed::string(format!("{}/./", fixtures_dir)),
        ),
        (
            "aliases".into(),
            PhpMixed::List(vec![
                PhpMixed::string("1.10.x-dev".to_string()),
                PhpMixed::string("2.10.x-dev".to_string()),
            ]),
        ),
        ("name".into(), PhpMixed::string("__root__".to_string())),
        ("true".into(), PhpMixed::Bool(true)),
        ("false".into(), PhpMixed::Bool(false)),
        ("null".into(), PhpMixed::Null),
    ]));

    let a_provider = PhpMixed::Array(IndexMap::from([
        (
            "foo".into(),
            PhpMixed::string("simple string/no backslash".to_string()),
        ),
        (
            "install_path".into(),
            PhpMixed::string(format!(
                "{}/vendor/{{${{passthru('bash -i')}}}}",
                fixtures_dir
            )),
        ),
        ("empty array".into(), PhpMixed::List(vec![])),
    ]));

    let c_c = PhpMixed::Array(IndexMap::from([
        (
            "install_path".into(),
            PhpMixed::string("/foo/bar/ven/do{}r/c/c${}".to_string()),
        ),
        ("aliases".into(), PhpMixed::List(vec![])),
        (
            "reference".into(),
            PhpMixed::string(
                "{${passthru('bash -i')}} Foo\\Bar\n\ttab\u{0b}verticaltab\0".to_string(),
            ),
        ),
    ]));

    let expected = PhpMixed::Array(IndexMap::from([
        ("root".into(), root),
        (
            "versions".into(),
            PhpMixed::Array(IndexMap::from([
                ("a/provider".into(), a_provider),
                ("c/c".into(), c_c),
            ])),
        ),
    ]));

    assert_eq!(PluginValue::from_php_mixed(&expected), raw_data);
}
