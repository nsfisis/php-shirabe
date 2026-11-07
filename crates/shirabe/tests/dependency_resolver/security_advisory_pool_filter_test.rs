//! ref: composer/tests/Composer/Test/DependencyResolver/SecurityAdvisoryPoolFilterTest.php sha1:85b97377005b1d7c540c946f9172976e15693ad9

// The filter parses each advisory's affectedVersions (e.g. ">=1.0.0,<1.1.0") into a
// constraint, which goes through the version parser's look-around regex that the regex
// crate cannot compile. The fixtures also build PackageRepository security-advisory data
// and run the Auditor.

use bstr::{B, BStr, BString};
use indexmap::IndexMap;
use shirabe::advisory::{AuditConfig, Auditor};
use shirabe::dependency_resolver::{Pool, Request, SecurityAdvisoryPoolFilter};
use shirabe::package::{CompletePackageHandle, PackageHandle, PackageInterfaceHandle};
use shirabe::repository::{PackageRepository, RepositoryInterfaceHandle};
use shirabe_php_shim::{PhpMixed, ToPhpString, uniqid};
use shirabe_semver::constraint::{AnyConstraint, SimpleConstraint};

#[test]
fn test_filter_packages_by_advisories() {
    let audit_config = AuditConfig::new(
        true,
        bstr::BString::from(Auditor::FORMAT_SUMMARY),
        Auditor::ABANDONED_FAIL.to_owned(),
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

    let advisory1 = generate_security_advisory(
        BStr::new("acme/package"),
        Some(BStr::new("CVE-1999-1000")),
        BStr::new(">=1.0.0,<1.1.0"),
    );
    let advisory2 = generate_security_advisory(
        BStr::new("acme/package"),
        Some(BStr::new("CVE-1999-1001")),
        BStr::new(">=1.0.0,<1.1.0"),
    );
    let mut security_advisories: IndexMap<BString, PhpMixed> = IndexMap::new();
    security_advisories.insert(
        "acme/package".into(),
        PhpMixed::List(vec![
            PhpMixed::Array(advisory1.clone()),
            PhpMixed::Array(advisory2.clone()),
        ]),
    );
    let mut config: IndexMap<BString, PhpMixed> = IndexMap::new();
    config.insert("package".into(), PhpMixed::List(vec![]));
    config.insert(
        "security-advisories".into(),
        PhpMixed::Array(security_advisories),
    );
    let repository = RepositoryInterfaceHandle::new(PackageRepository::new(config));

    let package: PackageInterfaceHandle = PackageHandle::new(
        BString::from("acme/package"),
        BString::from("1.0.0.0"),
        BString::from("1.0"),
    )
    .into();
    let expected_package1: PackageInterfaceHandle = PackageHandle::new(
        BString::from("acme/package"),
        BString::from("2.0.0.0"),
        BString::from("2.0"),
    )
    .into();
    let expected_package2: PackageInterfaceHandle = PackageHandle::new(
        BString::from("acme/other"),
        BString::from("1.0.0.0"),
        BString::from("1.0"),
    )
    .into();

    let pool = Pool::new(
        vec![
            package,
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
        SimpleConstraint::new("==", BString::from("1.0.0.0"), None).into();
    assert!(
        filtered_pool.is_security_removed_package_version(
            bstr::BStr::new("acme/package"),
            Some(&constraint)
        )
    );
    assert_eq!(
        filtered_pool
            .get_all_abandoned_removed_package_versions()
            .len(),
        0
    );

    let advisory_map = filtered_pool.get_all_security_removed_package_versions();
    assert!(advisory_map.contains_key(bstr::B("acme/package")));
    assert!(advisory_map[bstr::B("acme/package")].contains_key(bstr::B("1.0.0.0")));
    assert_eq!(
        vec![
            advisory1[B("advisoryId")].as_string().unwrap().to_owned(),
            advisory2[B("advisoryId")].as_string().unwrap().to_owned(),
        ],
        filtered_pool.get_security_advisory_identifiers_for_package_version(
            bstr::BStr::new("acme/package"),
            Some(&constraint)
        ),
    );
}

#[test]
fn test_dont_filter_packages_by_ignored_advisories() {
    let mut ignore_list: IndexMap<BString, Option<BString>> = IndexMap::new();
    ignore_list.insert(BString::from("CVE-2024-1234"), None);
    let audit_config = AuditConfig::new(
        true,
        bstr::BString::from(Auditor::FORMAT_SUMMARY),
        Auditor::ABANDONED_FAIL.to_owned(),
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

    let mut security_advisories: IndexMap<BString, PhpMixed> = IndexMap::new();
    security_advisories.insert(
        "acme/package".into(),
        PhpMixed::List(vec![PhpMixed::Array(generate_security_advisory(
            BStr::new("acme/package"),
            Some(BStr::new("CVE-2024-1234")),
            BStr::new(">=1.0.0,<1.1.0"),
        ))]),
    );
    let mut config: IndexMap<BString, PhpMixed> = IndexMap::new();
    config.insert("package".into(), PhpMixed::List(vec![]));
    config.insert(
        "security-advisories".into(),
        PhpMixed::Array(security_advisories),
    );
    let repository = RepositoryInterfaceHandle::new(PackageRepository::new(config));

    let expected_package1: PackageInterfaceHandle = PackageHandle::new(
        BString::from("acme/package"),
        BString::from("1.0.0.0"),
        BString::from("1.0"),
    )
    .into();
    let expected_package2: PackageInterfaceHandle = PackageHandle::new(
        BString::from("acme/package"),
        BString::from("1.1.0.0"),
        BString::from("1.1"),
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
        bstr::BString::from(Auditor::FORMAT_SUMMARY),
        Auditor::ABANDONED_FAIL.to_owned(),
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

    let mut security_advisories: IndexMap<BString, PhpMixed> = IndexMap::new();
    security_advisories.insert(
        "acme/package".into(),
        PhpMixed::List(vec![PhpMixed::Array(generate_security_advisory(
            BStr::new("acme/package"),
            Some(BStr::new("CVE-2024-1234")),
            BStr::new(">=1.0.0,<1.1.0"),
        ))]),
    );
    let mut config: IndexMap<BString, PhpMixed> = IndexMap::new();
    config.insert("package".into(), PhpMixed::List(vec![]));
    config.insert(
        "security-advisories".into(),
        PhpMixed::Array(security_advisories),
    );
    let repository = RepositoryInterfaceHandle::new(PackageRepository::new(config));

    let expected_package1: PackageInterfaceHandle = PackageHandle::new(
        BString::from("acme/package"),
        BString::from("1.0.0.0"),
        BString::from("1.0"),
    )
    .into();
    let expected_package2: PackageInterfaceHandle = PackageHandle::new(
        BString::from("acme/package"),
        BString::from("1.1.0.0"),
        BString::from("1.1"),
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
    let mut ignore_abandoned: IndexMap<BString, Option<BString>> = IndexMap::new();
    ignore_abandoned.insert(BString::from(package_name_ignore_abandoned), None);
    let audit_config = AuditConfig::new(
        true,
        bstr::BString::from(Auditor::FORMAT_SUMMARY),
        Auditor::ABANDONED_FAIL.to_owned(),
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
        BString::from("acme/package"),
        BString::from("1.0.0.0"),
        BString::from("1.0"),
    );
    abandoned_package.set_abandoned(PhpMixed::Bool(true));
    let ignore_abandoned_package = CompletePackageHandle::new(
        package_name_ignore_abandoned.to_php_string(),
        BString::from("1.0.0.0"),
        BString::from("1.0"),
    );
    ignore_abandoned_package.set_abandoned(PhpMixed::Bool(true));
    let expected_package = PackageHandle::new(
        BString::from("acme/other"),
        BString::from("1.1.0.0"),
        BString::from("1.1"),
    );

    let expected_package: PackageInterfaceHandle = expected_package.into();
    let abandoned_package: PackageInterfaceHandle = abandoned_package.into();
    let ignore_abandoned_package: PackageInterfaceHandle = ignore_abandoned_package.into();

    let pool = Pool::new(
        vec![
            expected_package.clone(),
            abandoned_package,
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
    package_name: &BStr,
    cve: Option<&BStr>,
    affected_versions: &BStr,
) -> IndexMap<BString, PhpMixed> {
    let mut source: IndexMap<BString, PhpMixed> = IndexMap::new();
    source.insert(
        "name".into(),
        PhpMixed::string(BString::from("Security Advisory")),
    );
    source.insert("remoteId".into(), PhpMixed::string(BString::from("test")));

    let mut advisory: IndexMap<BString, PhpMixed> = IndexMap::new();
    advisory.insert("advisoryId".into(), PhpMixed::string(uniqid("PKSA-")));
    advisory.insert(
        "packageName".into(),
        PhpMixed::string(package_name.to_php_string()),
    );
    advisory.insert("remoteId".into(), PhpMixed::string(BString::from("test")));
    advisory.insert(
        "title".into(),
        PhpMixed::string(BString::from("Security Advisory")),
    );
    advisory.insert("link".into(), PhpMixed::Null);
    advisory.insert(
        "cve".into(),
        match cve {
            Some(cve) => PhpMixed::string(cve.to_php_string()),
            None => PhpMixed::Null,
        },
    );
    advisory.insert(
        "affectedVersions".into(),
        PhpMixed::string(affected_versions.to_php_string()),
    );
    advisory.insert("source".into(), PhpMixed::string(BString::from("Tests")));
    advisory.insert(
        "reportedAt".into(),
        PhpMixed::string(BString::from("2024-04-31 12:37:47")),
    );
    advisory.insert(
        "composerRepository".into(),
        PhpMixed::string(BString::from("Package Repository")),
    );
    advisory.insert("severity".into(), PhpMixed::string(BString::from("high")));
    advisory.insert(
        "sources".into(),
        PhpMixed::List(vec![PhpMixed::Array(source)]),
    );
    advisory
}
