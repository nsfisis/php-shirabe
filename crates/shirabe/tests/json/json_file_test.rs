//! ref: composer/tests/Composer/Test/Json/JsonFileTest.php sha1:6d5617bb11969b5f9b9cf2e2286c61abce2eb473

use bstr::{B, BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe::json::{JsonEncodeOptions, JsonFile, JsonValidationException};
use shirabe_php_shim::{
    Catch as _, PhpMixed, bformat, error_message, json_decode_assoc, json_decode_obj,
    path_to_bytes, str_replace,
};
use shirabe_seld_json_lint::ParsingException;

/// ref: JsonFileTest::expectParseException
fn expect_parse_exception(text: &BStr, json: &BStr) {
    let err = JsonFile::parse_json(Some(json), None).unwrap_err();
    let message = error_message(&err);
    assert!(
        message.contains_str(text),
        "expected {text:?} in {message:?}"
    );
}

/// ref: JsonFileTest::assertJsonFormat
fn assert_json_format(json: &BStr, data: &PhpMixed, options: Option<JsonEncodeOptions>) {
    let json = str_replace("\r", "", json);
    match options {
        None => assert_eq!(json, JsonFile::encode(data).unwrap()),
        Some(options) => assert_eq!(json, JsonFile::encode_with_options(data, options).unwrap()),
    }
}

/// PHP's JSON_UNESCAPED_UNICODE-only flag set (used by testOnlyUnicode).
fn unescaped_unicode_only() -> JsonEncodeOptions {
    JsonEncodeOptions {
        unescaped_slashes: false,
        pretty_print: false,
        unescaped_unicode: true,
        indent: BString::from(JsonFile::INDENT_DEFAULT),
    }
}

fn fixture_path(name: impl AsRef<std::path::Path>) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../composer/tests/Composer/Test/Json/Fixtures")
        .join(name)
}

/// ref: TestCase::createTempFile.
fn create_temp_file() -> tempfile::NamedTempFile {
    tempfile::NamedTempFile::new().expect("failed to create a temporary file")
}

// INCOMPATIBILITY NOTE: the test_parse_error_* cases below assert serde_json's parse-error wording
// instead of upstream's. Composer (via seld/jsonlint) reports "Parse error on line N:" with
// jsonlint's own line numbering and hint messages; shirabe surfaces serde_json's errors, which flag
// the same syntax errors with different wording and sometimes a different line/column (serde_json
// points at where parsing fails, jsonlint at the offending token). This is a deliberate, permanent
// incompatibility: the seld/jsonlint parser is no longer used for JSON syntax validation.
#[test]
fn test_parse_error_detect_extra_comma() {
    let json = BStr::new("{\n        \"foo\": \"bar\",\n}");
    expect_parse_exception(BStr::new("trailing comma at line 3 column 1"), json);
}

#[test]
fn test_parse_error_detect_extra_comma_in_array() {
    let json = BStr::new("{\n        \"foo\": [\n            \"bar\",\n        ]\n}");
    expect_parse_exception(BStr::new("trailing comma at line 4 column 9"), json);
}

#[test]
fn test_parse_error_detect_unescaped_backslash() {
    let json = BStr::new("{\n        \"fo\\o\": \"bar\"\n}");
    expect_parse_exception(BStr::new("invalid escape at line 2 column 13"), json);
}

#[test]
fn test_parse_error_skips_escaped_backslash() {
    let json = BStr::new("{\n        \"fo\\\\o\": \"bar\"\n        \"a\": \"b\"\n}");
    expect_parse_exception(BStr::new("expected `,` or `}` at line 3 column 9"), json);
}

#[test]
fn test_parse_error_detect_single_quotes() {
    let json = BStr::new("{\n        'foo': \"bar\"\n}");
    expect_parse_exception(BStr::new("key must be a string at line 2 column 9"), json);
}

#[test]
fn test_parse_error_detect_missing_quotes() {
    let json = BStr::new("{\n        foo: \"bar\"\n}");
    expect_parse_exception(BStr::new("key must be a string at line 2 column 9"), json);
}

#[test]
fn test_parse_error_detect_array_as_hash() {
    let json = BStr::new("{\n        \"foo\": [\"bar\": \"baz\"]\n}");
    expect_parse_exception(BStr::new("expected `,` or `]` at line 2 column 22"), json);
}

#[test]
fn test_parse_error_detect_missing_comma() {
    let json = BStr::new("{\n        \"foo\": \"bar\"\n        \"bar\": \"foo\"\n}");
    expect_parse_exception(BStr::new("expected `,` or `}` at line 3 column 9"), json);
}

#[test]
fn test_parse_error_detect_missing_comma_multiline() {
    let json = BStr::new("{\n        \"foo\": \"barbar\"\n\n        \"bar\": \"foo\"\n}");
    expect_parse_exception(BStr::new("expected `,` or `}` at line 4 column 9"), json);
}

#[test]
fn test_parse_error_detect_missing_colon() {
    let json = BStr::new("{\n        \"foo\": \"bar\",\n        \"bar\" \"foo\"\n}");
    expect_parse_exception(BStr::new("expected `:` at line 3 column 15"), json);
}

#[test]
fn test_simple_json_string() {
    let mut data: IndexMap<BString, PhpMixed> = IndexMap::new();
    data.insert(
        "name".into(),
        PhpMixed::string(BString::from("composer/composer")),
    );
    let json = BStr::new("{\n    \"name\": \"composer/composer\"\n}");
    assert_json_format(json, &PhpMixed::Array(data), None);
}

#[test]
fn test_trailing_backslash() {
    let mut data: IndexMap<BString, PhpMixed> = IndexMap::new();
    data.insert("Metadata\\".into(), PhpMixed::string(BString::from("src/")));
    let json = BStr::new("{\n    \"Metadata\\\\\": \"src/\"\n}");
    assert_json_format(json, &PhpMixed::Array(data), None);
}

#[test]
fn test_format_empty_array() {
    let mut data: IndexMap<BString, PhpMixed> = IndexMap::new();
    data.insert("test".into(), PhpMixed::List(vec![]));
    data.insert("test2".into(), PhpMixed::Object(IndexMap::new()));
    let json = BStr::new("{\n    \"test\": [],\n    \"test2\": {}\n}");
    assert_json_format(json, &PhpMixed::Array(data), None);
}

#[test]
fn test_escape() {
    let mut data: IndexMap<BString, PhpMixed> = IndexMap::new();
    data.insert(
        "Metadata\\\"".into(),
        PhpMixed::string(BString::from("src/")),
    );
    let json = BStr::new("{\n    \"Metadata\\\\\\\"\": \"src/\"\n}");
    assert_json_format(json, &PhpMixed::Array(data), None);
}

#[test]
fn test_unicode() {
    let mut data: IndexMap<BString, PhpMixed> = IndexMap::new();
    data.insert(
        "Žluťoučký \" kůň".into(),
        PhpMixed::string(BString::from("úpěl ďábelské ódy za €")),
    );
    let json = BStr::new("{\n    \"Žluťoučký \\\" kůň\": \"úpěl ďábelské ódy za €\"\n}");
    assert_json_format(json, &PhpMixed::Array(data), None);
}

#[test]
fn test_only_unicode() {
    let data = PhpMixed::string(BString::from("\\/ƌ"));
    assert_json_format(
        BStr::new("\"\\\\\\/ƌ\""),
        &data,
        Some(unescaped_unicode_only()),
    );
}

#[test]
fn test_escaped_slashes() {
    let data = PhpMixed::string(BString::from("\\/foo"));
    assert_json_format(
        BStr::new("\"\\\\\\/foo\""),
        &data,
        Some(JsonEncodeOptions::none()),
    );
}

#[test]
fn test_escaped_backslashes() {
    let data = PhpMixed::string(BString::from("a\\b"));
    assert_json_format(
        BStr::new("\"a\\\\b\""),
        &data,
        Some(JsonEncodeOptions::none()),
    );
}

#[test]
fn test_escaped_unicode() {
    let data = PhpMixed::string(BString::from("ƌ"));
    assert_json_format(
        BStr::new("\"\\u018c\""),
        &data,
        Some(JsonEncodeOptions::none()),
    );
}

#[test]
fn test_double_escaped_unicode() {
    let data = PhpMixed::List(vec![
        PhpMixed::string(BString::from("Zdjęcia")),
        PhpMixed::string(BString::from("hjkjhl\\u0119kkjk")),
    ]);
    let encoded_data = JsonFile::encode(&data).unwrap();

    let mut wrapper: IndexMap<BString, PhpMixed> = IndexMap::new();
    wrapper.insert("t".into(), PhpMixed::string(encoded_data));
    let double_encoded_data = JsonFile::encode(&PhpMixed::Array(wrapper)).unwrap();

    let decoded_data = json_decode_assoc(&double_encoded_data).unwrap();
    let t = decoded_data.as_array().unwrap().get(B("t")).unwrap();
    let double_data = json_decode_assoc(t.as_string().unwrap()).unwrap();
    assert_eq!(data, double_data);
}

// Shares the tabs2.json scratch file with test_overwrites_indentation_by_default; PHPUnit runs
// tests serially, so the two never collide upstream.
#[test]
#[serial_test::serial(tabs2_fixture)]
fn test_preserve_indentation_after_read() {
    let src = fixture_path("tabs.json");
    let dst = fixture_path("tabs2.json");
    std::fs::copy(&src, &dst).unwrap();

    let json_file = JsonFile::new(&dst, None, None).unwrap();
    let _data = json_file.read().unwrap();
    let mut hash: IndexMap<BString, PhpMixed> = IndexMap::new();
    hash.insert("foo".into(), PhpMixed::string(BString::from("baz")));
    json_file.write(PhpMixed::Array(hash)).unwrap();

    assert_eq!(
        "{\n\t\"foo\": \"baz\"\n}\n",
        bstr::BString::from(std::fs::read(&dst).unwrap())
    );

    std::fs::remove_file(&dst).unwrap();
}

// See test_preserve_indentation_after_read.
#[test]
#[serial_test::serial(tabs2_fixture)]
fn test_overwrites_indentation_by_default() {
    let src = fixture_path("tabs.json");
    let dst = fixture_path("tabs2.json");
    std::fs::copy(&src, &dst).unwrap();

    let json_file = JsonFile::new(&dst, None, None).unwrap();
    let mut hash: IndexMap<BString, PhpMixed> = IndexMap::new();
    hash.insert("foo".into(), PhpMixed::string(BString::from("baz")));
    json_file.write(PhpMixed::Array(hash)).unwrap();

    assert_eq!(
        "{\n    \"foo\": \"baz\"\n}\n",
        bstr::BString::from(std::fs::read(&dst).unwrap())
    );

    std::fs::remove_file(&dst).unwrap();
}

#[test]
fn test_schema_validation() {
    let path = fixture_path("composer.json");
    let json = JsonFile::new(&path, None, None).unwrap();
    json.validate_schema(JsonFile::STRICT_SCHEMA, None).unwrap();
    json.validate_schema(JsonFile::LAX_SCHEMA, None).unwrap();
}

#[test]
fn test_schema_validation_error() {
    // INCOMPATIBILITY NOTE: upstream asserts justinrainbow's "name : NULL value found, but a string is
    // required". The jsonschema crate reports the same violation with different wording.
    let file_tmp = create_temp_file();
    let file = file_tmp.path().to_path_buf();
    std::fs::write(&file, b"{ \"name\": null }").unwrap();
    let json = JsonFile::new(file.clone(), None, None).unwrap();
    let expected_message = bformat!("\"{file}\" does not match the expected JSON schema");
    let expected_error = BString::from("name : null is not of type \"string\"");

    let err = json
        .validate_schema(JsonFile::STRICT_SCHEMA, None)
        .unwrap_err();
    let e = err.catch::<JsonValidationException>().unwrap();
    assert_eq!(expected_message, e.get_message());
    assert!(
        e.get_errors()
            .contains(&BString::from(expected_error.as_bstr()))
    );

    let err = json
        .validate_schema(JsonFile::LAX_SCHEMA, None)
        .unwrap_err();
    let e = err.catch::<JsonValidationException>().unwrap();
    assert_eq!(expected_message, e.get_message());
    assert!(
        e.get_errors()
            .contains(&BString::from(expected_error.as_bstr()))
    );
}

#[test]
fn test_schema_validation_lax_additional_properties() {
    // INCOMPATIBILITY NOTE: upstream asserts justinrainbow's "The property foo is not defined and the
    // definition does not allow additional properties". The jsonschema crate reports the same
    // additionalProperties violation with different wording.
    let file_tmp = create_temp_file();
    let file = file_tmp.path().to_path_buf();
    std::fs::write(
        &file,
        b"{ \"name\": \"vendor/package\", \"description\": \"generic description\", \"foo\": \"bar\" }",
    )
    .unwrap();
    let json = JsonFile::new(file.clone(), None, None).unwrap();

    let err = json
        .validate_schema(JsonFile::STRICT_SCHEMA, None)
        .unwrap_err();
    let e = err.catch::<JsonValidationException>().unwrap();
    assert_eq!(
        bformat!("\"{file}\" does not match the expected JSON schema"),
        e.get_message()
    );
    assert_eq!(
        &vec![BString::from(
            "Additional properties are not allowed ('foo' was unexpected)"
        )],
        e.get_errors()
    );

    json.validate_schema(JsonFile::LAX_SCHEMA, None).unwrap();
}

#[test]
fn test_schema_validation_lax_required() {
    // INCOMPATIBILITY NOTE: upstream asserts justinrainbow's property-prefixed strings such as
    // "name : The property name is required". The jsonschema crate phrases the message
    // differently ("name : \"name\" is a required property"), but the property prefix is reconstructed,
    // so the "PROPERTY : MESSAGE" shape matches: "name : \"name\" is a required property".
    let file_tmp = create_temp_file();
    let file = file_tmp.path().to_path_buf();
    let json = JsonFile::new(file.clone(), None, None).unwrap();

    let expected_message = bformat!("\"{file}\" does not match the expected JSON schema");

    std::fs::write(&file, b"{ }").unwrap();
    let err = json
        .validate_schema(JsonFile::STRICT_SCHEMA, None)
        .unwrap_err();
    let e = err.catch::<JsonValidationException>().unwrap();
    assert_eq!(expected_message, e.get_message());
    let errors = e.get_errors();
    assert!(errors.contains(&BString::from("name : \"name\" is a required property")));
    assert!(errors.contains(&BString::from(
        "description : \"description\" is a required property"
    )));
    json.validate_schema(JsonFile::LAX_SCHEMA, None).unwrap();

    std::fs::write(&file, b"{ \"name\": \"vendor/package\" }").unwrap();
    let err = json
        .validate_schema(JsonFile::STRICT_SCHEMA, None)
        .unwrap_err();
    let e = err.catch::<JsonValidationException>().unwrap();
    assert_eq!(expected_message, e.get_message());
    assert_eq!(
        &vec![BString::from(
            "description : \"description\" is a required property"
        )],
        e.get_errors()
    );
    json.validate_schema(JsonFile::LAX_SCHEMA, None).unwrap();

    std::fs::write(&file, b"{ \"description\": \"generic description\" }").unwrap();
    let err = json
        .validate_schema(JsonFile::STRICT_SCHEMA, None)
        .unwrap_err();
    let e = err.catch::<JsonValidationException>().unwrap();
    assert_eq!(expected_message, e.get_message());
    assert_eq!(
        &vec![BString::from("name : \"name\" is a required property")],
        e.get_errors()
    );
    json.validate_schema(JsonFile::LAX_SCHEMA, None).unwrap();

    std::fs::write(&file, b"{ \"type\": \"library\" }").unwrap();
    let err = json
        .validate_schema(JsonFile::STRICT_SCHEMA, None)
        .unwrap_err();
    let e = err.catch::<JsonValidationException>().unwrap();
    assert_eq!(expected_message, e.get_message());
    let errors = e.get_errors();
    assert!(errors.contains(&BString::from("name : \"name\" is a required property")));
    assert!(errors.contains(&BString::from(
        "description : \"description\" is a required property"
    )));
    json.validate_schema(JsonFile::LAX_SCHEMA, None).unwrap();

    std::fs::write(&file, b"{ \"type\": \"project\" }").unwrap();
    let err = json
        .validate_schema(JsonFile::STRICT_SCHEMA, None)
        .unwrap_err();
    let e = err.catch::<JsonValidationException>().unwrap();
    assert_eq!(expected_message, e.get_message());
    let errors = e.get_errors();
    assert!(errors.contains(&BString::from("name : \"name\" is a required property")));
    assert!(errors.contains(&BString::from(
        "description : \"description\" is a required property"
    )));
    json.validate_schema(JsonFile::LAX_SCHEMA, None).unwrap();

    std::fs::write(
        &file,
        b"{ \"name\": \"vendor/package\", \"description\": \"generic description\" }",
    )
    .unwrap();
    json.validate_schema(JsonFile::STRICT_SCHEMA, None).unwrap();
    json.validate_schema(JsonFile::LAX_SCHEMA, None).unwrap();
}

#[test]
fn test_custom_schema_validation_lax() {
    let file_tmp = create_temp_file();
    let file = file_tmp.path().to_path_buf();
    std::fs::write(
        &file,
        b"{ \"custom\": \"property\", \"another custom\": \"property\" }",
    )
    .unwrap();

    let schema_tmp = create_temp_file();
    let schema = schema_tmp.path();
    std::fs::write(
        schema,
        b"{ \"properties\": { \"custom\": { \"type\": \"string\" }}}",
    )
    .unwrap();

    let json = JsonFile::new(file, None, None).unwrap();

    json.validate_schema(
        JsonFile::LAX_SCHEMA,
        Some(bstr::BStr::new(path_to_bytes(schema))),
    )
    .unwrap();
}

#[test]
fn test_custom_schema_validation_strict() {
    let file_tmp = create_temp_file();
    let file = file_tmp.path().to_path_buf();
    std::fs::write(&file, b"{ \"custom\": \"property\" }").unwrap();

    let schema_tmp = create_temp_file();
    let schema = schema_tmp.path();
    std::fs::write(
        schema,
        b"{ \"properties\": { \"custom\": { \"type\": \"string\" }}}",
    )
    .unwrap();

    let json = JsonFile::new(file, None, None).unwrap();

    json.validate_schema(
        JsonFile::STRICT_SCHEMA,
        Some(bstr::BStr::new(path_to_bytes(schema))),
    )
    .unwrap();
}

#[test]
fn test_auth_schema_validation_with_custom_data_source() {
    // INCOMPATIBILITY NOTE: upstream asserts justinrainbow's "github-oauth : String value found, but an
    // object is required". The jsonschema crate reports the same type violation with different
    // wording.
    let json = json_decode_obj("{\"github-oauth\": \"foo\"}").unwrap();
    let expected_message =
        BString::from("\"COMPOSER_AUTH\" does not match the expected JSON schema");
    let expected_error = BString::from("github-oauth : \"foo\" is not of type \"object\"");

    let err = JsonFile::validate_json_schema(
        bstr::BStr::new("COMPOSER_AUTH"),
        &json,
        JsonFile::AUTH_SCHEMA,
        None,
    )
    .unwrap_err();
    let e = err.catch::<JsonValidationException>().unwrap();
    assert_eq!(expected_message, e.get_message());
    assert_eq!(&vec![expected_error], e.get_errors());
}

/// ref: shared `$data` for the simple merge-conflict cases.
fn merge_conflict_simple_data() -> PhpMixed {
    let mut data: IndexMap<BString, PhpMixed> = IndexMap::new();
    data.insert(
        "_readme".into(),
        PhpMixed::List(vec![
            PhpMixed::string(
                BString::from("This file locks the dependencies of your project to a known state"),
            ),
            PhpMixed::string(
                BString::from("Read more about it at https://getcomposer.org/doc/01-basic-usage.md#installing-dependencies"),
            ),
            PhpMixed::string(BString::from("This file is @generated automatically")),
        ]),
    );
    data.insert(
        "content-hash".into(),
        PhpMixed::string(BString::from(
            "VCS merge conflict detected. Please run `shirabe update --lock`.",
        )),
    );
    data.insert("packages".into(), PhpMixed::List(vec![]));
    data.insert("packages-dev".into(), PhpMixed::List(vec![]));
    data.insert("aliases".into(), PhpMixed::List(vec![]));
    data.insert(
        "minimum-stability".into(),
        PhpMixed::string(BString::from("stable")),
    );
    data.insert("stability-flags".into(), PhpMixed::List(vec![]));
    data.insert("prefer-stable".into(), PhpMixed::Bool(false));
    data.insert("prefer-lowest".into(), PhpMixed::Bool(false));
    data.insert("platform".into(), PhpMixed::List(vec![]));
    data.insert("platform-dev".into(), PhpMixed::List(vec![]));
    data.insert(
        "plugin-api-version".into(),
        PhpMixed::string(BString::from("2.3.0")),
    );
    PhpMixed::Array(data)
}

#[test]
fn test_composer_lock_file_merge_conflict_simple() {
    let data = merge_conflict_simple_data();
    let json = bstr::BString::from(
        std::fs::read(fixture_path("composer-lock-merge-conflict-simple.txt")).unwrap(),
    );
    assert_eq!(
        data,
        JsonFile::parse_json(Some(&json), Some(bstr::BStr::new("/path/to/composer.lock"))).unwrap()
    );
}

#[test]
fn test_composer_lock_file_merge_conflict_simple_crlf() {
    let data = merge_conflict_simple_data();
    let json = bstr::BString::from(
        std::fs::read(fixture_path("composer-lock-merge-conflict-simple.txt")).unwrap(),
    );
    assert_eq!(
        data,
        JsonFile::parse_json(Some(&json), Some(bstr::BStr::new("/path/to/composer.lock"))).unwrap()
    );
}

#[test]
fn test_composer_lock_file_merge_conflict_complex() {
    // complex files have multiple conflict markers and can thus not be simply resolved
    let data = bstr::BString::from(
        std::fs::read(fixture_path("composer-lock-merge-conflict-complex.txt")).unwrap(),
    );

    let err = JsonFile::parse_json(Some(&data), Some(bstr::BStr::new("/path/to/composer.lock")))
        .unwrap_err();
    assert!(err.is_instanceof::<ParsingException>());
}

#[test]
fn test_composer_lock_file_merge_conflict_complex_crlf() {
    // complex files have multiple conflict markers and can thus not be simply resolved
    let data = bstr::BString::from(
        std::fs::read(fixture_path(
            "composer-lock-merge-conflict-complex-crlf.txt",
        ))
        .unwrap(),
    );

    let err = JsonFile::parse_json(Some(&data), Some(bstr::BStr::new("/path/to/composer.lock")))
        .unwrap_err();
    assert!(err.is_instanceof::<ParsingException>());
}

#[test]
fn test_composer_lock_file_merge_conflict_extended() {
    let data = bstr::BString::from(
        std::fs::read(fixture_path("composer-lock-merge-conflict-extended.txt")).unwrap(),
    );

    let json =
        JsonFile::parse_json(Some(&data), Some(bstr::BStr::new("/path/to/composer.lock"))).unwrap();
    assert_eq!(
        "VCS merge conflict detected. Please run `shirabe update --lock`.",
        json.as_array()
            .unwrap()
            .get(B("content-hash"))
            .unwrap()
            .as_string()
            .unwrap()
    );
}
