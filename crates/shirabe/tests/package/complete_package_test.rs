//! ref: composer/tests/Composer/Test/Package/CompletePackageTest.php

use shirabe::package::package::Package;
use shirabe::package::package_interface::PackageInterface;
use shirabe_php_shim::legacy_lossy;
use shirabe_semver::VersionParser;

/// Memory package naming, versioning, and marshalling semantics provider
/// demonstrates several versioning schemes
fn provider_versioning_schemes() -> Vec<(&'static str, &'static str)> {
    vec![
        ("foo", "1-beta"),
        ("node", "0.5.6"),
        ("li3", "0.10"),
        ("mongodb_odm", "1.0.0BETA3"),
        ("DoctrineCommon", "2.2.0-DEV"),
    ]
}

#[test]
fn test_package_has_expected_naming_semantics() {
    for (name, version) in provider_versioning_schemes() {
        let version_parser = VersionParser;
        let norm_version =
            legacy_lossy(&version_parser.normalize(version, None).unwrap()).into_owned();
        let package = Package::new(
            bstr::BString::from(name.to_string()),
            bstr::BString::from(norm_version),
            bstr::BString::from(version.to_string()),
        );
        assert_eq!(name.to_ascii_lowercase(), package.get_name());
    }
}

#[test]
fn test_package_has_expected_versioning_semantics() {
    for (name, version) in provider_versioning_schemes() {
        let version_parser = VersionParser;
        let norm_version =
            legacy_lossy(&version_parser.normalize(version, None).unwrap()).into_owned();
        let package = Package::new(
            bstr::BString::from(name.to_string()),
            bstr::BString::from(norm_version.clone()),
            bstr::BString::from(version.to_string()),
        );
        assert_eq!(version, package.get_pretty_version());
        assert_eq!(norm_version, package.get_version());
    }
}

#[test]
fn test_package_has_expected_marshalling_semantics() {
    for (name, version) in provider_versioning_schemes() {
        let version_parser = VersionParser;
        let norm_version =
            legacy_lossy(&version_parser.normalize(version, None).unwrap()).into_owned();
        let package = Package::new(
            bstr::BString::from(name.to_string()),
            bstr::BString::from(norm_version.clone()),
            bstr::BString::from(version.to_string()),
        );
        assert_eq!(
            format!("{}-{}", name.to_ascii_lowercase(), norm_version),
            package.to_php_string()
        );
    }
}

#[test]
fn test_get_target_dir() {
    let mut package = Package::new(
        bstr::BString::from("a".to_string()),
        bstr::BString::from("1.0.0.0".to_string()),
        bstr::BString::from("1.0".to_string()),
    );

    assert_eq!(None, package.get_target_dir());

    package.set_target_dir(Some(bstr::BString::from("./../foo/".to_string())));
    assert_eq!(
        Some("foo/".to_string()),
        package
            .get_target_dir()
            .map(|s| legacy_lossy(&s).into_owned())
    );

    package.set_target_dir(Some(bstr::BString::from("foo/../../../bar/".to_string())));
    assert_eq!(
        Some("foo/bar/".to_string()),
        package
            .get_target_dir()
            .map(|s| legacy_lossy(&s).into_owned())
    );

    package.set_target_dir(Some(bstr::BString::from("../..".to_string())));
    assert_eq!(
        Some(String::new()),
        package
            .get_target_dir()
            .map(|s| legacy_lossy(&s).into_owned())
    );

    package.set_target_dir(Some(bstr::BString::from("..".to_string())));
    assert_eq!(
        Some(String::new()),
        package
            .get_target_dir()
            .map(|s| legacy_lossy(&s).into_owned())
    );

    package.set_target_dir(Some(bstr::BString::from("/..".to_string())));
    assert_eq!(
        Some(String::new()),
        package
            .get_target_dir()
            .map(|s| legacy_lossy(&s).into_owned())
    );

    package.set_target_dir(Some(bstr::BString::from("/foo/..".to_string())));
    assert_eq!(
        Some("foo/".to_string()),
        package
            .get_target_dir()
            .map(|s| legacy_lossy(&s).into_owned())
    );

    package.set_target_dir(Some(bstr::BString::from("/foo/..//bar".to_string())));
    assert_eq!(
        Some("foo/bar".to_string()),
        package
            .get_target_dir()
            .map(|s| legacy_lossy(&s).into_owned())
    );
}
