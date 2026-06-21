//! ref: composer/tests/Composer/Test/Repository/FilesystemRepositoryTest.php

use indexmap::IndexMap;
use shirabe::installed_versions::InstalledVersions;
use shirabe::json::json_file::JsonFile;
use shirabe::repository::RepositoryInterface;
use shirabe::repository::filesystem_repository::FilesystemRepository;
use shirabe_php_shim::PhpMixed;

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

#[ignore]
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

#[ignore]
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
        err.is::<shirabe::repository::InvalidRepositoryException>(),
        "expected InvalidRepositoryException, got: {err}"
    );
}

#[ignore]
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

#[test]
#[ignore = "requires mocking InstallationManager::get_install_path; write() takes a concrete InstallationManager with no trait/seam to stub the canned per-package paths the PHP test relies on"]
fn test_repository_write() {
    todo!()
}

#[test]
#[ignore = "requires mocking InstallationManager::get_install_path (concrete method, no stub seam) plus missing test helpers get_root_package and configure_links"]
fn test_repository_writes_installed_php() {
    todo!()
}

#[ignore]
#[test]
fn test_safely_load_installed_versions() {
    let fixtures_dir = format!(
        "{}/../../composer/tests/Composer/Test/Repository/Fixtures",
        env!("CARGO_MANIFEST_DIR")
    );
    let path = format!("{}/installed_complex.php", fixtures_dir);

    let result = FilesystemRepository::safely_load_installed_versions(&path);
    assert!(result, "The file should be considered valid");

    let raw_data = InstalledVersions::get_all_raw_data();
    let raw_data = raw_data.last().cloned().unwrap();

    let mut root: IndexMap<String, PhpMixed> = IndexMap::new();
    root.insert(
        "install_path".to_string(),
        PhpMixed::String(format!("{}/./", fixtures_dir)),
    );
    root.insert(
        "aliases".to_string(),
        PhpMixed::List(vec![
            PhpMixed::String("1.10.x-dev".to_string()),
            PhpMixed::String("2.10.x-dev".to_string()),
        ]),
    );
    root.insert("name".to_string(), PhpMixed::String("__root__".to_string()));
    root.insert("true".to_string(), PhpMixed::Bool(true));
    root.insert("false".to_string(), PhpMixed::Bool(false));
    root.insert("null".to_string(), PhpMixed::Null);

    let mut a_provider: IndexMap<String, PhpMixed> = IndexMap::new();
    a_provider.insert(
        "foo".to_string(),
        PhpMixed::String("simple string/no backslash".to_string()),
    );
    a_provider.insert(
        "install_path".to_string(),
        PhpMixed::String(format!(
            "{}/vendor/{{${{passthru('bash -i')}}}}",
            fixtures_dir
        )),
    );
    a_provider.insert("empty array".to_string(), PhpMixed::List(vec![]));

    let mut c_c: IndexMap<String, PhpMixed> = IndexMap::new();
    c_c.insert(
        "install_path".to_string(),
        PhpMixed::String("/foo/bar/ven/do{}r/c/c${}".to_string()),
    );
    c_c.insert("aliases".to_string(), PhpMixed::List(vec![]));
    c_c.insert(
        "reference".to_string(),
        PhpMixed::String("{${passthru('bash -i')}} Foo\\Bar\n\ttab\u{0b}verticaltab\0".to_string()),
    );

    let mut versions: IndexMap<String, PhpMixed> = IndexMap::new();
    versions.insert("a/provider".to_string(), PhpMixed::Array(a_provider));
    versions.insert("c/c".to_string(), PhpMixed::Array(c_c));

    let mut expected: IndexMap<String, PhpMixed> = IndexMap::new();
    expected.insert("root".to_string(), PhpMixed::Array(root));
    expected.insert("versions".to_string(), PhpMixed::Array(versions));

    assert_eq!(raw_data, expected);
}
