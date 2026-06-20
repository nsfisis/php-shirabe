//! ref: composer/tests/Composer/Test/Util/ZipTest.php

use shirabe::util::zip::Zip;

/// Reuses the PHP fixtures under composer/tests/Composer/Test/Util/Fixtures/Zip.
fn fixture(name: &str) -> String {
    format!(
        "{}/../../composer/tests/Composer/Test/Util/Fixtures/Zip/{}",
        env!("CARGO_MANIFEST_DIR"),
        name
    )
}

#[test]
#[ignore = "skipped in PHP whenever the zip extension is loaded, which it always is here"]
fn test_throws_exception_if_zip_extension_is_not_loaded() {
    assert!(Zip::get_composer_json("").is_err());
}

#[test]
#[ignore = "ZipArchive::new is todo!() in the php-shim"]
fn test_returns_nullif_the_zip_is_not_found() {
    let result = Zip::get_composer_json(&fixture("invalid.zip")).unwrap();

    assert_eq!(None, result);
}

#[test]
#[ignore = "ZipArchive::new is todo!() in the php-shim"]
fn test_returns_null_if_the_zip_is_empty() {
    let result = Zip::get_composer_json(&fixture("empty.zip")).unwrap();

    assert_eq!(None, result);
}

#[test]
#[ignore = "ZipArchive::new is todo!() in the php-shim"]
fn test_throws_exception_if_the_zip_has_no_composer_json() {
    assert!(Zip::get_composer_json(&fixture("nojson.zip")).is_err());
}

#[test]
#[ignore = "ZipArchive::new is todo!() in the php-shim"]
fn test_throws_exception_if_the_composer_json_is_in_a_sub_subfolder() {
    assert!(Zip::get_composer_json(&fixture("subfolders.zip")).is_err());
}

#[test]
#[ignore = "ZipArchive::new is todo!() in the php-shim"]
fn test_returns_composer_json_in_zip_root() {
    let result = Zip::get_composer_json(&fixture("root.zip")).unwrap();

    assert_eq!(Some("{\n    \"name\": \"foo/bar\"\n}\n".to_string()), result);
}

#[test]
#[ignore = "ZipArchive::new is todo!() in the php-shim"]
fn test_returns_composer_json_in_first_folder() {
    let result = Zip::get_composer_json(&fixture("folder.zip")).unwrap();

    assert_eq!(Some("{\n    \"name\": \"foo/bar\"\n}\n".to_string()), result);
}

#[test]
#[ignore = "ZipArchive::new is todo!() in the php-shim"]
fn test_multiple_top_level_dirs_is_invalid() {
    assert!(Zip::get_composer_json(&fixture("multiple.zip")).is_err());
}

#[test]
#[ignore = "ZipArchive::new is todo!() in the php-shim"]
fn test_returns_composer_json_from_first_subfolder() {
    let result = Zip::get_composer_json(&fixture("single-sub.zip")).unwrap();

    assert_eq!(Some("{\n    \"name\": \"foo/bar\"\n}\n".to_string()), result);
}
