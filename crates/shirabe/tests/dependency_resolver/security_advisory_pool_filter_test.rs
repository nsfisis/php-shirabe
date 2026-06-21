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
use shirabe_php_shim::PhpMixed;

#[test]
#[ignore = "requires PackageRepository to implement RepositoryInterface (filter takes Vec<RepositoryInterfaceHandle>) and php-shim uniqid() used by generateSecurityAdvisory; neither exists"]
fn test_filter_packages_by_advisories() {
    todo!()
}

#[test]
#[ignore = "requires PackageRepository to implement RepositoryInterface (filter takes Vec<RepositoryInterfaceHandle>) and php-shim uniqid() used by generateSecurityAdvisory; neither exists"]
fn test_dont_filter_packages_by_ignored_advisories() {
    todo!()
}

#[test]
#[ignore = "requires PackageRepository to implement RepositoryInterface (filter takes Vec<RepositoryInterfaceHandle>) and php-shim uniqid() used by generateSecurityAdvisory; neither exists"]
fn test_dont_filter_packages_with_block_insecure_disabled() {
    todo!()
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
