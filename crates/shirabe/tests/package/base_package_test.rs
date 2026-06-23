//! ref: composer/tests/Composer/Test/Package/BasePackageTest.php

use shirabe::package::DisplayMode;
use shirabe::package::base_package::package_names_to_regexp;
use shirabe::package::handle::PackageHandle;
use shirabe::repository::{ArrayRepository, RepositoryInterfaceHandle};
use shirabe_semver::version_parser::VersionParser;

use crate::test_case::get_package;

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
    let cases: Vec<(&str, bool, &str)> = vec![
        ("v2.1.0-RC2", true, "PrettyVersion v2.1.0-RC2"),
        (
            "bbf527a27356414bfa9bf520f018c5cb7af67c77",
            true,
            "PrettyVersion bbf527a",
        ),
        ("v1.0.0", false, "PrettyVersion v1.0.0"),
        (
            "bbf527a27356414bfa9bf520f018c5cb7af67c77",
            false,
            "PrettyVersion bbf527a27356414bfa9bf520f018c5cb7af67c77",
        ),
    ];

    for (source_reference, truncate, expected) in cases {
        let package = PackageHandle::new(
            "dummy/pkg".to_string(),
            VersionParser.normalize("dev-master", None).unwrap(),
            "PrettyVersion".to_string(),
        );
        package.__set_source_type(Some("git".to_string()));
        package.set_source_reference(Some(source_reference.to_string()));

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
        &["ext-*".to_string(), "monolog/monolog".to_string()],
        "{^%s$}i",
    );

    assert_eq!("{^ext\\-.*|monolog/monolog$}i", regexp);
}
