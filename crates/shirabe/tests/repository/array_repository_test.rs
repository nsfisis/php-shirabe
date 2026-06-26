//! ref: composer/tests/Composer/Test/Repository/ArrayRepositoryTest.php

use indexmap::IndexMap;
use shirabe::package::handle::PackageInterfaceHandle;
use shirabe::package::loader::array_loader::ArrayLoader;
use shirabe::repository::{
    AbandonedInfo, ArrayRepository, RepositoryInterface, SEARCH_FULLTEXT, SearchResult,
};
use shirabe_php_shim::PhpMixed;

use crate::test_case::{get_alias_package, get_package};

/// PHP `setType`/`setAbandoned` operate on non-root packages; the public handle API only allows
/// such setters on root packages, so packages carrying extra config are built via ArrayLoader.
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

#[derive(Debug, PartialEq)]
enum Abandoned {
    No,
    Yes,
    Replacement(String),
}

fn reprs(results: &[SearchResult]) -> Vec<(String, Option<String>, Abandoned)> {
    results
        .iter()
        .map(|r| {
            let abandoned = match &r.abandoned {
                None => Abandoned::No,
                Some(AbandonedInfo::Abandoned) => Abandoned::Yes,
                Some(AbandonedInfo::Replacement(s)) => Abandoned::Replacement(s.clone()),
            };
            (r.name.clone(), r.description.clone(), abandoned)
        })
        .collect()
}

#[test]
fn test_add_package() {
    let repo = ArrayRepository::new(vec![]).unwrap();
    repo.add_package(get_package("foo", "1")).unwrap();

    assert_eq!(1, repo.count().unwrap());
}

#[test]
fn test_remove_package() {
    let package = get_package("bar", "2");

    let mut repo = ArrayRepository::new(vec![]).unwrap();
    repo.add_package(get_package("foo", "1")).unwrap();
    repo.add_package(package.clone()).unwrap();

    assert_eq!(2, repo.count().unwrap());

    repo.remove_package(get_package("foo", "1"));

    assert_eq!(1, repo.count().unwrap());
    let packages = repo.get_packages().unwrap();
    assert_eq!(1, packages.len());
    assert!(packages[0].ptr_eq(&package));
}

#[test]
fn test_has_package() {
    let repo = ArrayRepository::new(vec![]).unwrap();
    repo.add_package(get_package("foo", "1")).unwrap();
    repo.add_package(get_package("bar", "2")).unwrap();

    assert!(repo.has_package(get_package("foo", "1")));
    assert!(!repo.has_package(get_package("bar", "1")));
}

#[test]
fn test_find_packages() {
    let mut repo = ArrayRepository::new(vec![]).unwrap();
    repo.add_package(get_package("foo", "1")).unwrap();
    repo.add_package(get_package("bar", "2")).unwrap();
    repo.add_package(get_package("bar", "3")).unwrap();

    let foo = repo.find_packages("foo", None).unwrap();
    assert_eq!(1, foo.len());
    assert_eq!(foo[0].get_name(), "foo");

    let bar = repo.find_packages("bar", None).unwrap();
    assert_eq!(2, bar.len());
    assert_eq!(bar[0].get_name(), "bar");
}

#[test]
#[ignore]
fn test_automatically_add_aliased_package_but_not_remove() {
    let repo = ArrayRepository::new(vec![]).unwrap();

    let package = get_package("foo", "1");
    let alias = get_alias_package(&package, "2");

    repo.add_package(alias.clone()).unwrap();

    assert_eq!(2, repo.count().unwrap());
    assert!(repo.has_package(get_package("foo", "1")));
    assert!(repo.has_package(get_package("foo", "2")));

    repo.remove_package(alias);

    assert_eq!(1, repo.count().unwrap());
}

#[test]
fn test_search() {
    let mut repo = ArrayRepository::new(vec![]).unwrap();

    repo.add_package(get_package("foo", "1")).unwrap();
    repo.add_package(get_package("bar", "1")).unwrap();

    assert_eq!(
        vec![("foo".to_string(), None, Abandoned::No)],
        reprs(
            &repo
                .search("foo".to_string(), SEARCH_FULLTEXT, None)
                .unwrap()
        )
    );

    assert_eq!(
        vec![("bar".to_string(), None, Abandoned::No)],
        reprs(
            &repo
                .search("bar".to_string(), SEARCH_FULLTEXT, None)
                .unwrap()
        )
    );

    assert!(
        repo.search("foobar".to_string(), SEARCH_FULLTEXT, None)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn test_search_with_package_type() {
    let mut repo = ArrayRepository::new(vec![]).unwrap();

    repo.add_package(get_package("foo", "1")).unwrap();
    repo.add_package(get_package("bar", "1")).unwrap();

    let package = loaded(
        "foobar",
        "1",
        vec![("type", PhpMixed::String("composer-plugin".to_string()))],
    );
    repo.add_package(package).unwrap();

    assert_eq!(
        vec![("foo".to_string(), None, Abandoned::No)],
        reprs(
            &repo
                .search(
                    "foo".to_string(),
                    SEARCH_FULLTEXT,
                    Some("library".to_string())
                )
                .unwrap()
        )
    );

    assert!(
        repo.search(
            "bar".to_string(),
            SEARCH_FULLTEXT,
            Some("package".to_string())
        )
        .unwrap()
        .is_empty()
    );

    assert_eq!(
        vec![("foobar".to_string(), None, Abandoned::No)],
        reprs(
            &repo
                .search("foo".to_string(), 0, Some("composer-plugin".to_string()))
                .unwrap()
        )
    );
}

#[test]
fn test_search_with_abandoned_packages() {
    let mut repo = ArrayRepository::new(vec![]).unwrap();

    let package1 = loaded("foo1", "1", vec![("abandoned", PhpMixed::Bool(true))]);
    repo.add_package(package1).unwrap();
    let package2 = loaded(
        "foo2",
        "1",
        vec![("abandoned", PhpMixed::String("bar".to_string()))],
    );
    repo.add_package(package2).unwrap();

    assert_eq!(
        vec![
            ("foo1".to_string(), None, Abandoned::Yes),
            (
                "foo2".to_string(),
                None,
                Abandoned::Replacement("bar".to_string())
            ),
        ],
        reprs(
            &repo
                .search("foo".to_string(), SEARCH_FULLTEXT, None)
                .unwrap()
        )
    );
}
