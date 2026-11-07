//! ref: composer/tests/Composer/Test/Util/PackageSorterTest.php sha1:4b88c411719eccf0ad4243f0938e16d009393493

use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe::package::{Link, PackageHandle, PackageInterfaceHandle};
use shirabe::util::PackageSorter;
use shirabe_php_shim::bconcat;
use shirabe_semver::constraint::MatchAllConstraint;

fn create_package(name: &BStr, requires: &[&BStr]) -> PackageInterfaceHandle {
    let package = PackageHandle::new(
        bconcat!(name),
        BString::from("1.0.0.0"),
        BString::from("1.0.0"),
    );

    let mut links: IndexMap<BString, Link> = IndexMap::new();
    for require_name in requires {
        links.insert(
            bconcat!(require_name),
            Link::new(
                package.get_name(),
                bconcat!(require_name),
                MatchAllConstraint::new(None).into(),
                None,
                Some(BString::from("*")),
            ),
        );
    }
    package.__set_requires(links.into_iter().collect::<indexmap::IndexMap<_, _>>());

    package.into()
}

fn names(packages: &[PackageInterfaceHandle]) -> Vec<BString> {
    packages.iter().map(|p| p.get_name()).collect()
}

#[test]
fn test_sorting_does_nothing_with_no_dependencies() {
    let packages = vec![
        create_package(BStr::new("foo/bar1"), &[]),
        create_package(BStr::new("foo/bar2"), &[]),
        create_package(BStr::new("foo/bar3"), &[]),
        create_package(BStr::new("foo/bar4"), &[]),
    ];

    let expected = names(&packages);
    let sorted_packages = PackageSorter::sort_packages(packages, IndexMap::new());

    assert_eq!(expected, names(&sorted_packages));
}

fn sorting_orders_dependencies_higher_than_package_cases() -> Vec<(
    Vec<PackageInterfaceHandle>,
    Vec<&'static BStr>,
    IndexMap<bstr::BString, i64>,
)> {
    vec![
        // one package is dep
        (
            vec![
                create_package(BStr::new("foo/bar1"), &[BStr::new("foo/bar4")]),
                create_package(BStr::new("foo/bar2"), &[BStr::new("foo/bar4")]),
                create_package(BStr::new("foo/bar3"), &[BStr::new("foo/bar4")]),
                create_package(BStr::new("foo/bar4"), &[]),
            ],
            vec![
                BStr::new("foo/bar4"),
                BStr::new("foo/bar1"),
                BStr::new("foo/bar2"),
                BStr::new("foo/bar3"),
            ],
            IndexMap::new(),
        ),
        // one package has more deps
        (
            vec![
                create_package(BStr::new("foo/bar1"), &[BStr::new("foo/bar2")]),
                create_package(BStr::new("foo/bar2"), &[BStr::new("foo/bar4")]),
                create_package(BStr::new("foo/bar3"), &[BStr::new("foo/bar4")]),
                create_package(BStr::new("foo/bar4"), &[]),
            ],
            vec![
                BStr::new("foo/bar4"),
                BStr::new("foo/bar2"),
                BStr::new("foo/bar1"),
                BStr::new("foo/bar3"),
            ],
            IndexMap::new(),
        ),
        // package is required by many, but requires one other
        (
            vec![
                create_package(BStr::new("foo/bar1"), &[BStr::new("foo/bar3")]),
                create_package(BStr::new("foo/bar2"), &[BStr::new("foo/bar3")]),
                create_package(BStr::new("foo/bar3"), &[BStr::new("foo/bar4")]),
                create_package(BStr::new("foo/bar4"), &[]),
                create_package(BStr::new("foo/bar5"), &[BStr::new("foo/bar3")]),
                create_package(BStr::new("foo/bar6"), &[BStr::new("foo/bar3")]),
            ],
            vec![
                BStr::new("foo/bar4"),
                BStr::new("foo/bar3"),
                BStr::new("foo/bar1"),
                BStr::new("foo/bar2"),
                BStr::new("foo/bar5"),
                BStr::new("foo/bar6"),
            ],
            IndexMap::new(),
        ),
        // one package has many requires
        (
            vec![
                create_package(BStr::new("foo/bar1"), &[BStr::new("foo/bar2")]),
                create_package(BStr::new("foo/bar2"), &[]),
                create_package(BStr::new("foo/bar3"), &[BStr::new("foo/bar4")]),
                create_package(BStr::new("foo/bar4"), &[]),
                create_package(BStr::new("foo/bar5"), &[BStr::new("foo/bar2")]),
                create_package(BStr::new("foo/bar6"), &[BStr::new("foo/bar2")]),
            ],
            vec![
                BStr::new("foo/bar2"),
                BStr::new("foo/bar4"),
                BStr::new("foo/bar1"),
                BStr::new("foo/bar3"),
                BStr::new("foo/bar5"),
                BStr::new("foo/bar6"),
            ],
            IndexMap::new(),
        ),
        // circular deps sorted alphabetically if weighted equally
        (
            vec![
                create_package(BStr::new("foo/bar1"), &[BStr::new("circular/part1")]),
                create_package(BStr::new("foo/bar2"), &[BStr::new("circular/part2")]),
                create_package(BStr::new("circular/part1"), &[BStr::new("circular/part2")]),
                create_package(BStr::new("circular/part2"), &[BStr::new("circular/part1")]),
            ],
            vec![
                BStr::new("circular/part1"),
                BStr::new("circular/part2"),
                BStr::new("foo/bar1"),
                BStr::new("foo/bar2"),
            ],
            IndexMap::new(),
        ),
        // equal weight sorted alphabetically
        (
            vec![
                create_package(BStr::new("foo/bar10"), &[BStr::new("foo/dep")]),
                create_package(BStr::new("foo/bar2"), &[BStr::new("foo/dep")]),
                create_package(BStr::new("foo/baz"), &[BStr::new("foo/dep")]),
                create_package(BStr::new("foo/dep"), &[]),
            ],
            vec![
                BStr::new("foo/dep"),
                BStr::new("foo/bar2"),
                BStr::new("foo/bar10"),
                BStr::new("foo/baz"),
            ],
            IndexMap::new(),
        ),
        // pre-weighted packages bumped to top incl their deps
        (
            vec![
                create_package(BStr::new("foo/bar"), &[BStr::new("foo/dep")]),
                create_package(BStr::new("foo/bar2"), &[BStr::new("foo/dep2")]),
                create_package(BStr::new("foo/dep"), &[]),
                create_package(BStr::new("foo/dep2"), &[]),
            ],
            vec![
                BStr::new("foo/dep"),
                BStr::new("foo/bar"),
                BStr::new("foo/dep2"),
                BStr::new("foo/bar2"),
            ],
            IndexMap::from([(bstr::BString::from("foo/bar"), -1000)]),
        ),
    ]
}

#[test]
fn test_sorting_orders_dependencies_higher_than_package() {
    for (packages, expected_ordered_list, weights) in
        sorting_orders_dependencies_higher_than_package_cases()
    {
        let sorted_packages = PackageSorter::sort_packages(packages, weights);
        let sorted_package_names = names(&sorted_packages);

        assert_eq!(expected_ordered_list, sorted_package_names);
    }
}
