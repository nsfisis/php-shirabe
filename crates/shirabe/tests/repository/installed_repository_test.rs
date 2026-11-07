//! ref: composer/tests/Composer/Test/Repository/InstalledRepositoryTest.php sha1:7b13911070956c1ca555b582c265f0f335b604c6

use crate::test_case::get_package;
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe::package::PackageInterfaceHandle;
use shirabe::package::loader::ArrayLoader;
use shirabe::repository::{
    ArrayRepository, FindPackageConstraint, InstalledArrayRepository, InstalledRepository,
    RepositoryInterfaceHandle,
};
use shirabe_php_shim::{PhpMixed, bconcat};

/// PHP `setReplaces`/`setProvides` operate on non-root packages; the public handle API only allows
/// link setters on root packages, so packages carrying links are built via ArrayLoader.
fn loaded(name: &BStr, version: &BStr, extra: Vec<(&BStr, PhpMixed)>) -> PackageInterfaceHandle {
    let mut config: IndexMap<BString, PhpMixed> = IndexMap::new();
    config.insert("name".into(), PhpMixed::string(bconcat!(name)));
    config.insert("version".into(), PhpMixed::string(bconcat!(version)));
    for (key, value) in extra {
        config.insert(bconcat!(key), value);
    }

    ArrayLoader::new(None, false)
        .load_packages(vec![config])
        .unwrap()
        .remove(0)
}

fn provided_link() -> PhpMixed {
    let mut links: IndexMap<BString, PhpMixed> = IndexMap::new();
    links.insert("provided".into(), PhpMixed::string(BString::from("*")));
    PhpMixed::Array(links)
}

#[test]
fn test_find_packages_with_replacers_and_providers() {
    let foo = loaded(
        BStr::new("foo"),
        BStr::new("1"),
        vec![(BStr::new("replace"), provided_link())],
    );
    let foo2 = get_package("foo", "2");
    let array_repo_one =
        InstalledArrayRepository::new_with_packages(vec![foo.clone(), foo2.clone()]).unwrap();

    let bar = get_package("bar", "1");
    let bar2 = loaded(
        BStr::new("bar"),
        BStr::new("2"),
        vec![(BStr::new("provide"), provided_link())],
    );
    let array_repo_two =
        InstalledArrayRepository::new_with_packages(vec![bar.clone(), bar2.clone()]).unwrap();

    let repo = InstalledRepository::new(vec![
        RepositoryInterfaceHandle::new(array_repo_one),
        RepositoryInterfaceHandle::new(array_repo_two),
    ]);

    let foo_matches = repo
        .find_packages_with_replacers_and_providers(
            bstr::BStr::new("foo"),
            Some(FindPackageConstraint::String(BString::from("2"))),
        )
        .unwrap();
    assert_eq!(1, foo_matches.len());
    assert!(foo_matches[0].ptr_eq(&foo2));

    let bar_matches = repo
        .find_packages_with_replacers_and_providers(
            bstr::BStr::new("bar"),
            Some(FindPackageConstraint::String(BString::from("1"))),
        )
        .unwrap();
    assert_eq!(1, bar_matches.len());
    assert!(bar_matches[0].ptr_eq(&bar));

    let provided_matches = repo
        .find_packages_with_replacers_and_providers(bstr::BStr::new("provided"), None)
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
