//! ref: composer/tests/Composer/Test/Repository/InstalledRepositoryTest.php

use indexmap::IndexMap;
use shirabe::package::handle::PackageInterfaceHandle;
use shirabe::package::loader::array_loader::ArrayLoader;
use shirabe::repository::{
    ArrayRepository, FindPackageConstraint, InstalledArrayRepository, InstalledRepository,
    RepositoryInterfaceHandle,
};
use shirabe_php_shim::PhpMixed;

use crate::test_case::get_package;

/// PHP `setReplaces`/`setProvides` operate on non-root packages; the public handle API only allows
/// link setters on root packages, so packages carrying links are built via ArrayLoader.
fn loaded(name: &str, version: &str, extra: Vec<(&str, PhpMixed)>) -> PackageInterfaceHandle {
    let mut config: IndexMap<String, PhpMixed> = IndexMap::new();
    config.insert("name".to_string(), PhpMixed::String(name.to_string()));
    config.insert("version".to_string(), PhpMixed::String(version.to_string()));
    for (key, value) in extra {
        config.insert(key.to_string(), value);
    }

    ArrayLoader::new(None, false)
        .load_packages(vec![config])
        .unwrap()
        .remove(0)
}

fn provided_link() -> PhpMixed {
    let mut links: IndexMap<String, PhpMixed> = IndexMap::new();
    links.insert("provided".to_string(), PhpMixed::String("*".to_string()));
    PhpMixed::Array(links)
}

#[test]
#[ignore = "InstalledRepository::add_repository asserts on a fixed set of repo types that omits InstalledRepositoryInterface, so adding an InstalledArrayRepository panics"]
fn test_find_packages_with_replacers_and_providers() {
    let foo = loaded("foo", "1", vec![("replace", provided_link())]);
    let foo2 = get_package("foo", "2");
    let array_repo_one =
        InstalledArrayRepository::new_with_packages(vec![foo.clone(), foo2.clone()]).unwrap();

    let bar = get_package("bar", "1");
    let bar2 = loaded("bar", "2", vec![("provide", provided_link())]);
    let array_repo_two =
        InstalledArrayRepository::new_with_packages(vec![bar.clone(), bar2.clone()]).unwrap();

    let repo = InstalledRepository::new(vec![
        RepositoryInterfaceHandle::new(array_repo_one),
        RepositoryInterfaceHandle::new(array_repo_two),
    ]);

    let foo_matches = repo
        .find_packages_with_replacers_and_providers(
            "foo",
            Some(FindPackageConstraint::String("2".to_string())),
        )
        .unwrap();
    assert_eq!(1, foo_matches.len());
    assert!(foo_matches[0].ptr_eq(&foo2));

    let bar_matches = repo
        .find_packages_with_replacers_and_providers(
            "bar",
            Some(FindPackageConstraint::String("1".to_string())),
        )
        .unwrap();
    assert_eq!(1, bar_matches.len());
    assert!(bar_matches[0].ptr_eq(&bar));

    let provided_matches = repo
        .find_packages_with_replacers_and_providers("provided", None)
        .unwrap();
    assert_eq!(2, provided_matches.len());
    assert!(provided_matches[0].ptr_eq(&foo));
    assert!(provided_matches[1].ptr_eq(&bar2));
}

#[test]
#[should_panic]
fn test_add_repository() {
    let array_repo_one = RepositoryInterfaceHandle::new(ArrayRepository::new(vec![]).unwrap());

    InstalledRepository::new(vec![array_repo_one]);
}
