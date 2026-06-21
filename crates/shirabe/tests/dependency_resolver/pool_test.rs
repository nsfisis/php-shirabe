//! ref: composer/tests/Composer/Test/DependencyResolver/PoolTest.php

use indexmap::IndexMap;
use shirabe::dependency_resolver::pool::Pool;
use shirabe::package::handle::PackageInterfaceHandle;

use crate::test_case::{get_package, get_version_constraint};

// ref: PoolTest::createPool
fn create_pool(packages: Vec<PackageInterfaceHandle>) -> Pool {
    Pool::new(
        packages,
        vec![],
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
    )
}

fn same_packages(a: &[PackageInterfaceHandle], b: &[PackageInterfaceHandle]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.ptr_eq(y))
}

#[test]
fn test_pool() {
    let package = get_package("foo", "1");

    let mut pool = create_pool(vec![package.clone()]);

    assert!(same_packages(
        &[package.clone()],
        &pool.what_provides("foo", None)
    ));
    assert!(same_packages(&[package], &pool.what_provides("foo", None)));
}

#[test]
fn test_what_provides_package_with_constraint() {
    let first_package = get_package("foo", "1");
    let second_package = get_package("foo", "2");

    let mut pool = create_pool(vec![first_package.clone(), second_package.clone()]);

    assert!(same_packages(
        &[first_package, second_package.clone()],
        &pool.what_provides("foo", None)
    ));
    assert!(same_packages(
        &[second_package],
        &pool.what_provides("foo", Some(&get_version_constraint("==", "2")))
    ));
}

#[test]
fn test_package_by_id() {
    let package = get_package("foo", "1");

    let pool = create_pool(vec![package.clone()]);

    assert!(package.ptr_eq(&pool.package_by_id(1)));
}

#[test]
fn test_what_provides_when_package_cannot_be_found() {
    let mut pool = create_pool(vec![]);

    assert!(pool.what_provides("foo", None).is_empty());
}
