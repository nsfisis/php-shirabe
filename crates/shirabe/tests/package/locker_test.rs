//! ref: composer/tests/Composer/Test/Package/LockerTest.php

use crate::test_case::installation_manager;
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::json::{JsonEncodeOptions, JsonFile};
use shirabe::package::Locker;
use shirabe::package::handle::{CompletePackageHandle, PackageInterfaceHandle};
use shirabe::plugin::plugin_interface;
use shirabe::repository::{FindPackageConstraint, RepositoryInterfaceHandle};
use shirabe::util::process_executor::ProcessExecutor;
use shirabe_php_shim::{Catch as _, LogicException, PhpMixed, bconcat, hash, trim, utf8_text};
use tempfile::TempDir;

fn null_io() -> std::rc::Rc<std::cell::RefCell<dyn IOInterface>> {
    std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()))
}

/// ref: LockerTest::getJsonContent — `JsonFile::encode(ksort([minimum-stability, name]), 0).unwrap()`.
fn get_json_content(custom_data: &[(&BStr, &BStr)]) -> BString {
    let mut data: IndexMap<BString, PhpMixed> = IndexMap::new();
    data.insert(
        "minimum-stability".into(),
        PhpMixed::string(BString::from("beta")),
    );
    data.insert("name".into(), PhpMixed::string(BString::from("test")));
    for (k, v) in custom_data {
        data.insert(bconcat!(k), PhpMixed::string(bconcat!(v)));
    }
    data.sort_keys();

    JsonFile::encode_with_options(&PhpMixed::Array(data), JsonEncodeOptions::none())
        .unwrap()
        .to_owned()
}

/// Builds a `Locker` backed by a real `composer.lock` `JsonFile` inside a fresh temp dir,
/// replacing PHP's `getMockBuilder(JsonFile)`. When `lock_contents` is `Some`, the lock file is
/// created with the given raw JSON; otherwise no lock file exists (so `exists()` is false).
fn make_locker(
    json_content: &BStr,
    lock_contents: Option<&BStr>,
) -> (
    Locker,
    TempDir,
    std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
) {
    let temp_dir = TempDir::new().unwrap();
    let lock_path = temp_dir.path().join("composer.lock");
    if let Some(contents) = lock_contents {
        std::fs::write(&lock_path, contents).unwrap();
    }

    let io = null_io();
    let json_file = JsonFile::new(&lock_path, None, None).unwrap();
    let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
        io.clone(),
    ))));
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
    let (mut locker, _temp_dir, _io) = make_locker(
        json_content.as_bstr(),
        Some(BStr::new(r#"{"packages": []}"#)),
    );

    assert!(locker.is_locked());
}

#[test]
fn test_get_not_locked_packages() {
    let json_content = get_json_content(&[]);
    // No lock file written => JsonFile::exists() is false.
    let (mut locker, _temp_dir, _io) = make_locker(json_content.as_bstr(), None);

    let err = locker
        .get_locked_repository(false)
        .expect_err("getLockedRepository should fail when no lock file exists");
    assert!(
        err.is_instanceof::<LogicException>(),
        "expected LogicException, got: {err}"
    );
}

#[test]
fn test_get_locked_packages() {
    let json_content = get_json_content(&[]);
    let lock = BStr::new(
        r#"{"packages": [
        {"name": "pkg1", "version": "1.0.0-beta"},
        {"name": "pkg2", "version": "0.1.10"}
    ]}"#,
    );
    let (mut locker, _temp_dir, _io) = make_locker(json_content.as_bstr(), Some(lock));

    let repo: RepositoryInterfaceHandle = locker.get_locked_repository(false).unwrap().into();

    assert!(
        repo.find_package(
            bstr::BStr::new("pkg1"),
            FindPackageConstraint::String(BString::from("1.0.0-beta"))
        )
        .unwrap()
        .is_some()
    );
    assert!(
        repo.find_package(
            bstr::BStr::new("pkg2"),
            FindPackageConstraint::String(BString::from("0.1.10"))
        )
        .unwrap()
        .is_some()
    );
}

#[test]
fn test_set_lock_data() {
    let json_content = bconcat!(get_json_content(&[]), "  ");
    let (mut locker, temp_dir, _io) = make_locker(json_content.as_bstr(), None);

    let package1: PackageInterfaceHandle = CompletePackageHandle::new(
        BString::from("pkg1"),
        BString::from("1.0.0.0"),
        BString::from("1.0.0-beta"),
    )
    .into();
    let package2: PackageInterfaceHandle = CompletePackageHandle::new(
        BString::from("pkg2"),
        BString::from("0.1.10.0"),
        BString::from("0.1.10"),
    )
    .into();

    let content_hash = hash("md5", trim(&json_content));

    let mut platform_overrides: IndexMap<BString, PhpMixed> = IndexMap::new();
    platform_overrides.insert("foo/bar".into(), PhpMixed::string(BString::from("1.0")));

    locker
        .set_lock_data(
            vec![package1, package2],
            Some(vec![]),
            IndexMap::new(),
            IndexMap::new(),
            vec![],
            bstr::BStr::new("dev"),
            IndexMap::new(),
            false,
            false,
            platform_overrides,
            true,
        )
        .unwrap();

    // The real JsonFile writes composer.lock; read it back and assert on the persisted structure,
    // standing in for PHPUnit's `->method('write')->with([...])` expectation.
    let written =
        bstr::BString::from(std::fs::read(temp_dir.path().join("composer.lock")).unwrap());
    let value: serde_json::Value = serde_json::from_slice(&written).unwrap();

    assert_eq!(
        value["content-hash"],
        serde_json::json!(utf8_text(&content_hash).unwrap())
    );
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
        serde_json::json!(utf8_text(plugin_interface::PLUGIN_API_VERSION).unwrap())
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
    let (mut locker, _temp_dir, _io) = make_locker(json_content.as_bstr(), None);

    // PHP mocks a PackageInterface with only getPrettyName()='pkg1' (and an empty pretty version),
    // which makes lockPackages throw a LogicException. A real package with an empty pretty version
    // reproduces that branch.
    let package1: PackageInterfaceHandle = CompletePackageHandle::new(
        BString::from("pkg1"),
        BString::default(),
        BString::default(),
    )
    .into();

    let err = locker
        .set_lock_data(
            vec![package1],
            Some(vec![]),
            IndexMap::new(),
            IndexMap::new(),
            vec![],
            bstr::BStr::new("dev"),
            IndexMap::new(),
            false,
            false,
            IndexMap::new(),
            true,
        )
        .expect_err("setLockData should fail for a package with no version");
    assert!(
        err.is_instanceof::<LogicException>(),
        "expected LogicException, got: {err}"
    );
}

#[test]
fn test_is_fresh() {
    let json_content = get_json_content(&[]);
    let lock = bconcat!(r#"{"hash": ""#, hash("md5", &json_content), r#""}"#);
    let (mut locker, _temp_dir, _io) = make_locker(json_content.as_bstr(), Some(lock.as_bstr()));

    assert!(locker.is_fresh().unwrap());
}

#[test]
fn test_is_fresh_false() {
    let json_content = get_json_content(&[]);
    // PHP stores the *content string* of a different composer.json in the hash field (an obviously
    // non-matching value), so isFresh() is false.
    let other = get_json_content(&[(BStr::new("name"), BStr::new("test2"))]);
    let lock = JsonFile::encode_with_options(
        &PhpMixed::Array({
            let mut m: IndexMap<BString, PhpMixed> = IndexMap::new();
            m.insert("hash".into(), PhpMixed::string(other));
            m
        }),
        JsonEncodeOptions::none(),
    )
    .unwrap();
    let (mut locker, _temp_dir, _io) =
        make_locker(json_content.as_bstr(), Some(lock.to_owned().as_bstr()));

    assert!(!locker.is_fresh().unwrap());
}

#[test]
fn test_is_fresh_with_content_hash() {
    let json_content = get_json_content(&[]);
    let lock = bconcat!(
        r#"{"hash": ""#,
        hash("md5", bconcat!(json_content, "  ")),
        r#"", "content-hash": ""#,
        hash("md5", &json_content),
        r#""}"#
    );
    let (mut locker, _temp_dir, _io) = make_locker(json_content.as_bstr(), Some(lock.as_bstr()));

    assert!(locker.is_fresh().unwrap());
}

#[test]
fn test_is_fresh_with_content_hash_and_no_hash() {
    let json_content = get_json_content(&[]);
    let lock = bconcat!(r#"{"content-hash": ""#, hash("md5", &json_content), r#""}"#);
    let (mut locker, _temp_dir, _io) = make_locker(json_content.as_bstr(), Some(lock.as_bstr()));

    assert!(locker.is_fresh().unwrap());
}

#[test]
fn test_is_fresh_false_with_content_hash() {
    let json_content = get_json_content(&[]);
    let different_hash = hash(
        "md5",
        get_json_content(&[(BStr::new("name"), BStr::new("test2"))]),
    );
    let lock = bconcat!(
        r#"{"hash": ""#,
        different_hash,
        r#"", "content-hash": ""#,
        different_hash,
        r#""}"#
    );
    let (mut locker, _temp_dir, _io) = make_locker(json_content.as_bstr(), Some(lock.as_bstr()));

    assert!(!locker.is_fresh().unwrap());
}
