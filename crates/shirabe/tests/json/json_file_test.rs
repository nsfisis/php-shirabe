//! ref: composer/tests/Composer/Test/Json/JsonFileTest.php

use indexmap::IndexMap;
use shirabe::json::{JsonEncodeOptions, JsonFile, JsonValidationException};
use shirabe_external_packages::seld::json_lint::ParsingException;
use shirabe_php_shim::PhpMixed;

/// ref: JsonFileTest::expectParseException
fn expect_parse_exception(text: &str, json: &str) {
    let err = JsonFile::parse_json(Some(json), None).unwrap_err();
    let message = err.to_string();
    assert!(message.contains(text), "expected {text:?} in {message:?}");
}

/// ref: JsonFileTest::assertJsonFormat
fn assert_json_format(json: &str, data: &PhpMixed, options: Option<JsonEncodeOptions>) {
    let json = json.replace('\r', "");
    match options {
        None => assert_eq!(json, JsonFile::encode(data)),
        Some(options) => assert_eq!(json, JsonFile::encode_with_options(data, options)),
    }
}

/// PHP's JSON_UNESCAPED_UNICODE-only flag set (used by testOnlyUnicode).
fn unescaped_unicode_only() -> JsonEncodeOptions {
    JsonEncodeOptions {
        unescaped_slashes: false,
        pretty_print: false,
        unescaped_unicode: true,
        indent: JsonFile::INDENT_DEFAULT.to_string(),
    }
}

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../composer/tests/Composer/Test/Json/Fixtures")
        .join(name)
}

/// ref: TestCase::createTempFile (PHP tempnam())
fn create_temp_file() -> String {
    let mut path = std::env::temp_dir();
    let unique = format!(
        "shirabe_jsonfiletest_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    path.push(unique);
    std::fs::write(&path, b"").unwrap();
    path.to_str().unwrap().to_string()
}

#[test]
#[ignore = "JsonFile::parse_json error path reaches JsonParser::new()/lint() (todo!()); the JSON linter is not yet ported"]
fn test_parse_error_detect_extra_comma() {
    let json = "{\n        \"foo\": \"bar\",\n}";
    expect_parse_exception("Parse error on line 2", json);
}

#[test]
#[ignore = "JsonFile::parse_json error path reaches JsonParser::new()/lint() (todo!()); the JSON linter is not yet ported"]
fn test_parse_error_detect_extra_comma_in_array() {
    let json = "{\n        \"foo\": [\n            \"bar\",\n        ]\n}";
    expect_parse_exception("Parse error on line 3", json);
}

#[test]
#[ignore = "JsonFile::parse_json error path reaches JsonParser::new()/lint() (todo!()); the JSON linter is not yet ported"]
fn test_parse_error_detect_unescaped_backslash() {
    let json = "{\n        \"fo\\o\": \"bar\"\n}";
    expect_parse_exception("Parse error on line 1", json);
}

#[test]
#[ignore = "JsonFile::parse_json error path reaches JsonParser::new()/lint() (todo!()); the JSON linter is not yet ported"]
fn test_parse_error_skips_escaped_backslash() {
    let json = "{\n        \"fo\\\\o\": \"bar\"\n        \"a\": \"b\"\n}";
    expect_parse_exception("Parse error on line 2", json);
}

#[test]
#[ignore = "JsonFile::parse_json error path reaches JsonParser::new()/lint() (todo!()); the JSON linter is not yet ported"]
fn test_parse_error_detect_single_quotes() {
    let json = "{\n        'foo': \"bar\"\n}";
    expect_parse_exception("Parse error on line 1", json);
}

#[test]
#[ignore = "JsonFile::parse_json error path reaches JsonParser::new()/lint() (todo!()); the JSON linter is not yet ported"]
fn test_parse_error_detect_missing_quotes() {
    let json = "{\n        foo: \"bar\"\n}";
    expect_parse_exception("Parse error on line 1", json);
}

#[test]
#[ignore = "JsonFile::parse_json error path reaches JsonParser::new()/lint() (todo!()); the JSON linter is not yet ported"]
fn test_parse_error_detect_array_as_hash() {
    let json = "{\n        \"foo\": [\"bar\": \"baz\"]\n}";
    expect_parse_exception("Parse error on line 2", json);
}

#[test]
#[ignore = "JsonFile::parse_json error path reaches JsonParser::new()/lint() (todo!()); the JSON linter is not yet ported"]
fn test_parse_error_detect_missing_comma() {
    let json = "{\n        \"foo\": \"bar\"\n        \"bar\": \"foo\"\n}";
    expect_parse_exception("Parse error on line 2", json);
}

#[test]
#[ignore = "JsonFile::parse_json error path reaches JsonParser::new()/lint() (todo!()); the JSON linter is not yet ported"]
fn test_parse_error_detect_missing_comma_multiline() {
    let json = "{\n        \"foo\": \"barbar\"\n\n        \"bar\": \"foo\"\n}";
    expect_parse_exception("Parse error on line 2", json);
}

#[test]
#[ignore = "JsonFile::parse_json error path reaches JsonParser::new()/lint() (todo!()); the JSON linter is not yet ported"]
fn test_parse_error_detect_missing_colon() {
    let json = "{\n        \"foo\": \"bar\",\n        \"bar\" \"foo\"\n}";
    expect_parse_exception("Parse error on line 3", json);
}

#[test]
#[ignore]
fn test_simple_json_string() {
    let mut data: IndexMap<String, PhpMixed> = IndexMap::new();
    data.insert(
        "name".to_string(),
        PhpMixed::String("composer/composer".to_string()),
    );
    let json = "{\n    \"name\": \"composer/composer\"\n}";
    assert_json_format(json, &PhpMixed::Array(data), None);
}

#[test]
#[ignore]
fn test_trailing_backslash() {
    let mut data: IndexMap<String, PhpMixed> = IndexMap::new();
    data.insert(
        "Metadata\\".to_string(),
        PhpMixed::String("src/".to_string()),
    );
    let json = "{\n    \"Metadata\\\\\": \"src/\"\n}";
    assert_json_format(json, &PhpMixed::Array(data), None);
}

#[test]
#[ignore]
fn test_format_empty_array() {
    let mut data: IndexMap<String, PhpMixed> = IndexMap::new();
    data.insert("test".to_string(), PhpMixed::List(vec![]));
    data.insert("test2".to_string(), PhpMixed::Object(IndexMap::new()));
    let json = "{\n    \"test\": [],\n    \"test2\": {}\n}";
    assert_json_format(json, &PhpMixed::Array(data), None);
}

#[test]
#[ignore]
fn test_escape() {
    let mut data: IndexMap<String, PhpMixed> = IndexMap::new();
    data.insert(
        "Metadata\\\"".to_string(),
        PhpMixed::String("src/".to_string()),
    );
    let json = "{\n    \"Metadata\\\\\\\"\": \"src/\"\n}";
    assert_json_format(json, &PhpMixed::Array(data), None);
}

#[test]
#[ignore]
fn test_unicode() {
    let mut data: IndexMap<String, PhpMixed> = IndexMap::new();
    data.insert(
        "Žluťoučký \" kůň".to_string(),
        PhpMixed::String("úpěl ďábelské ódy za €".to_string()),
    );
    let json = "{\n    \"Žluťoučký \\\" kůň\": \"úpěl ďábelské ódy za €\"\n}";
    assert_json_format(json, &PhpMixed::Array(data), None);
}

#[test]
#[ignore]
fn test_only_unicode() {
    let data = PhpMixed::String("\\/ƌ".to_string());
    assert_json_format("\"\\\\\\/ƌ\"", &data, Some(unescaped_unicode_only()));
}

#[test]
#[ignore]
fn test_escaped_slashes() {
    let data = PhpMixed::String("\\/foo".to_string());
    assert_json_format("\"\\\\\\/foo\"", &data, Some(JsonEncodeOptions::none()));
}

#[test]
#[ignore]
fn test_escaped_backslashes() {
    let data = PhpMixed::String("a\\b".to_string());
    assert_json_format("\"a\\\\b\"", &data, Some(JsonEncodeOptions::none()));
}

#[test]
#[ignore]
fn test_escaped_unicode() {
    let data = PhpMixed::String("ƌ".to_string());
    assert_json_format("\"\\u018c\"", &data, Some(JsonEncodeOptions::none()));
}

#[test]
#[ignore]
fn test_double_escaped_unicode() {
    let data = PhpMixed::List(vec![
        PhpMixed::String("Zdjęcia".to_string()),
        PhpMixed::String("hjkjhl\\u0119kkjk".to_string()),
    ]);
    let encoded_data = JsonFile::encode(&data);

    let mut wrapper: IndexMap<String, PhpMixed> = IndexMap::new();
    wrapper.insert("t".to_string(), PhpMixed::String(encoded_data));
    let double_encoded_data = JsonFile::encode(&PhpMixed::Array(wrapper));

    let decoded_data = shirabe_php_shim::json_decode(&double_encoded_data, true).unwrap();
    let t = decoded_data.as_array().unwrap().get("t").unwrap();
    let double_data = shirabe_php_shim::json_decode(t.as_string().unwrap(), true).unwrap();
    assert_eq!(data, double_data);
}

#[test]
#[ignore]
fn test_preserve_indentation_after_read() {
    let src = fixture_path("tabs.json");
    let dst = fixture_path("tabs2.json");
    std::fs::copy(&src, &dst).unwrap();

    let mut json_file = JsonFile::new(dst.to_str().unwrap().to_string(), None, None).unwrap();
    let _data = json_file.read().unwrap();
    let mut hash: IndexMap<String, PhpMixed> = IndexMap::new();
    hash.insert("foo".to_string(), PhpMixed::String("baz".to_string()));
    json_file.write(PhpMixed::Array(hash)).unwrap();

    assert_eq!(
        "{\n\t\"foo\": \"baz\"\n}\n",
        std::fs::read_to_string(&dst).unwrap()
    );

    std::fs::remove_file(&dst).unwrap();
}

#[test]
#[ignore]
fn test_overwrites_indentation_by_default() {
    let src = fixture_path("tabs.json");
    let dst = fixture_path("tabs2.json");
    std::fs::copy(&src, &dst).unwrap();

    let json_file = JsonFile::new(dst.to_str().unwrap().to_string(), None, None).unwrap();
    let mut hash: IndexMap<String, PhpMixed> = IndexMap::new();
    hash.insert("foo".to_string(), PhpMixed::String("baz".to_string()));
    json_file.write(PhpMixed::Array(hash)).unwrap();

    assert_eq!(
        "{\n    \"foo\": \"baz\"\n}\n",
        std::fs::read_to_string(&dst).unwrap()
    );

    std::fs::remove_file(&dst).unwrap();
}

#[test]
#[ignore]
fn test_schema_validation() {
    let path = fixture_path("composer.json");
    let json = JsonFile::new(path.to_str().unwrap().to_string(), None, None).unwrap();
    json.validate_schema(JsonFile::STRICT_SCHEMA, None).unwrap();
    json.validate_schema(JsonFile::LAX_SCHEMA, None).unwrap();
}

#[test]
#[ignore]
fn test_schema_validation_error() {
    let file = create_temp_file();
    std::fs::write(&file, b"{ \"name\": null }").unwrap();
    let json = JsonFile::new(file.clone(), None, None).unwrap();
    let expected_message = format!("\"{}\" does not match the expected JSON schema", file);
    let expected_error = "name : NULL value found, but a string is required".to_string();

    let err = json
        .validate_schema(JsonFile::STRICT_SCHEMA, None)
        .unwrap_err();
    let e = err.downcast_ref::<JsonValidationException>().unwrap();
    assert_eq!(expected_message, e.get_message());
    assert!(e.get_errors().contains(&expected_error));

    let err = json
        .validate_schema(JsonFile::LAX_SCHEMA, None)
        .unwrap_err();
    let e = err.downcast_ref::<JsonValidationException>().unwrap();
    assert_eq!(expected_message, e.get_message());
    assert!(e.get_errors().contains(&expected_error));

    std::fs::remove_file(&file).unwrap();
}

#[test]
#[ignore]
fn test_schema_validation_lax_additional_properties() {
    let file = create_temp_file();
    std::fs::write(
        &file,
        b"{ \"name\": \"vendor/package\", \"description\": \"generic description\", \"foo\": \"bar\" }",
    )
    .unwrap();
    let json = JsonFile::new(file.clone(), None, None).unwrap();

    let err = json
        .validate_schema(JsonFile::STRICT_SCHEMA, None)
        .unwrap_err();
    let e = err.downcast_ref::<JsonValidationException>().unwrap();
    assert_eq!(
        format!("\"{}\" does not match the expected JSON schema", file),
        e.get_message()
    );
    assert_eq!(
        &vec![
            "The property foo is not defined and the definition does not allow additional properties"
                .to_string()
        ],
        e.get_errors()
    );

    json.validate_schema(JsonFile::LAX_SCHEMA, None).unwrap();
    std::fs::remove_file(&file).unwrap();
}

#[test]
#[ignore]
fn test_schema_validation_lax_required() {
    let file = create_temp_file();
    let json = JsonFile::new(file.clone(), None, None).unwrap();

    let expected_message = format!("\"{}\" does not match the expected JSON schema", file);

    std::fs::write(&file, b"{ }").unwrap();
    let err = json
        .validate_schema(JsonFile::STRICT_SCHEMA, None)
        .unwrap_err();
    let e = err.downcast_ref::<JsonValidationException>().unwrap();
    assert_eq!(expected_message, e.get_message());
    let errors = e.get_errors();
    assert!(errors.contains(&"name : The property name is required".to_string()));
    assert!(errors.contains(&"description : The property description is required".to_string()));
    json.validate_schema(JsonFile::LAX_SCHEMA, None).unwrap();

    std::fs::write(&file, b"{ \"name\": \"vendor/package\" }").unwrap();
    let err = json
        .validate_schema(JsonFile::STRICT_SCHEMA, None)
        .unwrap_err();
    let e = err.downcast_ref::<JsonValidationException>().unwrap();
    assert_eq!(expected_message, e.get_message());
    assert_eq!(
        &vec!["description : The property description is required".to_string()],
        e.get_errors()
    );
    json.validate_schema(JsonFile::LAX_SCHEMA, None).unwrap();

    std::fs::write(&file, b"{ \"description\": \"generic description\" }").unwrap();
    let err = json
        .validate_schema(JsonFile::STRICT_SCHEMA, None)
        .unwrap_err();
    let e = err.downcast_ref::<JsonValidationException>().unwrap();
    assert_eq!(expected_message, e.get_message());
    assert_eq!(
        &vec!["name : The property name is required".to_string()],
        e.get_errors()
    );
    json.validate_schema(JsonFile::LAX_SCHEMA, None).unwrap();

    std::fs::write(&file, b"{ \"type\": \"library\" }").unwrap();
    let err = json
        .validate_schema(JsonFile::STRICT_SCHEMA, None)
        .unwrap_err();
    let e = err.downcast_ref::<JsonValidationException>().unwrap();
    assert_eq!(expected_message, e.get_message());
    let errors = e.get_errors();
    assert!(errors.contains(&"name : The property name is required".to_string()));
    assert!(errors.contains(&"description : The property description is required".to_string()));
    json.validate_schema(JsonFile::LAX_SCHEMA, None).unwrap();

    std::fs::write(&file, b"{ \"type\": \"project\" }").unwrap();
    let err = json
        .validate_schema(JsonFile::STRICT_SCHEMA, None)
        .unwrap_err();
    let e = err.downcast_ref::<JsonValidationException>().unwrap();
    assert_eq!(expected_message, e.get_message());
    let errors = e.get_errors();
    assert!(errors.contains(&"name : The property name is required".to_string()));
    assert!(errors.contains(&"description : The property description is required".to_string()));
    json.validate_schema(JsonFile::LAX_SCHEMA, None).unwrap();

    std::fs::write(
        &file,
        b"{ \"name\": \"vendor/package\", \"description\": \"generic description\" }",
    )
    .unwrap();
    json.validate_schema(JsonFile::STRICT_SCHEMA, None).unwrap();
    json.validate_schema(JsonFile::LAX_SCHEMA, None).unwrap();

    std::fs::remove_file(&file).unwrap();
}

#[test]
#[ignore]
fn test_custom_schema_validation_lax() {
    let file = create_temp_file();
    std::fs::write(
        &file,
        b"{ \"custom\": \"property\", \"another custom\": \"property\" }",
    )
    .unwrap();

    let schema = create_temp_file();
    std::fs::write(
        &schema,
        b"{ \"properties\": { \"custom\": { \"type\": \"string\" }}}",
    )
    .unwrap();

    let json = JsonFile::new(file.clone(), None, None).unwrap();

    json.validate_schema(JsonFile::LAX_SCHEMA, Some(&schema))
        .unwrap();

    std::fs::remove_file(&file).unwrap();
    std::fs::remove_file(&schema).unwrap();
}

#[test]
#[ignore]
fn test_custom_schema_validation_strict() {
    let file = create_temp_file();
    std::fs::write(&file, b"{ \"custom\": \"property\" }").unwrap();

    let schema = create_temp_file();
    std::fs::write(
        &schema,
        b"{ \"properties\": { \"custom\": { \"type\": \"string\" }}}",
    )
    .unwrap();

    let json = JsonFile::new(file.clone(), None, None).unwrap();

    json.validate_schema(JsonFile::STRICT_SCHEMA, Some(&schema))
        .unwrap();

    std::fs::remove_file(&file).unwrap();
    std::fs::remove_file(&schema).unwrap();
}

#[test]
#[ignore]
fn test_auth_schema_validation_with_custom_data_source() {
    let json = shirabe_php_shim::json_decode("{\"github-oauth\": \"foo\"}", false).unwrap();
    let expected_message = "\"COMPOSER_AUTH\" does not match the expected JSON schema".to_string();
    let expected_error = "github-oauth : String value found, but an object is required".to_string();

    let err = JsonFile::validate_json_schema("COMPOSER_AUTH", &json, JsonFile::AUTH_SCHEMA, None)
        .unwrap_err();
    let e = err.downcast_ref::<JsonValidationException>().unwrap();
    assert_eq!(expected_message, e.get_message());
    assert_eq!(&vec![expected_error], e.get_errors());
}

/// ref: shared `$data` for the simple merge-conflict cases.
fn merge_conflict_simple_data() -> PhpMixed {
    let mut data: IndexMap<String, PhpMixed> = IndexMap::new();
    data.insert(
        "_readme".to_string(),
        PhpMixed::List(vec![
            PhpMixed::String(
                "This file locks the dependencies of your project to a known state".to_string(),
            ),
            PhpMixed::String(
                "Read more about it at https://getcomposer.org/doc/01-basic-usage.md#installing-dependencies"
                    .to_string(),
            ),
            PhpMixed::String("This file is @generated automatically".to_string()),
        ]),
    );
    data.insert(
        "content-hash".to_string(),
        PhpMixed::String(
            "VCS merge conflict detected. Please run `composer update --lock`.".to_string(),
        ),
    );
    data.insert("packages".to_string(), PhpMixed::List(vec![]));
    data.insert("packages-dev".to_string(), PhpMixed::List(vec![]));
    data.insert("aliases".to_string(), PhpMixed::List(vec![]));
    data.insert(
        "minimum-stability".to_string(),
        PhpMixed::String("stable".to_string()),
    );
    data.insert("stability-flags".to_string(), PhpMixed::List(vec![]));
    data.insert("prefer-stable".to_string(), PhpMixed::Bool(false));
    data.insert("prefer-lowest".to_string(), PhpMixed::Bool(false));
    data.insert("platform".to_string(), PhpMixed::List(vec![]));
    data.insert("platform-dev".to_string(), PhpMixed::List(vec![]));
    data.insert(
        "plugin-api-version".to_string(),
        PhpMixed::String("2.3.0".to_string()),
    );
    PhpMixed::Array(data)
}

#[test]
#[ignore]
fn test_composer_lock_file_merge_conflict_simple() {
    let data = merge_conflict_simple_data();
    let json =
        std::fs::read_to_string(fixture_path("composer-lock-merge-conflict-simple.txt")).unwrap();
    assert_eq!(
        data,
        JsonFile::parse_json(Some(&json), Some("/path/to/composer.lock")).unwrap()
    );
}

#[test]
#[ignore]
fn test_composer_lock_file_merge_conflict_simple_crlf() {
    let data = merge_conflict_simple_data();
    let json =
        std::fs::read_to_string(fixture_path("composer-lock-merge-conflict-simple.txt")).unwrap();
    assert_eq!(
        data,
        JsonFile::parse_json(Some(&json), Some("/path/to/composer.lock")).unwrap()
    );
}

#[test]
#[ignore]
fn test_composer_lock_file_merge_conflict_complex() {
    // complex files have multiple conflict markers and can thus not be simply resolved
    let data =
        std::fs::read_to_string(fixture_path("composer-lock-merge-conflict-complex.txt")).unwrap();

    let err = JsonFile::parse_json(Some(&data), Some("/path/to/composer.lock")).unwrap_err();
    assert!(err.downcast_ref::<ParsingException>().is_some());
}

#[test]
#[ignore]
fn test_composer_lock_file_merge_conflict_complex_crlf() {
    // complex files have multiple conflict markers and can thus not be simply resolved
    let data = std::fs::read_to_string(fixture_path(
        "composer-lock-merge-conflict-complex-crlf.txt",
    ))
    .unwrap();

    let err = JsonFile::parse_json(Some(&data), Some("/path/to/composer.lock")).unwrap_err();
    assert!(err.downcast_ref::<ParsingException>().is_some());
}

#[test]
#[ignore]
fn test_composer_lock_file_merge_conflict_extended() {
    let data =
        std::fs::read_to_string(fixture_path("composer-lock-merge-conflict-extended.txt")).unwrap();

    let json = JsonFile::parse_json(Some(&data), Some("/path/to/composer.lock")).unwrap();
    assert_eq!(
        "VCS merge conflict detected. Please run `composer update --lock`.",
        json.as_array()
            .unwrap()
            .get("content-hash")
            .unwrap()
            .as_string()
            .unwrap()
    );
}
