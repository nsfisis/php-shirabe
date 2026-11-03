//! ref: composer/tests/Composer/Test/Json/ComposerSchemaTest.php
//!
//! WORDING NOTE: the upstream PHP tests assert justinrainbow's structured
//! `{property, message, constraint}` error arrays (e.g. "Does not match the regex pattern …",
//! "Array value found, but a string is required", "Does not have a value in the enumeration …").
//! Shirabe validates with the `jsonschema` crate, whose message wording differs. These ports
//! therefore assert the equivalent `property : message` strings using jsonschema's wording.
//! Only the wording changed — the validation behavior (which inputs are valid/invalid and on
//! which property) is identical to upstream.

use bstr::{BStr, BString, ByteSlice};
use shirabe::json::{JsonFile, JsonValidationException};
use shirabe_php_shim::{Catch as _, bconcat, json_decode_obj, php_str};

const NAME_PATTERN: &BStr =
    php_str(r#"^[a-z0-9]([_.-]?[a-z0-9]+)*/[a-z0-9](([_.]|-{1,2})?[a-z0-9]+)*$"#.as_bytes());
const VERSION_PATTERN: &BStr = php_str(r#"^[vV]?\d+(?:[.-]\d+){0,3}[._-]?(?:(?:[sS][tT][aA][bB][lL][eE]|[bB][eE][tT][aA]|[bB]|[rR][cC]|[aA][lL][pP][hH][aA]|[aA]|[pP][aA][tT][cC][hH]|[pP][lL]|[pP])(?:(?:[.-]?\d+)*)?)?(?:[.-]?[dD][eE][vV]|\.x-dev)?(?:\+.*)?$|^dev-.*$"#.as_bytes());

/// Ports `ComposerSchemaTest::check`: validate against the bundled `composer-schema.json`
/// (the `{"$ref": "file://…"}` wrapper used by `LAX_SCHEMA`), returning the validation error
/// strings, or an empty vec when the document is valid.
fn check(json: &BStr) -> Vec<BString> {
    let data = json_decode_obj(json).unwrap();
    match JsonFile::validate_json_schema(bstr::BStr::new("test"), &data, JsonFile::LAX_SCHEMA, None)
    {
        Ok(_) => Vec::new(),
        Err(e) => e
            .catch::<JsonValidationException>()
            .unwrap()
            .get_errors()
            .iter()
            .map(|e| e.to_owned())
            .collect(),
    }
}

#[test]
fn test_name_pattern() {
    let expected_error = vec![bconcat!(
        r#"name : "vendor/-pack__age" does not match ""#,
        NAME_PATTERN,
        r#"""#
    )];
    let json = BStr::new(r#"{"name": "vendor/-pack__age", "description": "description"}"#);
    assert_eq!(expected_error, check(json));

    let expected_error = vec![bconcat!(
        r#"name : "Vendor/Package" does not match ""#,
        NAME_PATTERN,
        r#"""#
    )];
    let json = BStr::new(r#"{"name": "Vendor/Package", "description": "description"}"#);
    assert_eq!(expected_error, check(json));
}

#[test]
fn test_version_pattern() {
    let versions: &[(&BStr, bool)] = &[
        (BStr::new("1.0.0"), true),
        (BStr::new("1.0.2"), true),
        (BStr::new("1.1.0"), true),
        (BStr::new("1.0.0-dev"), true),
        (BStr::new("1.0.0-Alpha"), true),
        (BStr::new("1.0.0-ALPHA"), true),
        (BStr::new("1.0.0-alphA"), true),
        (BStr::new("1.0.0-alpha3"), true),
        (BStr::new("1.0.0-Alpha3"), true),
        (BStr::new("1.0.0-ALPHA3"), true),
        (BStr::new("1.0.0-Beta"), true),
        (BStr::new("1.0.0-BETA"), true),
        (BStr::new("1.0.0-betA"), true),
        (BStr::new("1.0.0-beta232"), true),
        (BStr::new("1.0.0-Beta232"), true),
        (BStr::new("1.0.0-BETA232"), true),
        (BStr::new("10.4.13beta.2"), true),
        (BStr::new("1.0.0.RC.15-dev"), true),
        (BStr::new("1.0.0-RC"), true),
        (BStr::new("v2.0.4-p"), true),
        (BStr::new("dev-master"), true),
        (BStr::new("0.2.5.4"), true),
        (BStr::new("12345678-123456"), true),
        (BStr::new("20100102-203040-p1"), true),
        (BStr::new("2010-01-02.5"), true),
        (BStr::new("0.2.5.4-rc.2"), true),
        (BStr::new("dev-feature+issue-1"), true),
        (BStr::new("1.0.0-alpha.3.1+foo/-bar"), true),
        (BStr::new("00.01.03.04"), true),
        (BStr::new("041.x-dev"), true),
        (BStr::new("dev-foo bar"), true),
        (BStr::new("invalid"), false),
        (BStr::new("1.0be"), false),
        (BStr::new("1.0.0-meh"), false),
        (BStr::new("feature-foo"), false),
        (BStr::new("1.0 .2"), false),
    ];

    for &(version, is_valid) in versions {
        let json = bconcat!(
            r#"{"name": "vendor/package", "description": "description", "version": ""#,
            version,
            r#""}"#
        );
        if is_valid {
            assert!(
                check(json.as_bstr()).is_empty(),
                "expected {version} to be valid",
                version = version.display_lossy()
            );
        } else {
            let expected_error = vec![bconcat!(
                r#"version : ""#,
                version,
                r#"" does not match ""#,
                VERSION_PATTERN,
                r#"""#
            )];
            assert_eq!(
                expected_error,
                check(json.as_bstr()),
                "expected {version} to be invalid",
                version = version.display_lossy()
            );
        }
    }
}

#[test]
fn test_optional_abandoned_property() {
    let json =
        BStr::new(r#"{"name": "vendor/package", "description": "description", "abandoned": true}"#);
    assert!(check(json).is_empty());
}

#[test]
fn test_require_types() {
    let json = BStr::new(
        r#"{"name": "vendor/package", "description": "description", "require": {"a": ["b"]} }"#,
    );
    let expected_error = vec![BString::from(
        r#"require.a : ["b"] is not of type "string""#,
    )];
    assert_eq!(expected_error, check(json));
}

#[test]
fn test_minimum_stability_values() {
    let enum_error = |value: &BStr| {
        vec![bconcat!(
            r#"minimum-stability : ""#,
            value,
            r#"" is not one of "dev", "alpha" or 4 other candidates"#
        )]
    };

    let json = BStr::new(
        r#"{ "name": "vendor/package", "description": "generic description", "minimum-stability": "" }"#,
    );
    assert_eq!(enum_error(BStr::new("")), check(json), "empty string");

    let json = BStr::new(
        r#"{ "name": "vendor/package", "description": "generic description", "minimum-stability": "dummy" }"#,
    );
    assert_eq!(enum_error(BStr::new("dummy")), check(json), "dummy");

    let json = BStr::new(
        r#"{ "name": "vendor/package", "description": "generic description", "minimum-stability": "devz" }"#,
    );
    assert_eq!(enum_error(BStr::new("devz")), check(json), "devz");

    for stability in ["dev", "alpha", "beta", "rc", "RC", "stable"] {
        let json = bconcat!(
            r#"{ "name": "vendor/package", "description": "generic description", "minimum-stability": ""#,
            stability,
            r#"" }"#
        );
        assert!(
            check(json.as_bstr()).is_empty(),
            "expected {stability} to be valid"
        );
    }
}
