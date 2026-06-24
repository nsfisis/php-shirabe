//! ref: composer/tests/Composer/Test/Json/ComposerSchemaTest.php
//!
//! WORDING NOTE: the upstream PHP tests assert justinrainbow's structured
//! `{property, message, constraint}` error arrays (e.g. "Does not match the regex pattern …",
//! "Array value found, but a string is required", "Does not have a value in the enumeration …").
//! Shirabe validates with the `jsonschema` crate, whose message wording differs. These ports
//! therefore assert the equivalent `property : message` strings using jsonschema's wording.
//! Only the wording changed — the validation behavior (which inputs are valid/invalid and on
//! which property) is identical to upstream.

use shirabe::json::{JsonFile, JsonValidationException};
use shirabe_php_shim::json_decode;

const NAME_PATTERN: &str = r#"^[a-z0-9]([_.-]?[a-z0-9]+)*/[a-z0-9](([_.]|-{1,2})?[a-z0-9]+)*$"#;
const VERSION_PATTERN: &str = r#"^[vV]?\d+(?:[.-]\d+){0,3}[._-]?(?:(?:[sS][tT][aA][bB][lL][eE]|[bB][eE][tT][aA]|[bB]|[rR][cC]|[aA][lL][pP][hH][aA]|[aA]|[pP][aA][tT][cC][hH]|[pP][lL]|[pP])(?:(?:[.-]?\d+)*)?)?(?:[.-]?[dD][eE][vV]|\.x-dev)?(?:\+.*)?$|^dev-.*$"#;

/// Ports `ComposerSchemaTest::check`: validate against the bundled `composer-schema.json`
/// (the `{"$ref": "file://…"}` wrapper used by `LAX_SCHEMA`), returning the validation error
/// strings, or an empty vec when the document is valid.
fn check(json: &str) -> Vec<String> {
    let data = json_decode(json, false).unwrap();
    match JsonFile::validate_json_schema("test", &data, JsonFile::LAX_SCHEMA, None) {
        Ok(_) => Vec::new(),
        Err(e) => e
            .downcast_ref::<JsonValidationException>()
            .unwrap()
            .get_errors()
            .clone(),
    }
}

#[test]
fn test_name_pattern() {
    let expected_error = vec![format!(
        r#"name : "vendor/-pack__age" does not match "{NAME_PATTERN}""#
    )];
    let json = r#"{"name": "vendor/-pack__age", "description": "description"}"#;
    assert_eq!(expected_error, check(json));

    let expected_error = vec![format!(
        r#"name : "Vendor/Package" does not match "{NAME_PATTERN}""#
    )];
    let json = r#"{"name": "Vendor/Package", "description": "description"}"#;
    assert_eq!(expected_error, check(json));
}

#[test]
fn test_version_pattern() {
    let versions: &[(&str, bool)] = &[
        ("1.0.0", true),
        ("1.0.2", true),
        ("1.1.0", true),
        ("1.0.0-dev", true),
        ("1.0.0-Alpha", true),
        ("1.0.0-ALPHA", true),
        ("1.0.0-alphA", true),
        ("1.0.0-alpha3", true),
        ("1.0.0-Alpha3", true),
        ("1.0.0-ALPHA3", true),
        ("1.0.0-Beta", true),
        ("1.0.0-BETA", true),
        ("1.0.0-betA", true),
        ("1.0.0-beta232", true),
        ("1.0.0-Beta232", true),
        ("1.0.0-BETA232", true),
        ("10.4.13beta.2", true),
        ("1.0.0.RC.15-dev", true),
        ("1.0.0-RC", true),
        ("v2.0.4-p", true),
        ("dev-master", true),
        ("0.2.5.4", true),
        ("12345678-123456", true),
        ("20100102-203040-p1", true),
        ("2010-01-02.5", true),
        ("0.2.5.4-rc.2", true),
        ("dev-feature+issue-1", true),
        ("1.0.0-alpha.3.1+foo/-bar", true),
        ("00.01.03.04", true),
        ("041.x-dev", true),
        ("dev-foo bar", true),
        ("invalid", false),
        ("1.0be", false),
        ("1.0.0-meh", false),
        ("feature-foo", false),
        ("1.0 .2", false),
    ];

    for &(version, is_valid) in versions {
        let json = format!(
            r#"{{"name": "vendor/package", "description": "description", "version": "{version}"}}"#
        );
        if is_valid {
            assert!(check(&json).is_empty(), "expected {version} to be valid");
        } else {
            let expected_error = vec![format!(
                r#"version : "{version}" does not match "{VERSION_PATTERN}""#
            )];
            assert_eq!(
                expected_error,
                check(&json),
                "expected {version} to be invalid"
            );
        }
    }
}

#[test]
fn test_optional_abandoned_property() {
    let json = r#"{"name": "vendor/package", "description": "description", "abandoned": true}"#;
    assert!(check(json).is_empty());
}

#[test]
fn test_require_types() {
    let json =
        r#"{"name": "vendor/package", "description": "description", "require": {"a": ["b"]} }"#;
    let expected_error = vec![r#"require.a : ["b"] is not of type "string""#.to_string()];
    assert_eq!(expected_error, check(json));
}

#[test]
fn test_minimum_stability_values() {
    let enum_error = |value: &str| {
        vec![format!(
            r#"minimum-stability : "{value}" is not one of "dev", "alpha" or 4 other candidates"#
        )]
    };

    let json = r#"{ "name": "vendor/package", "description": "generic description", "minimum-stability": "" }"#;
    assert_eq!(enum_error(""), check(json), "empty string");

    let json = r#"{ "name": "vendor/package", "description": "generic description", "minimum-stability": "dummy" }"#;
    assert_eq!(enum_error("dummy"), check(json), "dummy");

    let json = r#"{ "name": "vendor/package", "description": "generic description", "minimum-stability": "devz" }"#;
    assert_eq!(enum_error("devz"), check(json), "devz");

    for stability in ["dev", "alpha", "beta", "rc", "RC", "stable"] {
        let json = format!(
            r#"{{ "name": "vendor/package", "description": "generic description", "minimum-stability": "{stability}" }}"#
        );
        assert!(check(&json).is_empty(), "expected {stability} to be valid");
    }
}
