//! ref: composer/tests/Composer/Test/Util/TarTest.php

use shirabe::util::tar::Tar;

/// Reuses the PHP fixtures under composer/tests/Composer/Test/Util/Fixtures/Tar.
fn fixture(name: &str) -> String {
    format!(
        "{}/../../composer/tests/Composer/Test/Util/Fixtures/Tar/{}",
        env!("CARGO_MANIFEST_DIR"),
        name
    )
}

#[test]
#[ignore = "PharData::new is todo!() in the php-shim"]
fn test_returns_nullif_the_tar_is_not_found() {
    let result = Tar::get_composer_json(&fixture("invalid.zip")).unwrap();

    assert_eq!(None, result);
}

#[test]
#[ignore = "PharData::new is todo!() in the php-shim"]
fn test_returns_null_if_the_tar_is_empty() {
    let result = Tar::get_composer_json(&fixture("empty.tar.gz")).unwrap();
    assert_eq!(None, result);
}

#[test]
#[ignore = "PharData::new is todo!() in the php-shim"]
fn test_throws_exception_if_the_tar_has_no_composer_json() {
    assert!(Tar::get_composer_json(&fixture("nojson.tar.gz")).is_err());
}

#[test]
#[ignore = "PharData::new is todo!() in the php-shim"]
fn test_throws_exception_if_the_composer_json_is_in_a_sub_subfolder() {
    assert!(Tar::get_composer_json(&fixture("subfolders.tar.gz")).is_err());
}

#[test]
#[ignore = "PharData::new is todo!() in the php-shim"]
fn test_returns_composer_json_in_tar_root() {
    let result = Tar::get_composer_json(&fixture("root.tar.gz")).unwrap();
    assert_eq!(Some("{\n    \"name\": \"foo/bar\"\n}\n".to_string()), result);
}

#[test]
#[ignore = "PharData::new is todo!() in the php-shim"]
fn test_returns_composer_json_in_first_folder() {
    let result = Tar::get_composer_json(&fixture("folder.tar.gz")).unwrap();
    assert_eq!(Some("{\n    \"name\": \"foo/bar\"\n}\n".to_string()), result);
}

#[test]
#[ignore = "PharData::new is todo!() in the php-shim"]
fn test_multiple_top_level_dirs_is_invalid() {
    assert!(Tar::get_composer_json(&fixture("multiple.tar.gz")).is_err());
}
