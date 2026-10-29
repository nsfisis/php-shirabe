//! ref: composer/tests/Composer/Test/Package/RootAliasPackageTest.php

use indexmap::IndexMap;
use shirabe::package::{Link, RootAliasPackageHandle, RootPackageHandle};
use shirabe_semver::constraint::MatchAllConstraint;

fn links(link_type: &str) -> IndexMap<String, Link> {
    let mut map: IndexMap<String, Link> = IndexMap::new();
    map.insert(
        "b".to_string(),
        Link::new(
            bstr::BString::from("a".to_string()),
            bstr::BString::from("b".to_string()),
            MatchAllConstraint::new(None).into(),
            Some(bstr::BString::from(link_type.to_string())),
            bstr::BString::from("self.version".to_string()),
        ),
    );
    map
}

fn alias() -> RootAliasPackageHandle {
    let root = RootPackageHandle::new(
        bstr::BString::from("something/something".to_string()),
        bstr::BString::from("1.0.0.0".to_string()),
        bstr::BString::from("1.0".to_string()),
    );
    RootAliasPackageHandle::new(
        root,
        bstr::BString::from("1.0".to_string()),
        bstr::BString::from("1.0.0.0".to_string()),
    )
}

#[test]
fn test_update_requires() {
    let alias = alias();
    assert!(alias.get_requires().is_empty());
    alias.set_requires(
        links(Link::TYPE_REQUIRE)
            .into_iter()
            .map(|(k, v)| (bstr::BString::from(k), v))
            .collect::<indexmap::IndexMap<_, _>>(),
    );
    assert!(!alias.get_requires().is_empty());
}

#[test]
fn test_update_dev_requires() {
    let alias = alias();
    assert!(alias.get_dev_requires().is_empty());
    alias.set_dev_requires(
        links(Link::TYPE_DEV_REQUIRE)
            .into_iter()
            .map(|(k, v)| (bstr::BString::from(k), v))
            .collect::<indexmap::IndexMap<_, _>>(),
    );
    assert!(!alias.get_dev_requires().is_empty());
}

#[test]
fn test_update_conflicts() {
    let alias = alias();
    assert!(alias.get_conflicts().is_empty());
    alias.set_conflicts(
        links(Link::TYPE_CONFLICT)
            .into_iter()
            .map(|(k, v)| (bstr::BString::from(k), v))
            .collect::<indexmap::IndexMap<_, _>>(),
    );
    assert!(!alias.get_conflicts().is_empty());
}

#[test]
fn test_update_provides() {
    let alias = alias();
    assert!(alias.get_provides().is_empty());
    alias.set_provides(
        links(Link::TYPE_PROVIDE)
            .into_iter()
            .map(|(k, v)| (bstr::BString::from(k), v))
            .collect::<indexmap::IndexMap<_, _>>(),
    );
    assert!(!alias.get_provides().is_empty());
}

#[test]
fn test_update_replaces() {
    let alias = alias();
    assert!(alias.get_replaces().is_empty());
    alias.set_replaces(
        links(Link::TYPE_REPLACE)
            .into_iter()
            .map(|(k, v)| (bstr::BString::from(k), v))
            .collect::<indexmap::IndexMap<_, _>>(),
    );
    assert!(!alias.get_replaces().is_empty());
}
