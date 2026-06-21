//! ref: composer/tests/Composer/Test/Package/BasePackageTest.php

use shirabe::package::base_package::package_names_to_regexp;
use shirabe::repository::{ArrayRepository, RepositoryInterfaceHandle};

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

// In PHP this mocks isDev()/getSourceType()/getPrettyVersion()/getSourceReference()
// on an abstract BasePackage to drive getFullPrettyVersion(). Reproducing those exact
// getter values requires mocking; a real package cannot carry the pretty version
// "PrettyVersion" together with isDev() == true.
#[ignore = "requires mocking isDev/getSourceType/getPrettyVersion/getSourceReference on an abstract BasePackage; no mock infrastructure exists and a real package cannot carry prettyVersion \"PrettyVersion\" with isDev() == true"]
#[test]
fn test_format_version_for_dev_package() {
    todo!()
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
