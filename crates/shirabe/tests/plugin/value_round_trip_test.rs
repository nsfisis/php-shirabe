//! Shirabe-specific integration tests for the immutable values that cross the plugin boundary
//! as real PHP objects: `Composer\Package\Link` with its `composer/semver` constraint, and the
//! `\DateTimeInterface` release date. Upstream Composer has no test for this (its plugins run
//! in-process), so the fixture `fixtures/values-v1` is Shirabe-owned.

use crate::async_runtime::run;
use crate::php_worker::{lock_php_worker, php_runtime_available};
use crate::plugin_installer_test::{new_installer, set_up};
use indexmap::IndexMap;
use shirabe::installer::InstallerInterface;
use shirabe::package::Link;
use shirabe::package::PackageInterfaceHandle;
use shirabe::package::loader::{ArrayLoader, JsonLoader, JsonLoaderInput};
use shirabe_semver::constraint::{
    AnyConstraint, MatchAllConstraint, MatchNoneConstraint, MultiConstraint, SimpleConstraint,
};

fn values_fixture_package() -> PackageInterfaceHandle {
    let loader = JsonLoader::new(Box::new(ArrayLoader::new(None, false)));
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/plugin/fixtures/values-v1/composer.json");
    loader
        .load(JsonLoaderInput::String(
            path.canonicalize().unwrap().to_str().unwrap().to_string(),
        ))
        .unwrap()
}

fn simple(operator: &str, version: &str, pretty_string: Option<&str>) -> AnyConstraint {
    AnyConstraint::Simple(SimpleConstraint::new(
        operator.to_string(),
        version.to_string(),
        pretty_string.map(str::to_string),
    ))
}

fn link(target: &str, constraint: AnyConstraint, pretty_constraint: &str) -> Link {
    Link::new(
        "dummy/root".to_string(),
        target.to_string(),
        constraint,
        Some(Link::TYPE_REQUIRE.to_string()),
        pretty_constraint.to_string(),
    )
}

/// Every constraint shape the codec knows, so the rendering the plugin writes covers all four
/// classes and both the set and the unset pretty string.
fn seeded_requires() -> IndexMap<String, Link> {
    let mut requires = IndexMap::new();
    requires.insert(
        "foo/simple".to_string(),
        link("foo/simple", simple(">=", "1.0.0.0", None), ">=1.0"),
    );
    requires.insert(
        "foo/multi".to_string(),
        link(
            "foo/multi",
            AnyConstraint::Multi(MultiConstraint::new(
                vec![
                    simple(">=", "1.0.0.0", None),
                    simple("<", "2.0.0.0", Some("<2.0")),
                ],
                true,
                Some("^1.0".to_string()),
            )),
            "^1.0",
        ),
    );
    requires.insert(
        "foo/all".to_string(),
        link(
            "foo/all",
            AnyConstraint::MatchAll(MatchAllConstraint::new(None)),
            "*",
        ),
    );
    requires.insert(
        "foo/none".to_string(),
        link(
            "foo/none",
            AnyConstraint::MatchNone(MatchNoneConstraint::new(Some("nothing".to_string()))),
            "",
        ),
    );
    requires
}

fn describe(constraint: &AnyConstraint) -> String {
    let shape = match constraint {
        AnyConstraint::Simple(c) => {
            format!("Constraint({} {})", c.get_operator(), c.get_version())
        }
        AnyConstraint::Multi(c) => format!(
            "MultiConstraint({}: {})",
            if c.is_conjunctive() { "and" } else { "or" },
            c.get_constraints()
                .iter()
                .map(describe)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        AnyConstraint::MatchAll(_) => "MatchAllConstraint()".to_string(),
        AnyConstraint::MatchNone(_) => "MatchNoneConstraint()".to_string(),
    };
    format!("{shape} pretty={}", constraint.get_pretty_string())
}

fn describe_links(links: &IndexMap<String, Link>) -> Vec<String> {
    links
        .iter()
        .map(|(name, link)| {
            format!(
                "{name} | {} | {} | {} | {} | {}",
                link.get_source(),
                link.get_target(),
                link.get_description(),
                link.get_pretty_constraint(),
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
    package.set_requires(seeded_requires());

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
