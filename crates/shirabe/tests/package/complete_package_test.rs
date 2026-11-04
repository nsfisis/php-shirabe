//! ref: composer/tests/Composer/Test/Package/CompletePackageTest.php sha1:49d1cc8808cabebb0080c40aee4cb75be9179bfd

use bstr::{BStr, BString};
use shirabe::package::package::Package;
use shirabe::package::package_interface::PackageInterface;
use shirabe_php_shim::bconcat;
use shirabe_semver::VersionParser;

/// Memory package naming, versioning, and marshalling semantics provider
/// demonstrates several versioning schemes
fn provider_versioning_schemes() -> Vec<(&'static BStr, &'static BStr)> {
    vec![
        (BStr::new("foo"), BStr::new("1-beta")),
        (BStr::new("node"), BStr::new("0.5.6")),
        (BStr::new("li3"), BStr::new("0.10")),
        (BStr::new("mongodb_odm"), BStr::new("1.0.0BETA3")),
        (BStr::new("DoctrineCommon"), BStr::new("2.2.0-DEV")),
    ]
}

#[test]
fn test_package_has_expected_naming_semantics() {
    for (name, version) in provider_versioning_schemes() {
        let version_parser = VersionParser;
        let norm_version = version_parser.normalize(version, None).unwrap();
        let package = Package::new(bconcat!(name), norm_version, bconcat!(version));
        assert_eq!(name.to_ascii_lowercase(), package.get_name());
    }
}

#[test]
fn test_package_has_expected_versioning_semantics() {
    for (name, version) in provider_versioning_schemes() {
        let version_parser = VersionParser;
        let norm_version = version_parser.normalize(version, None).unwrap();
        let package = Package::new(bconcat!(name), norm_version.clone(), bconcat!(version));
        assert_eq!(version, package.get_pretty_version());
        assert_eq!(norm_version, package.get_version());
    }
}

#[test]
fn test_package_has_expected_marshalling_semantics() {
    for (name, version) in provider_versioning_schemes() {
        let version_parser = VersionParser;
        let norm_version = version_parser.normalize(version, None).unwrap();
        let package = Package::new(bconcat!(name), norm_version.clone(), bconcat!(version));
        assert_eq!(
            bconcat!(name.to_ascii_lowercase(), "-", norm_version),
            package.to_php_string()
        );
    }
}

#[test]
fn test_get_target_dir() {
    let mut package = Package::new(
        BString::from("a"),
        BString::from("1.0.0.0"),
        BString::from("1.0"),
    );

    assert_eq!(None, package.get_target_dir());

    package.set_target_dir(Some(BString::from("./../foo/")));
    assert_eq!(Some("foo/".into()), package.get_target_dir());

    package.set_target_dir(Some(BString::from("foo/../../../bar/")));
    assert_eq!(Some("foo/bar/".into()), package.get_target_dir());

    package.set_target_dir(Some(BString::from("../..")));
    assert_eq!(Some("".into()), package.get_target_dir());

    package.set_target_dir(Some(BString::from("..")));
    assert_eq!(Some("".into()), package.get_target_dir());

    package.set_target_dir(Some(BString::from("/..")));
    assert_eq!(Some("".into()), package.get_target_dir());

    package.set_target_dir(Some(BString::from("/foo/..")));
    assert_eq!(Some("foo/".into()), package.get_target_dir());

    package.set_target_dir(Some(BString::from("/foo/..//bar")));
    assert_eq!(Some("foo/bar".into()), package.get_target_dir());
}
