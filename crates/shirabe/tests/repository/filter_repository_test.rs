//! ref: composer/tests/Composer/Test/Repository/FilterRepositoryTest.php sha1:d3394ae137863043a7a36d660dcb7a8a86a9f124

use crate::test_case::get_package;
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe::package::STABILITIES;
use shirabe::repository::{
    AdvisoryProviderInterface, ArrayRepository, FilterRepository, RepositoryInterface,
    RepositoryInterfaceHandle,
};
use shirabe_php_shim::{PhpMixed, ToPhpString};
use shirabe_semver::constraint::{AnyConstraint, MatchAllConstraint};

fn set_up() -> RepositoryInterfaceHandle {
    let repo = ArrayRepository::new(vec![
        get_package("foo/aaa", "1.0.0"),
        get_package("foo/bbb", "1.0.0"),
        get_package("bar/xxx", "1.0.0"),
        get_package("baz/yyy", "1.0.0"),
    ])
    .unwrap();
    RepositoryInterfaceHandle::new(repo)
}

fn config(only: Option<&[&BStr]>, exclude: Option<&[&BStr]>) -> IndexMap<BString, PhpMixed> {
    let mut c: IndexMap<BString, PhpMixed> = IndexMap::new();
    if let Some(only) = only {
        c.insert(
            "only".into(),
            PhpMixed::List(
                only.iter()
                    .map(|s| PhpMixed::string(s.to_php_string()))
                    .collect(),
            ),
        );
    }
    if let Some(exclude) = exclude {
        c.insert(
            "exclude".into(),
            PhpMixed::List(
                exclude
                    .iter()
                    .map(|s| PhpMixed::string(s.to_php_string()))
                    .collect(),
            ),
        );
    }
    c
}

fn match_all() -> AnyConstraint {
    MatchAllConstraint::new(None).into()
}

fn stabilities() -> IndexMap<bstr::BString, i64> {
    STABILITIES
        .iter()
        .map(|(k, v)| (bstr::BString::from(*k), *v))
        .collect()
}

#[test]
fn test_repo_matching() {
    let all = vec![
        BStr::new("foo/aaa"),
        BStr::new("foo/bbb"),
        BStr::new("bar/xxx"),
        BStr::new("baz/yyy"),
    ];
    let cases: Vec<(Vec<&BStr>, IndexMap<BString, PhpMixed>)> = vec![
        (
            vec![BStr::new("foo/aaa"), BStr::new("foo/bbb")],
            config(Some(&[BStr::new("foo/*")]), None),
        ),
        (
            vec![BStr::new("foo/aaa"), BStr::new("baz/yyy")],
            config(Some(&[BStr::new("foo/aaa"), BStr::new("baz/yyy")]), None),
        ),
        (
            vec![BStr::new("bar/xxx")],
            config(None, Some(&[BStr::new("foo/*"), BStr::new("baz/yyy")])),
        ),
        // make sure sub-patterns are not matched without wildcard
        (
            all.clone(),
            config(None, Some(&[BStr::new("foo/aa"), BStr::new("az/yyy")])),
        ),
        (
            vec![],
            config(Some(&[BStr::new("foo/aa"), BStr::new("az/yyy")]), None),
        ),
        // empty "only" means no packages allowed
        (vec![], config(Some(&[]), None)),
        // absent "only" means all packages allowed
        (all.clone(), config(None, None)),
        // empty or absent "exclude" have the same effect: none
        (all.clone(), config(None, Some(&[]))),
        (all.clone(), config(None, None)),
    ];

    for (expected, cfg) in cases {
        let mut repo = FilterRepository::new(set_up(), cfg).unwrap();
        let packages = repo.get_packages().unwrap();
        let names: Vec<bstr::BString> = packages
            .iter()
            .map(|p| bstr::BString::from(&p.get_name()[..]))
            .collect();

        let expected: Vec<BString> = expected.iter().map(|s| s.to_php_string()).collect();
        assert_eq!(expected, names);
    }
}

#[test]
fn test_both_filters_disallowed() {
    assert!(FilterRepository::new(set_up(), config(Some(&[]), Some(&[]))).is_err());
}

#[test]
fn test_security_advisories_disabled_in_child() {
    let mut repo =
        FilterRepository::new(set_up(), config(Some(&[BStr::new("foo/*")]), None)).unwrap();

    assert!(!repo.has_security_advisories().unwrap());

    let mut map: IndexMap<BString, AnyConstraint> = IndexMap::new();
    map.insert(BString::from("foo/aaa"), match_all());
    let result = repo
        .get_security_advisories(map.into_iter().collect::<indexmap::IndexMap<_, _>>(), true)
        .unwrap();

    assert!(result.names_found.is_empty());
    assert!(result.advisories.is_empty());
}

#[test]
fn test_canonical_default_true() {
    let mut repo = FilterRepository::new(set_up(), config(None, None)).unwrap();

    let mut map: IndexMap<BString, Option<AnyConstraint>> = IndexMap::new();
    map.insert(BString::from("foo/aaa"), Some(match_all()));
    let result = repo
        .load_packages(map, stabilities(), IndexMap::new(), IndexMap::new())
        .unwrap();

    assert_eq!(1, result.packages.len());
    assert_eq!(1, result.names_found.len());
}

#[test]
fn test_non_canonical() {
    let mut cfg: IndexMap<BString, PhpMixed> = IndexMap::new();
    cfg.insert("canonical".into(), PhpMixed::Bool(false));
    let mut repo = FilterRepository::new(set_up(), cfg).unwrap();

    let mut map: IndexMap<BString, Option<AnyConstraint>> = IndexMap::new();
    map.insert(BString::from("foo/aaa"), Some(match_all()));
    let result = repo
        .load_packages(map, stabilities(), IndexMap::new(), IndexMap::new())
        .unwrap();

    assert_eq!(1, result.packages.len());
    assert_eq!(0, result.names_found.len());
}
