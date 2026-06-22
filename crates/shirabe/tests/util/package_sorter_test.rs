//! ref: composer/tests/Composer/Test/Util/PackageSorterTest.php

use indexmap::IndexMap;
use shirabe::package::Link;
use shirabe::package::handle::{PackageHandle, PackageInterfaceHandle};
use shirabe::util::package_sorter::PackageSorter;
use shirabe_semver::constraint::MatchAllConstraint;

fn create_package(name: &str, requires: &[&str]) -> PackageInterfaceHandle {
    let package = PackageHandle::new(name.to_string(), "1.0.0.0".to_string(), "1.0.0".to_string());

    let mut links: IndexMap<String, Link> = IndexMap::new();
    for require_name in requires {
        links.insert(
            require_name.to_string(),
            Link::new(
                package.get_name(),
                require_name.to_string(),
                MatchAllConstraint::new(None).into(),
                None,
                "*".to_string(),
            ),
        );
    }
    package.__set_requires(links);

    package.into()
}

fn names(packages: &[PackageInterfaceHandle]) -> Vec<String> {
    packages.iter().map(|p| p.get_name()).collect()
}

#[test]
fn test_sorting_does_nothing_with_no_dependencies() {
    let packages = vec![
        create_package("foo/bar1", &[]),
        create_package("foo/bar2", &[]),
        create_package("foo/bar3", &[]),
        create_package("foo/bar4", &[]),
    ];

    let expected = names(&packages);
    let sorted_packages = PackageSorter::sort_packages(packages, IndexMap::new());

    assert_eq!(expected, names(&sorted_packages));
}

fn sorting_orders_dependencies_higher_than_package_cases() -> Vec<(
    Vec<PackageInterfaceHandle>,
    Vec<&'static str>,
    IndexMap<String, i64>,
)> {
    vec![
        // one package is dep
        (
            vec![
                create_package("foo/bar1", &["foo/bar4"]),
                create_package("foo/bar2", &["foo/bar4"]),
                create_package("foo/bar3", &["foo/bar4"]),
                create_package("foo/bar4", &[]),
            ],
            vec!["foo/bar4", "foo/bar1", "foo/bar2", "foo/bar3"],
            IndexMap::new(),
        ),
        // one package has more deps
        (
            vec![
                create_package("foo/bar1", &["foo/bar2"]),
                create_package("foo/bar2", &["foo/bar4"]),
                create_package("foo/bar3", &["foo/bar4"]),
                create_package("foo/bar4", &[]),
            ],
            vec!["foo/bar4", "foo/bar2", "foo/bar1", "foo/bar3"],
            IndexMap::new(),
        ),
        // package is required by many, but requires one other
        (
            vec![
                create_package("foo/bar1", &["foo/bar3"]),
                create_package("foo/bar2", &["foo/bar3"]),
                create_package("foo/bar3", &["foo/bar4"]),
                create_package("foo/bar4", &[]),
                create_package("foo/bar5", &["foo/bar3"]),
                create_package("foo/bar6", &["foo/bar3"]),
            ],
            vec![
                "foo/bar4", "foo/bar3", "foo/bar1", "foo/bar2", "foo/bar5", "foo/bar6",
            ],
            IndexMap::new(),
        ),
        // one package has many requires
        (
            vec![
                create_package("foo/bar1", &["foo/bar2"]),
                create_package("foo/bar2", &[]),
                create_package("foo/bar3", &["foo/bar4"]),
                create_package("foo/bar4", &[]),
                create_package("foo/bar5", &["foo/bar2"]),
                create_package("foo/bar6", &["foo/bar2"]),
            ],
            vec![
                "foo/bar2", "foo/bar4", "foo/bar1", "foo/bar3", "foo/bar5", "foo/bar6",
            ],
            IndexMap::new(),
        ),
        // circular deps sorted alphabetically if weighted equally
        (
            vec![
                create_package("foo/bar1", &["circular/part1"]),
                create_package("foo/bar2", &["circular/part2"]),
                create_package("circular/part1", &["circular/part2"]),
                create_package("circular/part2", &["circular/part1"]),
            ],
            vec!["circular/part1", "circular/part2", "foo/bar1", "foo/bar2"],
            IndexMap::new(),
        ),
        // equal weight sorted alphabetically
        (
            vec![
                create_package("foo/bar10", &["foo/dep"]),
                create_package("foo/bar2", &["foo/dep"]),
                create_package("foo/baz", &["foo/dep"]),
                create_package("foo/dep", &[]),
            ],
            vec!["foo/dep", "foo/bar2", "foo/bar10", "foo/baz"],
            IndexMap::new(),
        ),
        // pre-weighted packages bumped to top incl their deps
        (
            vec![
                create_package("foo/bar", &["foo/dep"]),
                create_package("foo/bar2", &["foo/dep2"]),
                create_package("foo/dep", &[]),
                create_package("foo/dep2", &[]),
            ],
            vec!["foo/dep", "foo/bar", "foo/dep2", "foo/bar2"],
            IndexMap::from([("foo/bar".to_string(), -1000)]),
        ),
    ]
}

#[test]
#[ignore]
fn test_sorting_orders_dependencies_higher_than_package() {
    for (packages, expected_ordered_list, weights) in
        sorting_orders_dependencies_higher_than_package_cases()
    {
        let sorted_packages = PackageSorter::sort_packages(packages, weights);
        let sorted_package_names = names(&sorted_packages);

        assert_eq!(expected_ordered_list, sorted_package_names);
    }
}
