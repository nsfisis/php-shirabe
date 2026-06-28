//! ref: composer/tests/Composer/Test/Package/LockerTest.php

use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::installer::InstallationManager;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::json::{JsonEncodeOptions, JsonFile};
use shirabe::package::Locker;
use shirabe::package::handle::{CompletePackageHandle, PackageInterfaceHandle};
use shirabe::plugin::plugin_interface;
use shirabe::repository::{FindPackageConstraint, RepositoryInterfaceHandle};
use shirabe::util::http_downloader::HttpDownloader;
use shirabe::util::r#loop::Loop;
use shirabe::util::process_executor::ProcessExecutor;
use shirabe_php_shim::{LogicException, PhpMixed, hash};
use std::cell::RefCell;
use std::rc::Rc;
use tempfile::TempDir;

fn null_io() -> Rc<RefCell<dyn IOInterface>> {
    Rc::new(RefCell::new(NullIO::new()))
}

fn installation_manager(io: &Rc<RefCell<dyn IOInterface>>) -> Rc<RefCell<InstallationManager>> {
    // These tests never reach Locker::get_package_time, so the InstallationManager is never
    // actually used; build it over a mock HttpDownloader to avoid the unimplemented curl backend.
    let config = Rc::new(RefCell::new(Config::new(false, None)));
    let http_downloader = Rc::new(RefCell::new(HttpDownloader::__new_mock(io.clone(), config)));
    let r#loop = Rc::new(RefCell::new(Loop::new(http_downloader, None)));
    Rc::new(RefCell::new(InstallationManager::new(
        r#loop,
        io.clone(),
        None,
    )))
}

/// ref: LockerTest::getJsonContent — `JsonFile::encode(ksort([minimum-stability, name]), 0)`.
fn get_json_content(custom_data: &[(&str, &str)]) -> String {
    let mut data: IndexMap<String, PhpMixed> = IndexMap::new();
    data.insert(
        "minimum-stability".to_string(),
        PhpMixed::String("beta".to_string()),
    );
    data.insert("name".to_string(), PhpMixed::String("test".to_string()));
    for (k, v) in custom_data {
        data.insert(k.to_string(), PhpMixed::String(v.to_string()));
    }
    data.sort_keys();

    JsonFile::encode_with_options(&PhpMixed::Array(data), JsonEncodeOptions::none())
}

/// Builds a `Locker` backed by a real `composer.lock` `JsonFile` inside a fresh temp dir,
/// replacing PHP's `getMockBuilder(JsonFile)`. When `lock_contents` is `Some`, the lock file is
/// created with the given raw JSON; otherwise no lock file exists (so `exists()` is false).
fn make_locker(
    json_content: &str,
    lock_contents: Option<&str>,
) -> (Locker, TempDir, Rc<RefCell<dyn IOInterface>>) {
    let temp_dir = TempDir::new().unwrap();
    let lock_path = temp_dir.path().join("composer.lock");
    if let Some(contents) = lock_contents {
        std::fs::write(&lock_path, contents).unwrap();
    }

    let io = null_io();
    let json_file = JsonFile::new(lock_path.to_string_lossy().into_owned(), None, None).unwrap();
    let process = Rc::new(RefCell::new(ProcessExecutor::new(Some(io.clone()))));
    let locker = Locker::new(
        io.clone(),
        json_file,
        installation_manager(&io),
        json_content,
        process,
    );
    (locker, temp_dir, io)
}

#[test]
fn test_is_locked() {
    let json_content = get_json_content(&[]);
    let (mut locker, _temp_dir, _io) = make_locker(&json_content, Some(r#"{"packages": []}"#));

    assert!(locker.is_locked());
}

#[test]
fn test_get_not_locked_packages() {
    let json_content = get_json_content(&[]);
    // No lock file written => JsonFile::exists() is false.
    let (mut locker, _temp_dir, _io) = make_locker(&json_content, None);

    let err = locker
        .get_locked_repository(false)
        .expect_err("getLockedRepository should fail when no lock file exists");
    assert!(
        err.downcast_ref::<LogicException>().is_some(),
        "expected LogicException, got: {err}"
    );
}

#[test]
fn test_get_locked_packages() {
    let json_content = get_json_content(&[]);
    let lock = r#"{"packages": [
        {"name": "pkg1", "version": "1.0.0-beta"},
        {"name": "pkg2", "version": "0.1.10"}
    ]}"#;
    let (mut locker, _temp_dir, _io) = make_locker(&json_content, Some(lock));

    let repo: RepositoryInterfaceHandle = locker.get_locked_repository(false).unwrap().into();

    assert!(
        repo.find_package(
            "pkg1",
            FindPackageConstraint::String("1.0.0-beta".to_string())
        )
        .unwrap()
        .is_some()
    );
    assert!(
        repo.find_package("pkg2", FindPackageConstraint::String("0.1.10".to_string()))
            .unwrap()
            .is_some()
    );
}

#[test]
fn test_set_lock_data() {
    let json_content = format!("{}  ", get_json_content(&[]));
    let (mut locker, temp_dir, _io) = make_locker(&json_content, None);

    let package1: PackageInterfaceHandle = CompletePackageHandle::new(
        "pkg1".to_string(),
        "1.0.0.0".to_string(),
        "1.0.0-beta".to_string(),
    )
    .into();
    let package2: PackageInterfaceHandle = CompletePackageHandle::new(
        "pkg2".to_string(),
        "0.1.10.0".to_string(),
        "0.1.10".to_string(),
    )
    .into();

    let content_hash = hash("md5", json_content.trim());

    let mut platform_overrides: IndexMap<String, PhpMixed> = IndexMap::new();
    platform_overrides.insert("foo/bar".to_string(), PhpMixed::String("1.0".to_string()));

    locker
        .set_lock_data(
            vec![package1, package2],
            Some(vec![]),
            IndexMap::new(),
            IndexMap::new(),
            vec![],
            "dev",
            IndexMap::new(),
            false,
            false,
            platform_overrides,
            true,
        )
        .unwrap();

    // The real JsonFile writes composer.lock; read it back and assert on the persisted structure,
    // standing in for PHPUnit's `->method('write')->with([...])` expectation.
    let written = std::fs::read_to_string(temp_dir.path().join("composer.lock")).unwrap();
    let value: serde_json::Value = serde_json::from_str(&written).unwrap();

    assert_eq!(value["content-hash"], serde_json::json!(content_hash));
    assert_eq!(
        value["packages"],
        serde_json::json!([
            {"name": "pkg1", "version": "1.0.0-beta", "type": "library"},
            {"name": "pkg2", "version": "0.1.10", "type": "library"},
        ])
    );
    assert_eq!(value["packages-dev"], serde_json::json!([]));
    assert_eq!(value["aliases"], serde_json::json!([]));
    assert_eq!(value["minimum-stability"], serde_json::json!("dev"));
    assert_eq!(value["stability-flags"], serde_json::json!({}));
    assert_eq!(value["platform"], serde_json::json!({}));
    assert_eq!(value["platform-dev"], serde_json::json!({}));
    assert_eq!(
        value["platform-overrides"],
        serde_json::json!({"foo/bar": "1.0"})
    );
    assert_eq!(value["prefer-stable"], serde_json::json!(false));
    assert_eq!(value["prefer-lowest"], serde_json::json!(false));
    assert_eq!(
        value["plugin-api-version"],
        serde_json::json!(plugin_interface::PLUGIN_API_VERSION)
    );
    assert_eq!(
        value["_readme"],
        serde_json::json!([
            "This file locks the dependencies of your project to a known state",
            "Read more about it at https://getcomposer.org/doc/01-basic-usage.md#installing-dependencies",
            "This file is @generated automatically",
        ])
    );
}

#[test]
fn test_lock_bad_packages() {
    let json_content = get_json_content(&[]);
    let (mut locker, _temp_dir, _io) = make_locker(&json_content, None);

    // PHP mocks a PackageInterface with only getPrettyName()='pkg1' (and an empty pretty version),
    // which makes lockPackages throw a LogicException. A real package with an empty pretty version
    // reproduces that branch.
    let package1: PackageInterfaceHandle =
        CompletePackageHandle::new("pkg1".to_string(), String::new(), String::new()).into();

    let err = locker
        .set_lock_data(
            vec![package1],
            Some(vec![]),
            IndexMap::new(),
            IndexMap::new(),
            vec![],
            "dev",
            IndexMap::new(),
            false,
            false,
            IndexMap::new(),
            true,
        )
        .expect_err("setLockData should fail for a package with no version");
    assert!(
        err.downcast_ref::<LogicException>().is_some(),
        "expected LogicException, got: {err}"
    );
}

#[test]
fn test_is_fresh() {
    let json_content = get_json_content(&[]);
    let lock = format!(r#"{{"hash": "{}"}}"#, hash("md5", &json_content));
    let (mut locker, _temp_dir, _io) = make_locker(&json_content, Some(&lock));

    assert!(locker.is_fresh().unwrap());
}

#[test]
fn test_is_fresh_false() {
    let json_content = get_json_content(&[]);
    // PHP stores the *content string* of a different composer.json in the hash field (an obviously
    // non-matching value), so isFresh() is false.
    let other = get_json_content(&[("name", "test2")]);
    let lock = JsonFile::encode_with_options(
        &PhpMixed::Array({
            let mut m: IndexMap<String, PhpMixed> = IndexMap::new();
            m.insert("hash".to_string(), PhpMixed::String(other));
            m
        }),
        JsonEncodeOptions::none(),
    );
    let (mut locker, _temp_dir, _io) = make_locker(&json_content, Some(&lock));

    assert!(!locker.is_fresh().unwrap());
}

#[test]
fn test_is_fresh_with_content_hash() {
    let json_content = get_json_content(&[]);
    let lock = format!(
        r#"{{"hash": "{}", "content-hash": "{}"}}"#,
        hash("md5", &format!("{}  ", json_content)),
        hash("md5", &json_content),
    );
    let (mut locker, _temp_dir, _io) = make_locker(&json_content, Some(&lock));

    assert!(locker.is_fresh().unwrap());
}

#[test]
fn test_is_fresh_with_content_hash_and_no_hash() {
    let json_content = get_json_content(&[]);
    let lock = format!(r#"{{"content-hash": "{}"}}"#, hash("md5", &json_content));
    let (mut locker, _temp_dir, _io) = make_locker(&json_content, Some(&lock));

    assert!(locker.is_fresh().unwrap());
}

#[test]
fn test_is_fresh_false_with_content_hash() {
    let json_content = get_json_content(&[]);
    let different_hash = hash("md5", &get_json_content(&[("name", "test2")]));
    let lock = format!(
        r#"{{"hash": "{}", "content-hash": "{}"}}"#,
        different_hash, different_hash,
    );
    let (mut locker, _temp_dir, _io) = make_locker(&json_content, Some(&lock));

    assert!(!locker.is_fresh().unwrap());
}
