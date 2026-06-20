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
macro_rules! encode_stub {
    ($name:ident) => {
        #[test]
        #[ignore = "asserts JsonFile::encode output for specific PHP flag/value combinations; not reproduced here"]
        fn $name() {
            todo!()
        }
    };
}

encode_stub!(test_simple_json_string);
encode_stub!(test_trailing_backslash);
encode_stub!(test_format_empty_array);
encode_stub!(test_escape);
encode_stub!(test_unicode);
encode_stub!(test_only_unicode);
encode_stub!(test_escaped_slashes);
encode_stub!(test_escaped_backslashes);
encode_stub!(test_escaped_unicode);
encode_stub!(test_double_escaped_unicode);

// These read a fixture composer.json and assert read/write indentation behaviour.
macro_rules! indentation_stub {
    ($name:ident) => {
        #[test]
        #[ignore = "reads a fixture file and asserts indentation behaviour of JsonFile read/write"]
        fn $name() {
            todo!()
        }
    };
}

indentation_stub!(test_preserve_indentation_after_read);
indentation_stub!(test_overwrites_indentation_by_default);

// validateSchema needs the bundled JSON schema and the json-schema validator plus fixtures.
macro_rules! schema_stub {
    ($name:ident) => {
        #[test]
        #[ignore = "needs JsonFile::validateSchema (json-schema validation) and the schema/fixtures"]
        fn $name() {
            todo!()
        }
    };
}

schema_stub!(test_schema_validation);
schema_stub!(test_schema_validation_error);
schema_stub!(test_schema_validation_lax_additional_properties);
schema_stub!(test_schema_validation_lax_required);
schema_stub!(test_custom_schema_validation_lax);
schema_stub!(test_custom_schema_validation_strict);
schema_stub!(test_auth_schema_validation_with_custom_data_source);

// The merge-conflict cases build large lock-file structures (with VCS conflict markers) and
// assert the resulting ParsingException; the fixtures are sizeable and not reproduced here.
macro_rules! merge_stub {
    ($name:ident) => {
        #[test]
        #[ignore = "builds a large lock-file structure with VCS merge markers; not reproduced here"]
        fn $name() {
            todo!()
        }
    };
}

merge_stub!(test_composer_lock_file_merge_conflict_simple);
merge_stub!(test_composer_lock_file_merge_conflict_simple_crlf);
merge_stub!(test_composer_lock_file_merge_conflict_complex);
merge_stub!(test_composer_lock_file_merge_conflict_complex_crlf);
merge_stub!(test_composer_lock_file_merge_conflict_extended);
