//! ref: composer/tests/Composer/Test/Package/Version/VersionParserTest.php

use indexmap::IndexMap;
use shirabe::package::version::version_parser::VersionParser;

fn pairs(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn entry(fields: &[(&str, &str)]) -> IndexMap<String, String> {
    fields
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
#[ignore]
fn test_parse_name_version_pairs() {
    for (input, result) in provide_parse_name_version_pairs_data() {
        let version_parser = VersionParser::new();

        assert_eq!(
            result,
            version_parser.parse_name_version_pairs(input).unwrap()
        );
    }
}

fn provide_parse_name_version_pairs_data() -> Vec<(Vec<String>, Vec<IndexMap<String, String>>)> {
    vec![
        (
            pairs(&["php:^7.0"]),
            vec![entry(&[("name", "php"), ("version", "^7.0")])],
        ),
        (
            pairs(&["php", "^7.0"]),
            vec![entry(&[("name", "php"), ("version", "^7.0")])],
        ),
        (
            pairs(&["php", "ext-apcu"]),
            vec![entry(&[("name", "php")]), entry(&[("name", "ext-apcu")])],
        ),
        (
            pairs(&["foo/*", "bar*", "acme/baz", "*@dev"]),
            vec![
                entry(&[("name", "foo/*")]),
                entry(&[("name", "bar*")]),
                entry(&[("name", "acme/baz"), ("version", "*@dev")]),
            ],
        ),
        (
            pairs(&["php", "*"]),
            vec![entry(&[("name", "php"), ("version", "*")])],
        ),
    ]
}

#[test]
fn test_is_upgrade() {
    for (from, to, expected) in provide_is_upgrade_tests() {
        assert_eq!(expected, VersionParser::is_upgrade(&from, &to).unwrap());
    }
}

fn provide_is_upgrade_tests() -> Vec<(String, String, bool)> {
    vec![
        ("0.9.0.0".to_string(), "1.0.0.0".to_string(), true),
        ("1.0.0.0".to_string(), "0.9.0.0".to_string(), false),
        (
            "1.0.0.0".to_string(),
            VersionParser::DEFAULT_BRANCH_ALIAS.to_string(),
            true,
        ),
        (
            VersionParser::DEFAULT_BRANCH_ALIAS.to_string(),
            VersionParser::DEFAULT_BRANCH_ALIAS.to_string(),
            true,
        ),
        (
            VersionParser::DEFAULT_BRANCH_ALIAS.to_string(),
            "1.0.0.0".to_string(),
            false,
        ),
        ("1.0.0.0".to_string(), "dev-foo".to_string(), true),
        ("dev-foo".to_string(), "dev-foo".to_string(), true),
        ("dev-foo".to_string(), "1.0.0.0".to_string(), true),
    ]
}
