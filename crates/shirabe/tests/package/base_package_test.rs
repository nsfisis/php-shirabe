//! ref: composer/tests/Composer/Test/Package/BasePackageTest.php sha1:1e16b5fbd5f7d20eec3ea3324574a57d6b57bef4

use crate::test_case::get_package;
use bstr::{BStr, BString};
use shirabe::package::{DisplayMode, PackageHandle, package_names_to_regexp};
use shirabe::repository::{ArrayRepository, RepositoryInterfaceHandle};
use shirabe_php_shim::bformat;
use shirabe_semver::VersionParser;

fn empty_repository() -> RepositoryInterfaceHandle {
    RepositoryInterfaceHandle::new(ArrayRepository::new(vec![]).unwrap())
}

#[test]
fn test_set_same_repository() {
    let package = get_package("foo", "1.0.0");
    let repository = empty_repository();

    package.set_repository(repository.clone()).unwrap();
    // Set against the same repository is allowed.
    package.set_repository(repository.clone()).unwrap();
}

#[test]
fn test_set_another_repository() {
    let package = get_package("foo", "1.0.0");
    let repository1 = empty_repository();
    let repository2 = empty_repository();

    // The package stores the repository as a weak ref, so keep both strong handles
    // alive for the "already in another repository" check to fire.
    package.set_repository(repository1.clone()).unwrap();
    assert!(package.set_repository(repository2.clone()).is_err());
}

// PHP mocks isDev()/getSourceType()/getPrettyVersion()/getSourceReference() on an abstract
// BasePackage. A real Package reproduces the same observable getters: a "dev-master" version makes
// isDev() true, while the pretty version is an independent constructor argument.
#[test]
fn test_format_version_for_dev_package() {
    let cases: Vec<(&BStr, bool, &BStr)> = vec![
        (
            BStr::new("v2.1.0-RC2"),
            true,
            BStr::new("PrettyVersion v2.1.0-RC2"),
        ),
        (
            BStr::new("bbf527a27356414bfa9bf520f018c5cb7af67c77"),
            true,
            BStr::new("PrettyVersion bbf527a"),
        ),
        (
            BStr::new("v1.0.0"),
            false,
            BStr::new("PrettyVersion v1.0.0"),
        ),
        (
            BStr::new("bbf527a27356414bfa9bf520f018c5cb7af67c77"),
            false,
            BStr::new("PrettyVersion bbf527a27356414bfa9bf520f018c5cb7af67c77"),
        ),
    ];

    for (source_reference, truncate, expected) in cases {
        let package = PackageHandle::new(
            BString::from("dummy/pkg"),
            bstr::BString::from(&VersionParser.normalize("dev-master", None).unwrap()[..]),
            BString::from("PrettyVersion"),
        );
        package.__set_source_type(Some(BString::from("git")));
        package.set_source_reference(Some(bformat!("{source_reference}")));

        assert_eq!(
            expected,
            package.get_full_pretty_version(truncate, DisplayMode::SourceRefIfDev)
        );
    }
}

#[test]
fn test_package_names_to_regexp() {
    // The PHP data provider yields a single row whose extra elements are ignored,
    // so only the first (packageNames, wrap, expected) triple is exercised.
    let regexp = package_names_to_regexp(
        &[BString::from("ext-*"), BString::from("monolog/monolog")],
        "{^%s$}i",
    );

    assert_eq!("{^ext\\-.*|monolog/monolog$}i", regexp);
}
