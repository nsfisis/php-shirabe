//! ref: composer/tests/Composer/Test/Advisory/AuditorTest.php

use chrono::Utc;
use indexmap::IndexMap;
use shirabe::advisory::AnySecurityAdvisory;
use shirabe::advisory::Auditor;
use shirabe::advisory::PartialSecurityAdvisory;
use shirabe::advisory::SecurityAdvisory;
use shirabe_semver::constraint::SimpleConstraint;

fn constraint(operator: &str, version: &str) -> shirabe_semver::constraint::AnyConstraint {
    SimpleConstraint::new(operator.to_string(), version.to_string(), None).into()
}

fn full_advisory() -> AnySecurityAdvisory {
    let mut source: IndexMap<String, String> = IndexMap::new();
    source.insert("name".to_string(), "foo".to_string());
    source.insert("remoteId".to_string(), "remoteID".to_string());
    AnySecurityAdvisory::Full(SecurityAdvisory::new(
        "foo/bar".to_string(),
        "123".to_string(),
        constraint("=", "1.0.0.0"),
        "test".to_string(),
        vec![source],
        Utc::now(),
        None,
        None,
        None,
    ))
}

fn full_advisory_with_id(advisory_id: &str) -> AnySecurityAdvisory {
    let mut source: IndexMap<String, String> = IndexMap::new();
    source.insert("name".to_string(), "foo".to_string());
    source.insert("remoteId".to_string(), "remoteID".to_string());
    AnySecurityAdvisory::Full(SecurityAdvisory::new(
        "foo/bar".to_string(),
        advisory_id.to_string(),
        constraint("=", "1.0.0.0"),
        "test".to_string(),
        vec![source],
        Utc::now(),
        None,
        None,
        None,
    ))
}

fn partial_advisory(advisory_id: &str) -> AnySecurityAdvisory {
    AnySecurityAdvisory::Partial(PartialSecurityAdvisory::new(
        "foo/bar".to_string(),
        advisory_id.to_string(),
        constraint("=", "1.0.0.0"),
    ))
}

fn ignore_list(pairs: Vec<(&str, Option<&str>)>) -> IndexMap<String, Option<String>> {
    pairs
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.map(String::from)))
        .collect()
}

// These run the Auditor against a mocked HttpDownloader/IO and packages built from version
// constraints (parsed via a look-around regex the regex crate cannot compile).
#[test]
#[ignore = "requires PHPUnit getMockBuilder partial mock of ComposerRepository (hasSecurityAdvisories/getSecurityAdvisories) and BufferIO; no mocking infrastructure exists"]
fn test_audit() {
    todo!()
}

#[test]
#[ignore = "requires getIOMock with expects() output expectations and getMockBuilder mock of ComposerRepository; no mocking infrastructure exists"]
fn test_audit_with_ignore() {
    todo!()
}

#[test]
#[ignore = "requires getMockBuilder partial mock of RepositorySet (getMatchingSecurityAdvisories willReturnCallback); no mocking infrastructure exists"]
fn test_audit_with_ignore_unreachable() {
    todo!()
}

#[test]
#[ignore = "requires getIOMock with expects() output expectations and getMockBuilder mock of ComposerRepository; no mocking infrastructure exists"]
fn test_audit_with_ignore_severity() {
    todo!()
}

#[test]
fn test_needs_complete_advisory_load() {
    let cases: Vec<(
        IndexMap<String, Vec<AnySecurityAdvisory>>,
        IndexMap<String, Option<String>>,
        bool,
    )> = vec![
        // no filter or advisories
        (IndexMap::new(), ignore_list(vec![]), false),
        // packagist filters are IDs so work fine with partial advisories
        (
            IndexMap::new(),
            ignore_list(vec![("PKSA-foo-bar", None)]),
            false,
        ),
        // packagist filters are IDs so work fine with partial advisories/2
        (
            {
                let mut m: IndexMap<String, Vec<AnySecurityAdvisory>> = IndexMap::new();
                m.insert(
                    "vendor1/package1".to_string(),
                    vec![full_advisory(), partial_advisory("1234")],
                );
                m
            },
            ignore_list(vec![("PKSA-foo-bar", Some("this is fine 🔥"))]),
            false,
        ),
        // no advisories no need to load any further
        (
            IndexMap::new(),
            ignore_list(vec![("CVE-2025-1234", None)]),
            false,
        ),
        // no advisories no need to load any further/2
        (
            {
                let mut m: IndexMap<String, Vec<AnySecurityAdvisory>> = IndexMap::new();
                m.insert("vendor1/package1".to_string(), vec![]);
                m
            },
            ignore_list(vec![("CVE-2025-1234", None)]),
            false,
        ),
        // CVE filter or other non-packagist ones might need to fully load for safety if partial advisories are present
        (
            {
                let mut m: IndexMap<String, Vec<AnySecurityAdvisory>> = IndexMap::new();
                m.insert(
                    "vendor1/package1".to_string(),
                    vec![full_advisory(), partial_advisory("1234")],
                );
                m
            },
            ignore_list(vec![("CVE-2025-1234", None)]),
            true,
        ),
        // filter does not trigger load if all advisories are fully loaded
        (
            {
                let mut m: IndexMap<String, Vec<AnySecurityAdvisory>> = IndexMap::new();
                m.insert("vendor1/package1".to_string(), vec![full_advisory()]);
                m.insert(
                    "vendor1/package2".to_string(),
                    vec![full_advisory_with_id("1234")],
                );
                m
            },
            ignore_list(vec![("CVE-2025-1234", None)]),
            false,
        ),
    ];

    let auditor = Auditor;
    for (advisories, ignore_list, expected) in cases {
        assert_eq!(
            expected,
            auditor.needs_complete_advisory_load(&advisories, &ignore_list)
        );
    }
}
