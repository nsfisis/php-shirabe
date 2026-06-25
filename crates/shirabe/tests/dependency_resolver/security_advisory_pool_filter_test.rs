//! ref: composer/tests/Composer/Test/DependencyResolver/SecurityAdvisoryPoolFilterTest.php

// The filter parses each advisory's affectedVersions (e.g. ">=1.0.0,<1.1.0") into a
// constraint, which goes through the version parser's look-around regex that the regex
// crate cannot compile. The fixtures also build PackageRepository security-advisory data
// and run the Auditor.

use indexmap::IndexMap;
use shirabe::advisory::AuditConfig;
use shirabe::advisory::Auditor;
use shirabe::dependency_resolver::SecurityAdvisoryPoolFilter;
use shirabe::dependency_resolver::pool::Pool;
use shirabe::dependency_resolver::request::Request;
use shirabe::package::handle::{CompletePackageHandle, PackageHandle, PackageInterfaceHandle};
use shirabe::repository::{PackageRepository, RepositoryInterfaceHandle};
use shirabe_php_shim::{PhpMixed, uniqid};
use shirabe_semver::constraint::{AnyConstraint, SimpleConstraint};

#[test]
fn test_filter_packages_by_advisories() {
    let audit_config = AuditConfig::new(
        true,
        Auditor::FORMAT_SUMMARY.to_string(),
        Auditor::ABANDONED_FAIL.to_string(),
        true,
        true,
        false,
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
    );
    let filter = SecurityAdvisoryPoolFilter::new(Auditor, audit_config);

    let advisory1 =
        generate_security_advisory("acme/package", Some("CVE-1999-1000"), ">=1.0.0,<1.1.0");
    let advisory2 =
        generate_security_advisory("acme/package", Some("CVE-1999-1001"), ">=1.0.0,<1.1.0");
    let mut security_advisories: IndexMap<String, PhpMixed> = IndexMap::new();
    security_advisories.insert(
        "acme/package".to_string(),
        PhpMixed::List(vec![
            PhpMixed::Array(advisory1.clone()),
            PhpMixed::Array(advisory2.clone()),
        ]),
    );
    let mut config: IndexMap<String, PhpMixed> = IndexMap::new();
    config.insert("package".to_string(), PhpMixed::List(vec![]));
    config.insert(
        "security-advisories".to_string(),
        PhpMixed::Array(security_advisories),
    );
    let repository = RepositoryInterfaceHandle::new(PackageRepository::new(config));

    let package: PackageInterfaceHandle = PackageHandle::new(
        "acme/package".to_string(),
        "1.0.0.0".to_string(),
        "1.0".to_string(),
    )
    .into();
    let expected_package1: PackageInterfaceHandle = PackageHandle::new(
        "acme/package".to_string(),
        "2.0.0.0".to_string(),
        "2.0".to_string(),
    )
    .into();
    let expected_package2: PackageInterfaceHandle = PackageHandle::new(
        "acme/other".to_string(),
        "1.0.0.0".to_string(),
        "1.0".to_string(),
    )
    .into();

    let pool = Pool::new(
        vec![
            package.clone(),
            expected_package1.clone(),
            expected_package2.clone(),
        ],
        vec![],
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
    );
    let filtered_pool = filter
        .filter(pool, vec![repository], &Request::new(None))
        .unwrap();

    let packages = filtered_pool.get_packages();
    assert_eq!(packages.len(), 2);
    assert!(packages[0].ptr_eq(&expected_package1));
    assert!(packages[1].ptr_eq(&expected_package2));

    let constraint: AnyConstraint =
        SimpleConstraint::new("==".to_string(), "1.0.0.0".to_string(), None).into();
    assert!(filtered_pool.is_security_removed_package_version("acme/package", Some(&constraint)));
    assert_eq!(
        filtered_pool
            .get_all_abandoned_removed_package_versions()
            .len(),
        0
    );

    let advisory_map = filtered_pool.get_all_security_removed_package_versions();
    assert!(advisory_map.contains_key("acme/package"));
    assert!(advisory_map["acme/package"].contains_key("1.0.0.0"));
    assert_eq!(
        vec![
            advisory1["advisoryId"].as_string().unwrap().to_string(),
            advisory2["advisoryId"].as_string().unwrap().to_string(),
        ],
        filtered_pool.get_security_advisory_identifiers_for_package_version(
            "acme/package",
            Some(&constraint)
        ),
    );
}

#[test]
fn test_dont_filter_packages_by_ignored_advisories() {
    let mut ignore_list: IndexMap<String, Option<String>> = IndexMap::new();
    ignore_list.insert("CVE-2024-1234".to_string(), None);
    let audit_config = AuditConfig::new(
        true,
        Auditor::FORMAT_SUMMARY.to_string(),
        Auditor::ABANDONED_FAIL.to_string(),
        true,
        true,
        false,
        ignore_list.clone(),
        ignore_list,
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
    );
    let filter = SecurityAdvisoryPoolFilter::new(Auditor, audit_config);

    let mut security_advisories: IndexMap<String, PhpMixed> = IndexMap::new();
    security_advisories.insert(
        "acme/package".to_string(),
        PhpMixed::List(vec![PhpMixed::Array(generate_security_advisory(
            "acme/package",
            Some("CVE-2024-1234"),
            ">=1.0.0,<1.1.0",
        ))]),
    );
    let mut config: IndexMap<String, PhpMixed> = IndexMap::new();
    config.insert("package".to_string(), PhpMixed::List(vec![]));
    config.insert(
        "security-advisories".to_string(),
        PhpMixed::Array(security_advisories),
    );
    let repository = RepositoryInterfaceHandle::new(PackageRepository::new(config));

    let expected_package1: PackageInterfaceHandle = PackageHandle::new(
        "acme/package".to_string(),
        "1.0.0.0".to_string(),
        "1.0".to_string(),
    )
    .into();
    let expected_package2: PackageInterfaceHandle = PackageHandle::new(
        "acme/package".to_string(),
        "1.1.0.0".to_string(),
        "1.1".to_string(),
    )
    .into();

    let pool = Pool::new(
        vec![expected_package1.clone(), expected_package2.clone()],
        vec![],
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
    );
    let filtered_pool = filter
        .filter(pool, vec![repository], &Request::new(None))
        .unwrap();

    let packages = filtered_pool.get_packages();
    assert_eq!(packages.len(), 2);
    assert!(packages[0].ptr_eq(&expected_package1));
    assert!(packages[1].ptr_eq(&expected_package2));
    assert_eq!(
        filtered_pool
            .get_all_abandoned_removed_package_versions()
            .len(),
        0
    );
    assert_eq!(
        filtered_pool
            .get_all_security_removed_package_versions()
            .len(),
        0
    );
}

#[test]
fn test_dont_filter_packages_with_block_insecure_disabled() {
    let audit_config = AuditConfig::new(
        true,
        Auditor::FORMAT_SUMMARY.to_string(),
        Auditor::ABANDONED_FAIL.to_string(),
        false,
        true,
        false,
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
    );
    let filter = SecurityAdvisoryPoolFilter::new(Auditor, audit_config);

    let mut security_advisories: IndexMap<String, PhpMixed> = IndexMap::new();
    security_advisories.insert(
        "acme/package".to_string(),
        PhpMixed::List(vec![PhpMixed::Array(generate_security_advisory(
            "acme/package",
            Some("CVE-2024-1234"),
            ">=1.0.0,<1.1.0",
        ))]),
    );
    let mut config: IndexMap<String, PhpMixed> = IndexMap::new();
    config.insert("package".to_string(), PhpMixed::List(vec![]));
    config.insert(
        "security-advisories".to_string(),
        PhpMixed::Array(security_advisories),
    );
    let repository = RepositoryInterfaceHandle::new(PackageRepository::new(config));

    let expected_package1: PackageInterfaceHandle = PackageHandle::new(
        "acme/package".to_string(),
        "1.0.0.0".to_string(),
        "1.0".to_string(),
    )
    .into();
    let expected_package2: PackageInterfaceHandle = PackageHandle::new(
        "acme/package".to_string(),
        "1.1.0.0".to_string(),
        "1.1".to_string(),
    )
    .into();

    let pool = Pool::new(
        vec![expected_package1.clone(), expected_package2.clone()],
        vec![],
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
    );
    let filtered_pool = filter
        .filter(pool, vec![repository], &Request::new(None))
        .unwrap();

    let packages = filtered_pool.get_packages();
    assert_eq!(packages.len(), 2);
    assert!(packages[0].ptr_eq(&expected_package1));
    assert!(packages[1].ptr_eq(&expected_package2));
    assert_eq!(
        filtered_pool
            .get_all_abandoned_removed_package_versions()
            .len(),
        0
    );
    assert_eq!(
        filtered_pool
            .get_all_security_removed_package_versions()
            .len(),
        0
    );
}

#[test]
fn test_dont_filter_packages_with_abandoned_package() {
    let package_name_ignore_abandoned = "acme/ignore-abandoned";
    let mut ignore_abandoned: IndexMap<String, Option<String>> = IndexMap::new();
    ignore_abandoned.insert(package_name_ignore_abandoned.to_string(), None);
    let audit_config = AuditConfig::new(
        true,
        Auditor::FORMAT_SUMMARY.to_string(),
        Auditor::ABANDONED_FAIL.to_string(),
        true,
        true,
        false,
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
        ignore_abandoned.clone(),
        ignore_abandoned,
    );
    let filter = SecurityAdvisoryPoolFilter::new(Auditor, audit_config);

    let abandoned_package = CompletePackageHandle::new(
        "acme/package".to_string(),
        "1.0.0.0".to_string(),
        "1.0".to_string(),
    );
    abandoned_package.set_abandoned(PhpMixed::Bool(true));
    let ignore_abandoned_package = CompletePackageHandle::new(
        package_name_ignore_abandoned.to_string(),
        "1.0.0.0".to_string(),
        "1.0".to_string(),
    );
    ignore_abandoned_package.set_abandoned(PhpMixed::Bool(true));
    let expected_package = PackageHandle::new(
        "acme/other".to_string(),
        "1.1.0.0".to_string(),
        "1.1".to_string(),
    );

    let expected_package: PackageInterfaceHandle = expected_package.into();
    let abandoned_package: PackageInterfaceHandle = abandoned_package.into();
    let ignore_abandoned_package: PackageInterfaceHandle = ignore_abandoned_package.into();

    let pool = Pool::new(
        vec![
            expected_package.clone(),
            abandoned_package.clone(),
            ignore_abandoned_package.clone(),
        ],
        vec![],
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
    );
    let filtered_pool = filter.filter(pool, vec![], &Request::new(None)).unwrap();

    let packages = filtered_pool.get_packages();
    assert_eq!(packages.len(), 2);
    assert!(packages[0].ptr_eq(&expected_package));
    assert!(packages[1].ptr_eq(&ignore_abandoned_package));
    assert_eq!(
        filtered_pool
            .get_all_abandoned_removed_package_versions()
            .len(),
        1
    );
    assert_eq!(
        filtered_pool
            .get_all_security_removed_package_versions()
            .len(),
        0
    );
}

fn generate_security_advisory(
    package_name: &str,
    cve: Option<&str>,
    affected_versions: &str,
) -> IndexMap<String, PhpMixed> {
    let mut source: IndexMap<String, PhpMixed> = IndexMap::new();
    source.insert(
        "name".to_string(),
        PhpMixed::String("Security Advisory".to_string()),
    );
    source.insert("remoteId".to_string(), PhpMixed::String("test".to_string()));

    let mut advisory: IndexMap<String, PhpMixed> = IndexMap::new();
    advisory.insert(
        "advisoryId".to_string(),
        PhpMixed::String(uniqid("PKSA-", false)),
    );
    advisory.insert(
        "packageName".to_string(),
        PhpMixed::String(package_name.to_string()),
    );
    advisory.insert("remoteId".to_string(), PhpMixed::String("test".to_string()));
    advisory.insert(
        "title".to_string(),
        PhpMixed::String("Security Advisory".to_string()),
    );
    advisory.insert("link".to_string(), PhpMixed::Null);
    advisory.insert(
        "cve".to_string(),
        match cve {
            Some(cve) => PhpMixed::String(cve.to_string()),
            None => PhpMixed::Null,
        },
    );
    advisory.insert(
        "affectedVersions".to_string(),
        PhpMixed::String(affected_versions.to_string()),
    );
    advisory.insert("source".to_string(), PhpMixed::String("Tests".to_string()));
    advisory.insert(
        "reportedAt".to_string(),
        PhpMixed::String("2024-04-31 12:37:47".to_string()),
    );
    advisory.insert(
        "composerRepository".to_string(),
        PhpMixed::String("Package Repository".to_string()),
    );
    advisory.insert("severity".to_string(), PhpMixed::String("high".to_string()));
    advisory.insert(
        "sources".to_string(),
        PhpMixed::List(vec![PhpMixed::Array(source)]),
    );
    advisory
}
