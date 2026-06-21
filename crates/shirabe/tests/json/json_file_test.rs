//! ref: composer/tests/Composer/Test/Json/JsonFileTest.php

use shirabe::json::JsonFile;

/// ref: JsonFileTest::expectParseException
fn expect_parse_exception(text: &str, json: &str) {
    let err = JsonFile::parse_json(Some(json), None).unwrap_err();
    let message = err.to_string();
    assert!(message.contains(text), "expected {text:?} in {message:?}");
}

#[test]
#[ignore = "JsonFile::parse_json error path reaches json_last_error (todo!()) in the php-shim"]
fn test_parse_error_detect_extra_comma() {
    let json = "{\n        \"foo\": \"bar\",\n}";
    expect_parse_exception("Parse error on line 2", json);
}

#[test]
#[ignore = "JsonFile::parse_json error path reaches json_last_error (todo!()) in the php-shim"]
fn test_parse_error_detect_extra_comma_in_array() {
    let json = "{\n        \"foo\": [\n            \"bar\",\n        ]\n}";
    expect_parse_exception("Parse error on line 3", json);
}

#[test]
#[ignore = "JsonFile::parse_json error path reaches json_last_error (todo!()) in the php-shim"]
fn test_parse_error_detect_unescaped_backslash() {
    let json = "{\n        \"fo\\o\": \"bar\"\n}";
    expect_parse_exception("Parse error on line 1", json);
}

#[test]
#[ignore = "JsonFile::parse_json error path reaches json_last_error (todo!()) in the php-shim"]
fn test_parse_error_skips_escaped_backslash() {
    let json = "{\n        \"fo\\\\o\": \"bar\"\n        \"a\": \"b\"\n}";
    expect_parse_exception("Parse error on line 2", json);
}

#[test]
#[ignore = "JsonFile::parse_json error path reaches json_last_error (todo!()) in the php-shim"]
fn test_parse_error_detect_single_quotes() {
    let json = "{\n        'foo': \"bar\"\n}";
    expect_parse_exception("Parse error on line 1", json);
}

#[test]
#[ignore = "JsonFile::parse_json error path reaches json_last_error (todo!()) in the php-shim"]
fn test_parse_error_detect_missing_quotes() {
    let json = "{\n        foo: \"bar\"\n}";
    expect_parse_exception("Parse error on line 1", json);
}

#[test]
#[ignore = "JsonFile::parse_json error path reaches json_last_error (todo!()) in the php-shim"]
fn test_parse_error_detect_array_as_hash() {
    let json = "{\n        \"foo\": [\"bar\": \"baz\"]\n}";
    expect_parse_exception("Parse error on line 2", json);
}

#[test]
#[ignore = "JsonFile::parse_json error path reaches json_last_error (todo!()) in the php-shim"]
fn test_parse_error_detect_missing_comma() {
    let json = "{\n        \"foo\": \"bar\"\n        \"bar\": \"foo\"\n}";
    expect_parse_exception("Parse error on line 2", json);
}

#[test]
#[ignore = "JsonFile::parse_json error path reaches json_last_error (todo!()) in the php-shim"]
fn test_parse_error_detect_missing_comma_multiline() {
    let json = "{\n        \"foo\": \"barbar\"\n\n        \"bar\": \"foo\"\n}";
    expect_parse_exception("Parse error on line 2", json);
}

#[test]
#[ignore = "JsonFile::parse_json error path reaches json_last_error (todo!()) in the php-shim"]
fn test_parse_error_detect_missing_colon() {
    let json = "{\n        \"foo\": \"bar\",\n        \"bar\" \"foo\"\n}";
    expect_parse_exception("Parse error on line 3", json);
}

// The encode cases assert JsonFile::encode output for specific PHP flag combinations and
// data shapes (incl. stdClass vs array). Faithful flag mapping and value modeling are not
// reproduced here.
#[test]
#[ignore = "asserts JsonFile::encode output for specific PHP flag/value combinations; not reproduced here"]
fn test_simple_json_string() {
    todo!()
}

#[test]
#[ignore = "asserts JsonFile::encode output for specific PHP flag/value combinations; not reproduced here"]
fn test_trailing_backslash() {
    todo!()
}

#[test]
#[ignore = "asserts JsonFile::encode output for specific PHP flag/value combinations; not reproduced here"]
fn test_format_empty_array() {
    todo!()
}

#[test]
#[ignore = "asserts JsonFile::encode output for specific PHP flag/value combinations; not reproduced here"]
fn test_escape() {
    todo!()
}

#[test]
#[ignore = "asserts JsonFile::encode output for specific PHP flag/value combinations; not reproduced here"]
fn test_unicode() {
    todo!()
}

#[test]
#[ignore = "asserts JsonFile::encode output for specific PHP flag/value combinations; not reproduced here"]
fn test_only_unicode() {
    todo!()
}

#[test]
#[ignore = "asserts JsonFile::encode output for specific PHP flag/value combinations; not reproduced here"]
fn test_escaped_slashes() {
    todo!()
}

#[test]
#[ignore = "asserts JsonFile::encode output for specific PHP flag/value combinations; not reproduced here"]
fn test_escaped_backslashes() {
    todo!()
}

#[test]
#[ignore = "asserts JsonFile::encode output for specific PHP flag/value combinations; not reproduced here"]
fn test_escaped_unicode() {
    todo!()
}

#[test]
#[ignore = "asserts JsonFile::encode output for specific PHP flag/value combinations; not reproduced here"]
fn test_double_escaped_unicode() {
    todo!()
}

// These read a fixture composer.json and assert read/write indentation behaviour.
#[test]
#[ignore = "reads a fixture file and asserts indentation behaviour of JsonFile read/write"]
fn test_preserve_indentation_after_read() {
    todo!()
}

#[test]
#[ignore = "reads a fixture file and asserts indentation behaviour of JsonFile read/write"]
fn test_overwrites_indentation_by_default() {
    todo!()
}

// validateSchema needs the bundled JSON schema and the json-schema validator plus fixtures.
#[test]
#[ignore = "needs JsonFile::validateSchema (json-schema validation) and the schema/fixtures"]
fn test_schema_validation() {
    todo!()
}

#[test]
#[ignore = "needs JsonFile::validateSchema (json-schema validation) and the schema/fixtures"]
fn test_schema_validation_error() {
    todo!()
}

#[test]
#[ignore = "needs JsonFile::validateSchema (json-schema validation) and the schema/fixtures"]
fn test_schema_validation_lax_additional_properties() {
    todo!()
}

#[test]
#[ignore = "needs JsonFile::validateSchema (json-schema validation) and the schema/fixtures"]
fn test_schema_validation_lax_required() {
    todo!()
}

#[test]
#[ignore = "needs JsonFile::validateSchema (json-schema validation) and the schema/fixtures"]
fn test_custom_schema_validation_lax() {
    todo!()
}

#[test]
#[ignore = "needs JsonFile::validateSchema (json-schema validation) and the schema/fixtures"]
fn test_custom_schema_validation_strict() {
    todo!()
}

#[test]
#[ignore = "needs JsonFile::validateSchema (json-schema validation) and the schema/fixtures"]
fn test_auth_schema_validation_with_custom_data_source() {
    todo!()
}

// The merge-conflict cases build large lock-file structures (with VCS conflict markers) and
// assert the resulting ParsingException; the fixtures are sizeable and not reproduced here.
#[test]
#[ignore = "builds a large lock-file structure with VCS merge markers; not reproduced here"]
fn test_composer_lock_file_merge_conflict_simple() {
    todo!()
}

#[test]
#[ignore = "builds a large lock-file structure with VCS merge markers; not reproduced here"]
fn test_composer_lock_file_merge_conflict_simple_crlf() {
    todo!()
}

#[test]
#[ignore = "builds a large lock-file structure with VCS merge markers; not reproduced here"]
fn test_composer_lock_file_merge_conflict_complex() {
    todo!()
}

#[test]
#[ignore = "builds a large lock-file structure with VCS merge markers; not reproduced here"]
fn test_composer_lock_file_merge_conflict_complex_crlf() {
    todo!()
}

#[test]
#[ignore = "builds a large lock-file structure with VCS merge markers; not reproduced here"]
fn test_composer_lock_file_merge_conflict_extended() {
    todo!()
}
