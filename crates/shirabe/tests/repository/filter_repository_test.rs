//! ref: composer/tests/Composer/Test/Repository/FilterRepositoryTest.php

use indexmap::IndexMap;
use shirabe::package::base_package::STABILITIES;
use shirabe::repository::{
    AdvisoryProviderInterface, ArrayRepository, FilterRepository, RepositoryInterface,
    RepositoryInterfaceHandle,
};
use shirabe_semver::constraint::{AnyConstraint, MatchAllConstraint};

use crate::test_case::get_package;

/// ref: FilterRepositoryTest::setUp
fn array_repo() -> RepositoryInterfaceHandle {
    let repo = ArrayRepository::new(vec![
        get_package("foo/aaa", "1.0.0"),
        get_package("foo/bbb", "1.0.0"),
        get_package("bar/xxx", "1.0.0"),
        get_package("baz/yyy", "1.0.0"),
    ])
    .unwrap();
    RepositoryInterfaceHandle::new(repo)
}

fn config(only: Option<&[&str]>, exclude: Option<&[&str]>) -> IndexMap<String, shirabe_php_shim::PhpMixed> {
    use shirabe_php_shim::PhpMixed;
    let mut c: IndexMap<String, PhpMixed> = IndexMap::new();
    if let Some(only) = only {
        c.insert(
            "only".to_string(),
            PhpMixed::List(only.iter().map(|s| PhpMixed::String(s.to_string())).collect()),
        );
    }
    if let Some(exclude) = exclude {
        c.insert(
            "exclude".to_string(),
            PhpMixed::List(exclude.iter().map(|s| PhpMixed::String(s.to_string())).collect()),
        );
    }
    c
}

fn match_all() -> AnyConstraint {
    MatchAllConstraint::new(None).into()
}

fn stabilities() -> IndexMap<String, i64> {
    STABILITIES.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

#[test]
fn test_repo_matching() {
    let all = vec!["foo/aaa", "foo/bbb", "bar/xxx", "baz/yyy"];
    let cases: Vec<(Vec<&str>, IndexMap<String, shirabe_php_shim::PhpMixed>)> = vec![
        (vec!["foo/aaa", "foo/bbb"], config(Some(&["foo/*"]), None)),
        (
            vec!["foo/aaa", "baz/yyy"],
            config(Some(&["foo/aaa", "baz/yyy"]), None),
        ),
        (vec!["bar/xxx"], config(None, Some(&["foo/*", "baz/yyy"]))),
        // make sure sub-patterns are not matched without wildcard
        (all.clone(), config(None, Some(&["foo/aa", "az/yyy"]))),
        (vec![], config(Some(&["foo/aa", "az/yyy"]), None)),
        // empty "only" means no packages allowed
        (vec![], config(Some(&[]), None)),
        // absent "only" means all packages allowed
        (all.clone(), config(None, None)),
        // empty or absent "exclude" have the same effect: none
        (all.clone(), config(None, Some(&[]))),
        (all.clone(), config(None, None)),
    ];

    for (expected, cfg) in cases {
        let mut repo = FilterRepository::new(array_repo(), cfg).unwrap();
        let packages = repo.get_packages().unwrap();
        let names: Vec<String> = packages.iter().map(|p| p.get_name()).collect();

        let expected: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        assert_eq!(expected, names);
    }
}

#[test]
fn test_both_filters_disallowed() {
    assert!(FilterRepository::new(array_repo(), config(Some(&[]), Some(&[]))).is_err());
}

#[test]
fn test_security_advisories_disabled_in_child() {
    let mut repo = FilterRepository::new(array_repo(), config(Some(&["foo/*"]), None)).unwrap();

    assert!(!repo.has_security_advisories().unwrap());

    let mut map: IndexMap<String, AnyConstraint> = IndexMap::new();
    map.insert("foo/aaa".to_string(), match_all());
    let result = repo.get_security_advisories(map, true).unwrap();

    assert!(result.names_found.is_empty());
    assert!(result.advisories.is_empty());
}

#[test]
fn test_canonical_default_true() {
    let mut repo = FilterRepository::new(array_repo(), config(None, None)).unwrap();

    let mut map: IndexMap<String, Option<AnyConstraint>> = IndexMap::new();
    map.insert("foo/aaa".to_string(), Some(match_all()));
    let result = repo
        .load_packages(map, stabilities(), IndexMap::new(), IndexMap::new())
        .unwrap();

    assert_eq!(1, result.packages.len());
    assert_eq!(1, result.names_found.len());
}

#[test]
fn test_non_canonical() {
    use shirabe_php_shim::PhpMixed;
    let mut cfg: IndexMap<String, PhpMixed> = IndexMap::new();
    cfg.insert("canonical".to_string(), PhpMixed::Bool(false));
    let mut repo = FilterRepository::new(array_repo(), cfg).unwrap();

    let mut map: IndexMap<String, Option<AnyConstraint>> = IndexMap::new();
    map.insert("foo/aaa".to_string(), Some(match_all()));
    let result = repo
        .load_packages(map, stabilities(), IndexMap::new(), IndexMap::new())
        .unwrap();

    assert_eq!(1, result.packages.len());
    assert_eq!(0, result.names_found.len());
}
