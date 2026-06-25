//! ref: composer/tests/Composer/Test/Package/RootAliasPackageTest.php

use indexmap::IndexMap;
use shirabe::package::{Link, RootAliasPackageHandle, RootPackageHandle};
use shirabe_semver::constraint::MatchAllConstraint;

fn links(link_type: &str) -> IndexMap<String, Link> {
    let mut map: IndexMap<String, Link> = IndexMap::new();
    map.insert(
        "b".to_string(),
        Link::new(
            "a".to_string(),
            "b".to_string(),
            MatchAllConstraint::new(None).into(),
            Some(link_type.to_string()),
            "self.version".to_string(),
        ),
    );
    map
}

fn alias() -> RootAliasPackageHandle {
    let root = RootPackageHandle::new(
        "something/something".to_string(),
        "1.0.0.0".to_string(),
        "1.0".to_string(),
    );
    RootAliasPackageHandle::new(root, "1.0".to_string(), "1.0.0.0".to_string())
}

#[test]
fn test_update_requires() {
    let alias = alias();
    assert!(alias.get_requires().is_empty());
    alias.set_requires(links(Link::TYPE_REQUIRE));
    assert!(!alias.get_requires().is_empty());
}

#[test]
fn test_update_dev_requires() {
    let alias = alias();
    assert!(alias.get_dev_requires().is_empty());
    alias.set_dev_requires(links(Link::TYPE_DEV_REQUIRE));
    assert!(!alias.get_dev_requires().is_empty());
}

#[test]
fn test_update_conflicts() {
    let alias = alias();
    assert!(alias.get_conflicts().is_empty());
    alias.set_conflicts(links(Link::TYPE_CONFLICT));
    assert!(!alias.get_conflicts().is_empty());
}

#[test]
fn test_update_provides() {
    let alias = alias();
    assert!(alias.get_provides().is_empty());
    alias.set_provides(links(Link::TYPE_PROVIDE));
    assert!(!alias.get_provides().is_empty());
}

#[test]
fn test_update_replaces() {
    let alias = alias();
    assert!(alias.get_replaces().is_empty());
    alias.set_replaces(links(Link::TYPE_REPLACE));
    assert!(!alias.get_replaces().is_empty());
}
