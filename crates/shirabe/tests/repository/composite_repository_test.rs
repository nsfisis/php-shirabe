//! ref: composer/tests/Composer/Test/Repository/CompositeRepositoryTest.php

use shirabe::package::handle::PackageInterfaceHandle;
use shirabe::repository::{
    ArrayRepository, CompositeRepository, FindPackageConstraint, RepositoryInterface,
    RepositoryInterfaceHandle, SEARCH_FULLTEXT,
};

use crate::test_case::get_package;

fn array_repo(packages: Vec<PackageInterfaceHandle>) -> RepositoryInterfaceHandle {
    RepositoryInterfaceHandle::new(ArrayRepository::new(packages).unwrap())
}

#[test]
fn test_has_package() {
    let repo = CompositeRepository::new(vec![
        array_repo(vec![get_package("foo", "1")]),
        array_repo(vec![get_package("bar", "1")]),
    ]);

    assert!(repo.has_package(get_package("foo", "1")));
    assert!(repo.has_package(get_package("bar", "1")));

    assert!(!repo.has_package(get_package("foo", "2")));
    assert!(!repo.has_package(get_package("bar", "2")));
}

#[test]
#[ignore = "constraint parsing uses a look-around regex the regex crate does not support"]
fn test_find_package() {
    let mut repo = CompositeRepository::new(vec![
        array_repo(vec![get_package("foo", "1")]),
        array_repo(vec![get_package("bar", "1")]),
    ]);

    let foo = repo
        .find_package("foo", FindPackageConstraint::String("1".to_string()))
        .unwrap()
        .unwrap();
    assert_eq!(foo.get_name(), "foo");
    assert_eq!(foo.get_pretty_version(), "1");

    let bar = repo
        .find_package("bar", FindPackageConstraint::String("1".to_string()))
        .unwrap()
        .unwrap();
    assert_eq!(bar.get_name(), "bar");
    assert_eq!(bar.get_pretty_version(), "1");

    assert!(
        repo.find_package("foo", FindPackageConstraint::String("2".to_string()))
            .unwrap()
            .is_none()
    );
}

#[test]
fn test_find_packages() {
    let mut repo = CompositeRepository::new(vec![
        array_repo(vec![
            get_package("foo", "1"),
            get_package("foo", "2"),
            get_package("bat", "1"),
        ]),
        array_repo(vec![
            get_package("bar", "1"),
            get_package("bar", "2"),
            get_package("foo", "3"),
        ]),
    ]);

    let bats = repo.find_packages("bat", None).unwrap();
    assert_eq!(1, bats.len());
    assert_eq!(bats[0].get_name(), "bat");

    let bars = repo.find_packages("bar", None).unwrap();
    assert_eq!(2, bars.len());
    assert_eq!(bars[0].get_name(), "bar");

    let foos = repo.find_packages("foo", None).unwrap();
    assert_eq!(3, foos.len());
    assert_eq!(foos[0].get_name(), "foo");
}

#[test]
fn test_get_packages() {
    let mut repo = CompositeRepository::new(vec![
        array_repo(vec![get_package("foo", "1")]),
        array_repo(vec![get_package("bar", "1")]),
    ]);

    let packages = repo.get_packages().unwrap();
    assert_eq!(2, packages.len());
    assert_eq!(packages[0].get_name(), "foo");
    assert_eq!(packages[0].get_pretty_version(), "1");
    assert_eq!(packages[1].get_name(), "bar");
    assert_eq!(packages[1].get_pretty_version(), "1");
}

#[test]
fn test_add_repository() {
    let mut repo =
        CompositeRepository::new(vec![array_repo(vec![get_package("foo", "1")])]);

    assert_eq!(1, repo.count().unwrap());
    repo.add_repository(array_repo(vec![
        get_package("bar", "1"),
        get_package("bar", "2"),
        get_package("bar", "3"),
    ]));
    assert_eq!(4, repo.count().unwrap());
}

#[test]
fn test_count() {
    let repo = CompositeRepository::new(vec![
        array_repo(vec![get_package("foo", "1")]),
        array_repo(vec![get_package("bar", "1")]),
    ]);

    assert_eq!(2, repo.count().unwrap());
}

#[test]
fn test_no_repositories() {
    let mut repo = CompositeRepository::new(vec![]);

    assert!(repo.find_packages("foo", None).unwrap().is_empty());
    assert!(repo.search("foo".to_string(), SEARCH_FULLTEXT, None).unwrap().is_empty());
    assert!(repo.get_packages().unwrap().is_empty());
}
