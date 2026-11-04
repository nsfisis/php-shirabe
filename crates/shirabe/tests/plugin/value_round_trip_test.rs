//! Shirabe-specific integration tests for the immutable values that cross the plugin boundary
//! as real PHP objects: `Composer\Package\Link` with its `composer/semver` constraint, and the
//! `\DateTimeInterface` release date. Upstream Composer has no test for this (its plugins run
//! in-process), so the fixtures `fixtures/values-v1` and `fixtures/link-pretty-constraint-v1`
//! are Shirabe-owned.

use crate::async_runtime::run;
use crate::php_worker::{lock_php_worker, php_runtime_available};
use crate::plugin_installer_test::{new_installer, set_up};
use bstr::{B, BStr, BString};
use indexmap::IndexMap;
use shirabe::installer::InstallerInterface;
use shirabe::package::loader::{ArrayLoader, JsonLoader, JsonLoaderInput};
use shirabe::package::{Link, PackageInterfaceHandle};
use shirabe_php_shim::{
    Catch, UnexpectedValueException, bconcat, display_to_bytes, implode, path_to_bytes,
};
use shirabe_semver::constraint::{
    AnyConstraint, MatchAllConstraint, MatchNoneConstraint, MultiConstraint, SimpleConstraint,
};

fn values_fixture_package() -> PackageInterfaceHandle {
    let loader = JsonLoader::new(Box::new(ArrayLoader::new(None, false)));
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/plugin/fixtures/values-v1/composer.json");
    loader
        .load(JsonLoaderInput::String(
            path_to_bytes(&path.canonicalize().unwrap()).into(),
        ))
        .unwrap()
}

fn simple(operator: &BStr, version: &BStr, pretty_string: Option<&BStr>) -> AnyConstraint {
    AnyConstraint::Simple(SimpleConstraint::new(
        operator,
        bconcat!(version),
        pretty_string.map(|s| s.to_owned()),
    ))
}

fn link(target: &BStr, constraint: AnyConstraint, pretty_constraint: &BStr) -> Link {
    Link::new(
        BString::from("dummy/root"),
        bconcat!(target),
        constraint,
        Some(Link::TYPE_REQUIRE.to_owned()),
        Some(bconcat!(pretty_constraint)),
    )
}

/// Every constraint shape the codec knows, so the rendering the plugin writes covers all four
/// classes and both the set and the unset pretty string.
fn seeded_requires() -> IndexMap<BString, Link> {
    let mut requires = IndexMap::new();
    requires.insert(
        BString::from("foo/simple"),
        link(
            BStr::new("foo/simple"),
            simple(BStr::new(">="), BStr::new("1.0.0.0"), None),
            BStr::new(">=1.0"),
        ),
    );
    requires.insert(
        BString::from("foo/multi"),
        link(
            BStr::new("foo/multi"),
            AnyConstraint::Multi(MultiConstraint::new(
                vec![
                    simple(BStr::new(">="), BStr::new("1.0.0.0"), None),
                    simple(
                        BStr::new("<"),
                        BStr::new("2.0.0.0"),
                        Some(BStr::new("<2.0")),
                    ),
                ],
                true,
                Some(BString::from("^1.0")),
            )),
            BStr::new("^1.0"),
        ),
    );
    requires.insert(
        BString::from("foo/all"),
        link(
            BStr::new("foo/all"),
            AnyConstraint::MatchAll(MatchAllConstraint::new(None)),
            BStr::new("*"),
        ),
    );
    requires.insert(
        BString::from("foo/none"),
        link(
            BStr::new("foo/none"),
            AnyConstraint::MatchNone(MatchNoneConstraint::new(Some(BString::from("nothing")))),
            BStr::new(""),
        ),
    );
    requires
}

fn describe(constraint: &AnyConstraint) -> BString {
    let shape = match constraint {
        AnyConstraint::Simple(c) => {
            bconcat!(
                "Constraint(",
                display_to_bytes(c.get_operator()),
                " ",
                c.get_version(),
                ")"
            )
        }
        AnyConstraint::Multi(c) => bconcat!(
            "MultiConstraint(",
            if c.is_conjunctive() { "and" } else { "or" },
            ": ",
            implode(
                ", ",
                &c.get_constraints().iter().map(describe).collect::<Vec<_>>()
            ),
            ")"
        ),
        AnyConstraint::MatchAll(_) => BString::from("MatchAllConstraint()"),
        AnyConstraint::MatchNone(_) => BString::from("MatchNoneConstraint()"),
    };
    bconcat!(shape, " pretty=", constraint.get_pretty_string())
}

fn describe_links(links: &IndexMap<BString, Link>) -> Vec<BString> {
    links
        .iter()
        .map(|(name, link)| {
            bconcat!(
                name,
                " | ",
                link.get_source(),
                " | ",
                link.get_target(),
                " | ",
                link.get_description(),
                " | ",
                link.get_pretty_constraint().unwrap(),
                " | ",
                describe(link.get_constraint())
            )
        })
        .collect()
}

#[test]
fn test_links_and_release_date_round_trip_through_the_plugin() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    let set_up = set_up();
    let package = set_up.composer.borrow().get_package().clone();
    package
        .set_requires(
            seeded_requires()
                .into_iter()
                .collect::<indexmap::IndexMap<_, _>>(),
        )
        .unwrap();

    let installer = new_installer(&set_up);
    set_up.pm.borrow_mut().load_installed_plugins().unwrap();
    run(installer.install(&set_up.repository, values_fixture_package())).unwrap();

    // What the plugin saw: each seeded link rebuilt in the child as a real Link over a real
    // composer/semver constraint.
    assert_eq!(
        "foo/simple | dummy/root | foo/simple | requires | >=1.0 | Constraint(>= 1.0.0.0) pretty=>= 1.0.0.0\n\
         foo/multi | dummy/root | foo/multi | requires | ^1.0 | MultiConstraint(and: Constraint(>= 1.0.0.0) pretty=>= 1.0.0.0, Constraint(< 2.0.0.0) pretty=<2.0) pretty=^1.0\n\
         foo/all | dummy/root | foo/all | requires | * | MatchAllConstraint() pretty=*\n\
         foo/none | dummy/root | foo/none | requires |  | MatchNoneConstraint() pretty=nothing\n\
         release date: 2024-03-03T20:06:07.123456+00:00\n",
        set_up.io.borrow().get_output()
    );

    // What the plugin wrote back: links it built PHP-side, decoded into Rust values.
    assert_eq!(
        vec![
            "bar/plain | dummy/root | bar/plain | requires | 2.0 | Constraint(== 2.0.0.0) pretty=2.0",
            "bar/multi | dummy/root | bar/multi | requires (for development) | <1.0 || >=3.0 | MultiConstraint(or: Constraint(< 1.0.0.0) pretty=< 1.0.0.0, Constraint(>= 3.0.0.0) pretty=>= 3.0.0.0) pretty=[< 1.0.0.0 || >= 3.0.0.0]",
            "bar/all | dummy/root | bar/all | requires | * | MatchAllConstraint() pretty=*",
            "bar/none | dummy/root | bar/none | requires |  | MatchNoneConstraint() pretty=nothing",
        ],
        describe_links(&package.get_requires())
    );
    // The plugin wrote an Asia/Tokyo instant with microseconds; both survive the crossing,
    // and the child sees the same instant back in UTC.
    assert_eq!(
        "2024-03-03T20:06:07.123456Z",
        package
            .get_release_date()
            .unwrap()
            .to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
    );
}

fn link_pretty_constraint_fixture_package() -> PackageInterfaceHandle {
    let loader = JsonLoader::new(Box::new(ArrayLoader::new(None, false)));
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/plugin/fixtures/link-pretty-constraint-v1/composer.json");
    loader
        .load(JsonLoaderInput::String(
            path_to_bytes(&path.canonicalize().unwrap()).into(),
        ))
        .unwrap()
}

/// A link built without a pretty constraint keeps it unset across the boundary in both
/// directions, so `getPrettyConstraint()` throws on either side as it does in PHP.
#[test]
fn test_link_without_pretty_constraint_throws_on_both_sides() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    let set_up = set_up();
    let package = set_up.composer.borrow().get_package().clone();
    let mut requires = IndexMap::new();
    requires.insert(
        BString::from("foo/set"),
        link(
            BStr::new("foo/set"),
            AnyConstraint::MatchAll(MatchAllConstraint::new(None)),
            BStr::new("*"),
        ),
    );
    requires.insert(
        BString::from("foo/unset"),
        Link::new(
            BString::from("dummy/root"),
            BString::from("foo/unset"),
            AnyConstraint::MatchAll(MatchAllConstraint::new(None)),
            Some(Link::TYPE_REQUIRE.to_owned()),
            None,
        ),
    );
    package.set_requires(requires).unwrap();

    let installer = new_installer(&set_up);
    set_up.pm.borrow_mut().load_installed_plugins().unwrap();
    run(installer.install(&set_up.repository, link_pretty_constraint_fixture_package())).unwrap();

    assert_eq!(
        "foo/set: *\n\
         foo/unset: UnexpectedValueException: Link dummy/root requires foo/unset (*) has been misconfigured and had no prettyConstraint given.\n",
        set_up.io.borrow().get_output()
    );

    let requires = package.get_requires();
    let link = requires.get(B("bar/unset")).unwrap();
    assert_eq!(None, link.get_pretty_constraint_for_serialization());
    let error = link.get_pretty_constraint().unwrap_err();
    assert_eq!(
        "Link dummy/root relates to bar/unset (*) has been misconfigured and had no prettyConstraint given.",
        error
            .catch::<UnexpectedValueException>()
            .expect("an UnexpectedValueException")
            .get_message()
    );
}
