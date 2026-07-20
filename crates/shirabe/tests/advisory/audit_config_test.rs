//! ref: composer/tests/Composer/Test/Advisory/AuditConfigTest.php

use indexmap::IndexMap;
use shirabe::advisory::AuditConfig;
use shirabe::advisory::Auditor;
use shirabe::config::Config;
use shirabe_php_shim::PhpMixed;

fn arr(pairs: Vec<(&str, PhpMixed)>) -> PhpMixed {
    PhpMixed::Array(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn list(items: Vec<PhpMixed>) -> PhpMixed {
    PhpMixed::List(items)
}

fn s(v: &str) -> PhpMixed {
    PhpMixed::String(v.to_string())
}

fn ignore_list(pairs: Vec<(&str, Option<&str>)>) -> IndexMap<String, Option<String>> {
    pairs
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.map(String::from)))
        .collect()
}

fn audit_config_from(audit_section: PhpMixed) -> anyhow::Result<AuditConfig> {
    let mut config = Config::new(true, None);
    let mut top: IndexMap<String, PhpMixed> = IndexMap::new();
    top.insert("config".to_string(), arr(vec![("audit", audit_section)]));
    config.merge(&top, "test");

    AuditConfig::from_config(&mut config, true, Auditor::FORMAT_SUMMARY)
}

#[test]
fn test_simple_format() {
    let audit_config = audit_config_from(arr(vec![(
        "ignore",
        list(vec![s("CVE-2024-1234"), s("CVE-2024-5678")]),
    )]))
    .unwrap();

    assert_eq!(
        ignore_list(vec![("CVE-2024-1234", None), ("CVE-2024-5678", None)]),
        audit_config.ignore_list_for_audit
    );
    assert_eq!(
        ignore_list(vec![("CVE-2024-1234", None), ("CVE-2024-5678", None)]),
        audit_config.ignore_list_for_blocking
    );
}

#[test]
fn test_detailed_format_audit_only() {
    let audit_config = audit_config_from(arr(vec![(
        "ignore",
        arr(vec![(
            "CVE-2024-1234",
            arr(vec![
                ("apply", s("audit")),
                ("reason", s("Only ignore for auditing")),
            ]),
        )]),
    )]))
    .unwrap();

    assert_eq!(
        ignore_list(vec![("CVE-2024-1234", Some("Only ignore for auditing"))]),
        audit_config.ignore_list_for_audit
    );
    assert_eq!(ignore_list(vec![]), audit_config.ignore_list_for_blocking);
}

#[test]
fn test_detailed_format_block_only() {
    let audit_config = audit_config_from(arr(vec![(
        "ignore",
        arr(vec![(
            "CVE-2024-1234",
            arr(vec![
                ("apply", s("block")),
                ("reason", s("Only ignore for blocking")),
            ]),
        )]),
    )]))
    .unwrap();

    assert_eq!(ignore_list(vec![]), audit_config.ignore_list_for_audit);
    assert_eq!(
        ignore_list(vec![("CVE-2024-1234", Some("Only ignore for blocking"))]),
        audit_config.ignore_list_for_blocking
    );
}

#[test]
fn test_mixed_formats() {
    let audit_config = audit_config_from(arr(vec![(
        "ignore",
        arr(vec![
            ("0", s("CVE-2024-1234")),
            ("CVE-2024-5678", s("Simple reason")),
            (
                "CVE-2024-9999",
                arr(vec![
                    ("apply", s("audit")),
                    ("reason", s("Detailed reason")),
                ]),
            ),
            ("CVE-2024-8888", arr(vec![("apply", s("block"))])),
        ]),
    )]))
    .unwrap();

    assert_eq!(
        ignore_list(vec![
            ("CVE-2024-1234", None),
            ("CVE-2024-5678", Some("Simple reason")),
            ("CVE-2024-9999", Some("Detailed reason")),
        ]),
        audit_config.ignore_list_for_audit
    );
    assert_eq!(
        ignore_list(vec![
            ("CVE-2024-1234", None),
            ("CVE-2024-5678", Some("Simple reason")),
            ("CVE-2024-8888", None),
        ]),
        audit_config.ignore_list_for_blocking
    );
}

#[test]
fn test_ignore_severity_simple_array() {
    let audit_config = audit_config_from(arr(vec![(
        "ignore-severity",
        list(vec![s("low"), s("medium")]),
    )]))
    .unwrap();

    assert_eq!(
        ignore_list(vec![("low", None), ("medium", None)]),
        audit_config.ignore_severity_for_audit
    );
    assert_eq!(
        ignore_list(vec![("low", None), ("medium", None)]),
        audit_config.ignore_severity_for_blocking
    );
}

#[test]
fn test_ignore_severity_detailed_format() {
    let audit_config = audit_config_from(arr(vec![(
        "ignore-severity",
        arr(vec![
            (
                "low",
                arr(vec![
                    ("apply", s("audit")),
                    ("reason", s("We accept low severity issues")),
                ]),
            ),
            ("medium", arr(vec![("apply", s("block"))])),
        ]),
    )]))
    .unwrap();

    assert_eq!(
        ignore_list(vec![("low", Some("We accept low severity issues"))]),
        audit_config.ignore_severity_for_audit
    );
    assert_eq!(
        ignore_list(vec![("medium", None)]),
        audit_config.ignore_severity_for_blocking
    );
}

#[test]
fn test_ignore_abandoned_simple_format() {
    let audit_config = audit_config_from(arr(vec![(
        "ignore-abandoned",
        list(vec![s("vendor/package1"), s("vendor/package2")]),
    )]))
    .unwrap();

    assert_eq!(
        ignore_list(vec![("vendor/package1", None), ("vendor/package2", None)]),
        audit_config.ignore_abandoned_for_audit
    );
    assert_eq!(
        ignore_list(vec![("vendor/package1", None), ("vendor/package2", None)]),
        audit_config.ignore_abandoned_for_blocking
    );
}

#[test]
fn test_ignore_abandoned_detailed_format() {
    let audit_config = audit_config_from(arr(vec![(
        "ignore-abandoned",
        arr(vec![
            (
                "vendor/package1",
                arr(vec![
                    ("apply", s("audit")),
                    ("reason", s("Report but do not block")),
                ]),
            ),
            (
                "vendor/package2",
                arr(vec![
                    ("apply", s("block")),
                    ("reason", s("Block but do not report")),
                ]),
            ),
        ]),
    )]))
    .unwrap();

    assert_eq!(
        ignore_list(vec![("vendor/package1", Some("Report but do not block"))]),
        audit_config.ignore_abandoned_for_audit
    );
    assert_eq!(
        ignore_list(vec![("vendor/package2", Some("Block but do not report"))]),
        audit_config.ignore_abandoned_for_blocking
    );
}

#[test]
fn test_invalid_apply_value() {
    let result = audit_config_from(arr(vec![(
        "ignore",
        arr(vec![("CVE-2024-1234", arr(vec![("apply", s("invalid"))]))]),
    )]));

    assert!(result.is_err());
}
