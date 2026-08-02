//! Oracle tests: the `PluginValue` codec against the real PHP `serialize()`/`unserialize()`.
//!
//! Encode direction: bytes produced by the Rust encoder are unserialized and re-serialized by
//! the PHP core implementation; the result must be byte-identical. Decode direction: bytes
//! produced by PHP `serialize()` must decode into a `PluginValue` whose re-encoding is
//! byte-identical. Floats (serialize_precision), non-UTF-8 byte strings and deep nesting are the
//! focus areas.

use indexmap::IndexMap;
use shirabe_external_packages::symfony::process::PhpExecutableFinder;
use shirabe_php_rpc::value::{serialize, unserialize};
use shirabe_php_rpc::{PluginValue, call_function};

fn php_available() -> bool {
    PhpExecutableFinder::new().find(false).is_some()
}

/// Feeds raw serialize() bytes through the PHP core codec and returns what PHP re-serializes.
fn php_reserialize(bytes: &[u8]) -> Vec<u8> {
    let outcome = call_function(
        "__shirabe_oracle_roundtrip",
        vec![PluginValue::String(bytes.to_vec())],
    )
    .expect("oracle roundtrip request failed");
    match outcome.expect("oracle roundtrip threw") {
        PluginValue::String(bytes) => bytes,
        other => panic!("oracle roundtrip returned a non-string: {other:?}"),
    }
}

/// Runs a PHP snippet and returns its `return` value.
fn php_eval(code: &str) -> PluginValue {
    call_function("__shirabe_eval", vec![PluginValue::string(code)])
        .expect("eval request failed")
        .expect("eval threw")
}

fn assert_php_agrees(value: &PluginValue) {
    let encoded = serialize(value);
    let reserialized = php_reserialize(&encoded);
    assert_eq!(
        String::from_utf8_lossy(&reserialized),
        String::from_utf8_lossy(&encoded),
        "PHP re-serialized {value:?} differently"
    );
}

#[test]
fn encode_direction_matches_php_for_scalars_and_floats() {
    if !php_available() {
        return;
    }

    let floats = [
        0.0,
        -0.0,
        1.5,
        0.1,
        2.0,
        -2.0,
        100.0,
        1e15,
        1e16,
        1e17,
        1e18,
        1e20,
        1.5e20,
        1e-4,
        1e-5,
        12345.6789e-9,
        1e-300,
        f64::MAX,
        5e-324,
        1.0 / 3.0,
        0.30000000000000004,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    for f in floats {
        assert_php_agrees(&PluginValue::Float(f));
    }

    for value in [
        PluginValue::Null,
        PluginValue::Bool(true),
        PluginValue::Bool(false),
        PluginValue::Int(0),
        PluginValue::Int(i64::MAX),
        PluginValue::Int(i64::MIN),
        PluginValue::string(""),
        PluginValue::string("héllo wörld"),
    ] {
        assert_php_agrees(&value);
    }
}

#[test]
fn encode_direction_matches_php_for_non_utf8_bytes() {
    if !php_available() {
        return;
    }

    assert_php_agrees(&PluginValue::String(vec![0xff, 0x00, 0xfe, 0x80, b'"']));
    assert_php_agrees(&PluginValue::String((0u8..=255).collect()));

    let mut map: IndexMap<Vec<u8>, PluginValue> = IndexMap::new();
    map.insert(vec![0xff, 0x00], PluginValue::String(vec![0x80]));
    map.insert(b"05".to_vec(), PluginValue::Bool(true));
    map.insert(b"5".to_vec(), PluginValue::Null);
    assert_php_agrees(&PluginValue::Array(map));
}

#[test]
fn encode_direction_matches_php_for_nested_arrays() {
    if !php_available() {
        return;
    }

    let mut inner: IndexMap<Vec<u8>, PluginValue> = IndexMap::new();
    inner.insert(b"a".to_vec(), PluginValue::Int(1));
    inner.insert(
        b"b".to_vec(),
        PluginValue::List(vec![
            PluginValue::Float(0.1),
            PluginValue::string("x"),
            PluginValue::List(vec![]),
        ]),
    );
    assert_php_agrees(&PluginValue::Array(inner));

    let mut deep = PluginValue::Null;
    for _ in 0..64 {
        deep = PluginValue::List(vec![deep]);
    }
    assert_php_agrees(&deep);
}

#[test]
fn decode_direction_matches_php_serialize_output() {
    if !php_available() {
        return;
    }

    let snippets = [
        // Mixed key types: PHP canonicalizes "5" to an int key, keeps "05" as a string.
        r#"return serialize(["a" => 1, 5 => true, "05" => [1, 2, [0.5]], "z" => null]);"#,
        // Non-UTF-8 byte strings, both as values and as keys.
        "return serialize([\"\\xff\\x00key\" => \"\\x80\\x81\", 0 => \"plain\"]);",
        // Floats straight from the PHP formatter.
        r#"return serialize([0.1, 2.0, 1e17, 1e-5, -0.0, NAN, INF, -INF, 1/3]);"#,
        // A sparse int-keyed array (not a list).
        r#"return serialize([3 => "c", 1 => "a"]);"#,
        // Deep nesting built in a loop.
        r#"$v = "leaf"; for ($i = 0; $i < 256; $i++) { $v = [$v]; } return serialize($v);"#,
    ];

    for snippet in snippets {
        let PluginValue::String(php_bytes) = php_eval(snippet) else {
            panic!("snippet did not return a string: {snippet}");
        };
        let decoded = unserialize(&php_bytes)
            .unwrap_or_else(|e| panic!("failed to decode PHP output for `{snippet}`: {e:#}"));
        let reencoded = serialize(&decoded);
        assert_eq!(
            String::from_utf8_lossy(&reencoded),
            String::from_utf8_lossy(&php_bytes),
            "re-encoding diverged for `{snippet}`"
        );
    }
}

#[test]
fn decode_direction_distinguishes_lists_from_maps() {
    if !php_available() {
        return;
    }

    let PluginValue::String(bytes) = php_eval(r#"return serialize([10, 20, 30]);"#) else {
        panic!("expected serialized bytes");
    };
    assert_eq!(
        unserialize(&bytes).unwrap(),
        PluginValue::List(vec![
            PluginValue::Int(10),
            PluginValue::Int(20),
            PluginValue::Int(30),
        ]),
    );

    let PluginValue::String(bytes) = php_eval(r#"return serialize([1 => 10, 0 => 20]);"#) else {
        panic!("expected serialized bytes");
    };
    let decoded = unserialize(&bytes).unwrap();
    assert!(
        matches!(decoded, PluginValue::Array(_)),
        "out-of-order int keys must not decode as a list: {decoded:?}"
    );
}
