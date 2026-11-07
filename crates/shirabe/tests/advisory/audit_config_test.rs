//! ref: composer/tests/Composer/Test/Advisory/AuditConfigTest.php sha1:ec83fc0e007309d7d47241f7308ab88f3800efd9

use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe::advisory::{AuditConfig, Auditor};
use shirabe::config::Config;
use shirabe_php_shim::{PhpMixed, ToPhpString};

fn arr(pairs: Vec<(&BStr, PhpMixed)>) -> PhpMixed {
    PhpMixed::Array(
        pairs
            .into_iter()
            .map(|(k, v)| (k.to_php_string(), v))
            .collect(),
    )
}

fn list(items: Vec<PhpMixed>) -> PhpMixed {
    PhpMixed::List(items)
}

fn s(v: &BStr) -> PhpMixed {
    PhpMixed::string(v.to_php_string())
}

fn ignore_list(pairs: Vec<(&BStr, Option<&BStr>)>) -> IndexMap<BString, Option<BString>> {
    pairs
        .into_iter()
        .map(|(k, v)| (BString::from(k), v.map(BString::from)))
        .collect()
}

fn audit_config_from(audit_section: PhpMixed) -> anyhow::Result<AuditConfig> {
    let mut config = Config::new(true, None);
    let mut top: IndexMap<BString, PhpMixed> = IndexMap::new();
    top.insert(
        "config".into(),
        arr(vec![(BStr::new("audit"), audit_section)]),
    );
    config.merge(&top, "test");

    AuditConfig::from_config(&mut config, true, Auditor::FORMAT_SUMMARY)
}

#[test]
fn test_simple_format() {
    let audit_config = audit_config_from(arr(vec![(
        BStr::new("ignore"),
        list(vec![
            s(BStr::new("CVE-2024-1234")),
            s(BStr::new("CVE-2024-5678")),
        ]),
    )]))
    .unwrap();

    assert_eq!(
        ignore_list(vec![
            (BStr::new("CVE-2024-1234"), None),
            (BStr::new("CVE-2024-5678"), None)
        ]),
        audit_config.ignore_list_for_audit
    );
    assert_eq!(
        ignore_list(vec![
            (BStr::new("CVE-2024-1234"), None),
            (BStr::new("CVE-2024-5678"), None)
        ]),
        audit_config.ignore_list_for_blocking
    );
}

#[test]
fn test_detailed_format_audit_only() {
    let audit_config = audit_config_from(arr(vec![(
        BStr::new("ignore"),
        arr(vec![(
            BStr::new("CVE-2024-1234"),
            arr(vec![
                (BStr::new("apply"), s(BStr::new("audit"))),
                (
                    BStr::new("reason"),
                    s(BStr::new("Only ignore for auditing")),
                ),
            ]),
        )]),
    )]))
    .unwrap();

    assert_eq!(
        ignore_list(vec![(
            BStr::new("CVE-2024-1234"),
            Some(BStr::new("Only ignore for auditing"))
        )]),
        audit_config.ignore_list_for_audit
    );
    assert_eq!(ignore_list(vec![]), audit_config.ignore_list_for_blocking);
}

#[test]
fn test_detailed_format_block_only() {
    let audit_config = audit_config_from(arr(vec![(
        BStr::new("ignore"),
        arr(vec![(
            BStr::new("CVE-2024-1234"),
            arr(vec![
                (BStr::new("apply"), s(BStr::new("block"))),
                (
                    BStr::new("reason"),
                    s(BStr::new("Only ignore for blocking")),
                ),
            ]),
        )]),
    )]))
    .unwrap();

    assert_eq!(ignore_list(vec![]), audit_config.ignore_list_for_audit);
    assert_eq!(
        ignore_list(vec![(
            BStr::new("CVE-2024-1234"),
            Some(BStr::new("Only ignore for blocking"))
        )]),
        audit_config.ignore_list_for_blocking
    );
}

#[test]
fn test_mixed_formats() {
    let audit_config = audit_config_from(arr(vec![(
        BStr::new("ignore"),
        arr(vec![
            (BStr::new("0"), s(BStr::new("CVE-2024-1234"))),
            (BStr::new("CVE-2024-5678"), s(BStr::new("Simple reason"))),
            (
                BStr::new("CVE-2024-9999"),
                arr(vec![
                    (BStr::new("apply"), s(BStr::new("audit"))),
                    (BStr::new("reason"), s(BStr::new("Detailed reason"))),
                ]),
            ),
            (
                BStr::new("CVE-2024-8888"),
                arr(vec![(BStr::new("apply"), s(BStr::new("block")))]),
            ),
        ]),
    )]))
    .unwrap();

    assert_eq!(
        ignore_list(vec![
            (BStr::new("CVE-2024-1234"), None),
            (BStr::new("CVE-2024-5678"), Some(BStr::new("Simple reason"))),
            (
                BStr::new("CVE-2024-9999"),
                Some(BStr::new("Detailed reason"))
            ),
        ]),
        audit_config.ignore_list_for_audit
    );
    assert_eq!(
        ignore_list(vec![
            (BStr::new("CVE-2024-1234"), None),
            (BStr::new("CVE-2024-5678"), Some(BStr::new("Simple reason"))),
            (BStr::new("CVE-2024-8888"), None),
        ]),
        audit_config.ignore_list_for_blocking
    );
}

#[test]
fn test_ignore_severity_simple_array() {
    let audit_config = audit_config_from(arr(vec![(
        BStr::new("ignore-severity"),
        list(vec![s(BStr::new("low")), s(BStr::new("medium"))]),
    )]))
    .unwrap();

    assert_eq!(
        ignore_list(vec![(BStr::new("low"), None), (BStr::new("medium"), None)]),
        audit_config.ignore_severity_for_audit
    );
    assert_eq!(
        ignore_list(vec![(BStr::new("low"), None), (BStr::new("medium"), None)]),
        audit_config.ignore_severity_for_blocking
    );
}

#[test]
fn test_ignore_severity_detailed_format() {
    let audit_config = audit_config_from(arr(vec![(
        BStr::new("ignore-severity"),
        arr(vec![
            (
                BStr::new("low"),
                arr(vec![
                    (BStr::new("apply"), s(BStr::new("audit"))),
                    (
                        BStr::new("reason"),
                        s(BStr::new("We accept low severity issues")),
                    ),
                ]),
            ),
            (
                BStr::new("medium"),
                arr(vec![(BStr::new("apply"), s(BStr::new("block")))]),
            ),
        ]),
    )]))
    .unwrap();

    assert_eq!(
        ignore_list(vec![(
            BStr::new("low"),
            Some(BStr::new("We accept low severity issues"))
        )]),
        audit_config.ignore_severity_for_audit
    );
    assert_eq!(
        ignore_list(vec![(BStr::new("medium"), None)]),
        audit_config.ignore_severity_for_blocking
    );
}

#[test]
fn test_ignore_abandoned_simple_format() {
    let audit_config = audit_config_from(arr(vec![(
        BStr::new("ignore-abandoned"),
        list(vec![
            s(BStr::new("vendor/package1")),
            s(BStr::new("vendor/package2")),
        ]),
    )]))
    .unwrap();

    assert_eq!(
        ignore_list(vec![
            (BStr::new("vendor/package1"), None),
            (BStr::new("vendor/package2"), None)
        ]),
        audit_config.ignore_abandoned_for_audit
    );
    assert_eq!(
        ignore_list(vec![
            (BStr::new("vendor/package1"), None),
            (BStr::new("vendor/package2"), None)
        ]),
        audit_config.ignore_abandoned_for_blocking
    );
}

#[test]
fn test_ignore_abandoned_detailed_format() {
    let audit_config = audit_config_from(arr(vec![(
        BStr::new("ignore-abandoned"),
        arr(vec![
            (
                BStr::new("vendor/package1"),
                arr(vec![
                    (BStr::new("apply"), s(BStr::new("audit"))),
                    (BStr::new("reason"), s(BStr::new("Report but do not block"))),
                ]),
            ),
            (
                BStr::new("vendor/package2"),
                arr(vec![
                    (BStr::new("apply"), s(BStr::new("block"))),
                    (BStr::new("reason"), s(BStr::new("Block but do not report"))),
                ]),
            ),
        ]),
    )]))
    .unwrap();

    assert_eq!(
        ignore_list(vec![(
            BStr::new("vendor/package1"),
            Some(BStr::new("Report but do not block"))
        )]),
        audit_config.ignore_abandoned_for_audit
    );
    assert_eq!(
        ignore_list(vec![(
            BStr::new("vendor/package2"),
            Some(BStr::new("Block but do not report"))
        )]),
        audit_config.ignore_abandoned_for_blocking
    );
}

#[test]
fn test_invalid_apply_value() {
    let result = audit_config_from(arr(vec![(
        BStr::new("ignore"),
        arr(vec![(
            BStr::new("CVE-2024-1234"),
            arr(vec![(BStr::new("apply"), s(BStr::new("invalid")))]),
        )]),
    )]));

    assert!(result.is_err());
}
