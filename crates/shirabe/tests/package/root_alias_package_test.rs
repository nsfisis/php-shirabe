//! ref: composer/tests/Composer/Test/Package/RootAliasPackageTest.php sha1:fd66027c1f11794c1380df060c71848bcd49de34

use bstr::BString;
use indexmap::IndexMap;
use shirabe::package::{Link, RootAliasPackageHandle, RootPackageHandle};
use shirabe_semver::constraint::MatchAllConstraint;

fn links(link_type: &bstr::BStr) -> IndexMap<BString, Link> {
    let mut map: IndexMap<BString, Link> = IndexMap::new();
    map.insert(
        BString::from("b"),
        Link::new(
            BString::from("a"),
            BString::from("b"),
            MatchAllConstraint::new(None).into(),
            Some(link_type.to_owned()),
            Some(BString::from("self.version")),
        ),
    );
    map
}

fn alias() -> RootAliasPackageHandle {
    let root = RootPackageHandle::new(
        BString::from("something/something"),
        BString::from("1.0.0.0"),
        BString::from("1.0"),
    );
    RootAliasPackageHandle::new(root, BString::from("1.0"), BString::from("1.0.0.0")).unwrap()
}

#[test]
fn test_update_requires() {
    let alias = alias();
    assert!(alias.get_requires().is_empty());
    alias
        .set_requires(
            links(Link::TYPE_REQUIRE)
                .into_iter()
                .collect::<indexmap::IndexMap<_, _>>(),
        )
        .unwrap();
    assert!(!alias.get_requires().is_empty());
}

#[test]
fn test_update_dev_requires() {
    let alias = alias();
    assert!(alias.get_dev_requires().is_empty());
    alias
        .set_dev_requires(
            links(Link::TYPE_DEV_REQUIRE)
                .into_iter()
                .collect::<indexmap::IndexMap<_, _>>(),
        )
        .unwrap();
    assert!(!alias.get_dev_requires().is_empty());
}

#[test]
fn test_update_conflicts() {
    let alias = alias();
    assert!(alias.get_conflicts().is_empty());
    alias
        .set_conflicts(
            links(Link::TYPE_CONFLICT)
                .into_iter()
                .collect::<indexmap::IndexMap<_, _>>(),
        )
        .unwrap();
    assert!(!alias.get_conflicts().is_empty());
}

#[test]
fn test_update_provides() {
    let alias = alias();
    assert!(alias.get_provides().is_empty());
    alias
        .set_provides(
            links(Link::TYPE_PROVIDE)
                .into_iter()
                .collect::<indexmap::IndexMap<_, _>>(),
        )
        .unwrap();
    assert!(!alias.get_provides().is_empty());
}

#[test]
fn test_update_replaces() {
    let alias = alias();
    assert!(alias.get_replaces().is_empty());
    alias
        .set_replaces(
            links(Link::TYPE_REPLACE)
                .into_iter()
                .collect::<indexmap::IndexMap<_, _>>(),
        )
        .unwrap();
    assert!(!alias.get_replaces().is_empty());
}
