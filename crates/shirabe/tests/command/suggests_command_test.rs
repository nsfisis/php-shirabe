//! ref: composer/tests/Composer/Test/Command/SuggestsCommandTest.php

use crate::test_case::{
    RunOptions, create_composer_lock, create_installed_json, get_application_tester,
    get_complete_package, get_version_constraint, init_temp_composer,
};
use indexmap::IndexMap;
use serial_test::serial;
use shirabe::package::Link;
use shirabe::package::handle::{CompletePackageHandle, PackageInterfaceHandle};
use shirabe_php_shim::PhpMixed;

/// ref: SuggestsCommandTest::getPackageWithSuggestAndRequires
fn get_package_with_suggest_and_requires(
    name: &str,
    version: &str,
    suggests: &[(&str, &str)],
    requires: &[(&str, Link)],
    require_devs: &[(&str, Link)],
) -> CompletePackageHandle {
    let pkg = get_complete_package(name, version);
    let suggests_map: IndexMap<String, String> = suggests
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    pkg.__set_suggests(suggests_map);
    let requires_map: IndexMap<String, Link> = requires
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
    pkg.__set_requires(requires_map);
    let dev_requires_map: IndexMap<String, Link> = require_devs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
    pkg.__set_dev_requires(dev_requires_map);
    pkg
}

fn link(source: &str, target: &str, description: &str) -> Link {
    Link::new(
        source.to_string(),
        target.to_string(),
        get_version_constraint(">=", "1.0"),
        Some(description.to_string()),
        "^1.0".to_string(),
    )
}

#[test]
#[serial]
#[ignore]
fn test_installed_packages_with_no_suggestions() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "repositories": {
                "packages": {
                    "type": "package",
                    "package": [
                        { "name": "vendor1/package1", "version": "1.0.0" },
                        { "name": "vendor2/package2", "version": "1.0.0" },
                    ],
                },
            },
            "require": {
                "vendor1/package1": "1.*",
                "vendor2/package2": "1.*",
            },
        })),
        None,
        None,
        true,
    );

    let packages: Vec<PackageInterfaceHandle> = vec![
        get_complete_package("vendor1/package1", "1.0.0").into(),
        get_complete_package("vendor2/package2", "1.0.0").into(),
    ];

    create_installed_json(&packages, &[], true);
    create_composer_lock(&packages, &[]);

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![(PhpMixed::from("command"), PhpMixed::from("suggest"))],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code);
    assert!(
        app_tester.get_display().trim().is_empty(),
        "expected empty display, got: {:?}",
        app_tester.get_display()
    );

    drop(tear_down);
}

/// Builds the package set shared by every `test_suggest` case.
fn suggest_packages() -> (Vec<PackageInterfaceHandle>, Vec<PackageInterfaceHandle>) {
    let packages: Vec<PackageInterfaceHandle> = vec![
        get_package_with_suggest_and_requires(
            "vendor1/package1",
            "1.0.0",
            &[("vendor3/suggested", "helpful for vendor1/package1")],
            &[(
                "vendor6/package6",
                link("vendor1/package1", "vendor6/package6", Link::TYPE_REQUIRE),
            )],
            &[
                (
                    "vendor4/dev-suggested",
                    link(
                        "vendor1/package1",
                        "vendor4/dev-suggested",
                        Link::TYPE_DEV_REQUIRE,
                    ),
                ),
                (
                    "vendor3/suggested",
                    link(
                        "vendor1/package1",
                        "vendor3/suggested",
                        Link::TYPE_DEV_REQUIRE,
                    ),
                ),
            ],
        )
        .into(),
        get_package_with_suggest_and_requires(
            "vendor6/package6",
            "1.0.0",
            &[("vendor7/transitive", "helpful for vendor6/package6")],
            &[],
            &[],
        )
        .into(),
    ];
    let dev_packages: Vec<PackageInterfaceHandle> = vec![
        get_package_with_suggest_and_requires(
            "vendor2/package2",
            "1.0.0",
            &[("vendor4/dev-suggested", "helpful for vendor2/package2")],
            &[(
                "vendor5/dev-package",
                link(
                    "vendor2/package2",
                    "vendor5/dev-package",
                    Link::TYPE_REQUIRE,
                ),
            )],
            &[],
        )
        .into(),
        get_package_with_suggest_and_requires(
            "vendor5/dev-package",
            "1.0.0",
            &[("vendor8/dev-transitive", "helpful for vendor5/dev-package")],
            &[],
            &[(
                "vendor8/dev-transitive",
                link(
                    "vendor5/dev-package",
                    "vendor8/dev-transitive",
                    Link::TYPE_DEV_REQUIRE,
                ),
            )],
        )
        .into(),
    ];
    (packages, dev_packages)
}

fn run_suggest_case(has_lock_file: bool, command: &[(&str, PhpMixed)], expected: &str) {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "repositories": {
                "packages": {
                    "type": "package",
                    "package": [
                        { "name": "vendor1/package1", "version": "1.0.0", "suggests": { "vendor3/suggested": "helpful for vendor1/package1" }, "require": { "vendor6/package6": "^1.0" }, "require-dev": { "vendor3/suggested": "^1.0", "vendor4/dev-suggested": "^1.0" } },
                        { "name": "vendor2/package2", "version": "1.0.0", "suggests": { "vendor4/dev-suggested": "helpful for vendor2/package2" }, "require": { "vendor5/dev-package": "^1.0" } },
                        { "name": "vendor5/dev-package", "version": "1.0.0", "suggests": { "vendor8/dev-transitive": "helpful for vendor5/dev-package" }, "require-dev": { "vendor8/dev-transitive": "^1.0" } },
                        { "name": "vendor6/package6", "version": "1.0.0", "suggests": { "vendor7/transitive": "helpful for vendor6/package6" } },
                    ],
                },
            },
            "require": { "vendor1/package1": "^1" },
            "require-dev": { "vendor2/package2": "^1" },
        })),
        None,
        None,
        true,
    );

    let (packages, dev_packages) = suggest_packages();
    create_installed_json(&packages, &dev_packages, true);
    if has_lock_file {
        create_composer_lock(&packages, &dev_packages);
    }

    let mut input: Vec<(PhpMixed, PhpMixed)> =
        vec![(PhpMixed::from("command"), PhpMixed::from("suggest"))];
    for (k, v) in command {
        input.push((PhpMixed::from(*k), v.clone()));
    }

    let mut app_tester = get_application_tester();
    let status_code = app_tester.run(input, RunOptions::default()).unwrap();
    assert_eq!(0, status_code);
    assert_eq!(expected.trim(), app_tester.get_display().trim());

    drop(tear_down);
}

#[test]
#[serial]
#[ignore]
fn test_suggest() {
    let t = PhpMixed::from(true);
    let by_package = ("--by-package", t.clone());
    let by_suggestion = ("--by-suggestion", t.clone());
    let no_dev = ("--no-dev", t.clone());
    let all = ("--all", t.clone());
    let list = ("--list", t.clone());

    // 'with lockfile, show suggested' / 'without lockfile, show suggested'
    let basic = "vendor1/package1 suggests:
 - vendor3/suggested: helpful for vendor1/package1

vendor2/package2 suggests:
 - vendor4/dev-suggested: helpful for vendor2/package2

2 additional suggestions by transitive dependencies can be shown with --all";
    run_suggest_case(true, &[], basic);
    run_suggest_case(false, &[], basic);

    // 'with lockfile, show suggested (excluding dev)'
    run_suggest_case(
        true,
        std::slice::from_ref(&no_dev),
        "vendor1/package1 suggests:
 - vendor3/suggested: helpful for vendor1/package1

1 additional suggestions by transitive dependencies can be shown with --all",
    );
    // 'without lockfile, show suggested (excluding dev)'
    run_suggest_case(false, std::slice::from_ref(&no_dev), basic);

    let all_suggested = "vendor1/package1 suggests:
 - vendor3/suggested: helpful for vendor1/package1

vendor2/package2 suggests:
 - vendor4/dev-suggested: helpful for vendor2/package2

vendor5/dev-package suggests:
 - vendor8/dev-transitive: helpful for vendor5/dev-package

vendor6/package6 suggests:
 - vendor7/transitive: helpful for vendor6/package6";
    run_suggest_case(true, std::slice::from_ref(&all), all_suggested);
    run_suggest_case(false, std::slice::from_ref(&all), all_suggested);

    // 'with lockfile, show all suggested (excluding dev)'
    run_suggest_case(
        true,
        &[all.clone(), no_dev.clone()],
        "vendor1/package1 suggests:
 - vendor3/suggested: helpful for vendor1/package1

vendor6/package6 suggests:
 - vendor7/transitive: helpful for vendor6/package6",
    );
    run_suggest_case(false, &[all.clone(), no_dev.clone()], all_suggested);

    // grouped by package
    run_suggest_case(true, std::slice::from_ref(&by_package), basic);
    run_suggest_case(false, std::slice::from_ref(&by_package), basic);
    run_suggest_case(
        true,
        &[by_package.clone(), no_dev.clone()],
        "vendor1/package1 suggests:
 - vendor3/suggested: helpful for vendor1/package1

1 additional suggestions by transitive dependencies can be shown with --all",
    );
    run_suggest_case(false, &[by_package.clone(), no_dev.clone()], basic);

    // grouped by suggestion
    let by_suggestion_out = "vendor3/suggested is suggested by:
 - vendor1/package1: helpful for vendor1/package1

vendor4/dev-suggested is suggested by:
 - vendor2/package2: helpful for vendor2/package2

2 additional suggestions by transitive dependencies can be shown with --all";
    run_suggest_case(
        true,
        std::slice::from_ref(&by_suggestion),
        by_suggestion_out,
    );
    run_suggest_case(
        false,
        std::slice::from_ref(&by_suggestion),
        by_suggestion_out,
    );
    run_suggest_case(
        true,
        &[by_suggestion.clone(), no_dev.clone()],
        "vendor3/suggested is suggested by:
 - vendor1/package1: helpful for vendor1/package1

1 additional suggestions by transitive dependencies can be shown with --all",
    );
    run_suggest_case(
        false,
        &[by_suggestion.clone(), no_dev.clone()],
        by_suggestion_out,
    );

    // grouped by package and suggestion
    let by_both = "vendor1/package1 suggests:
 - vendor3/suggested: helpful for vendor1/package1

vendor2/package2 suggests:
 - vendor4/dev-suggested: helpful for vendor2/package2

------------------------------------------------------------------------------
vendor3/suggested is suggested by:
 - vendor1/package1: helpful for vendor1/package1

vendor4/dev-suggested is suggested by:
 - vendor2/package2: helpful for vendor2/package2

2 additional suggestions by transitive dependencies can be shown with --all";
    run_suggest_case(true, &[by_package.clone(), by_suggestion.clone()], by_both);
    run_suggest_case(false, &[by_package.clone(), by_suggestion.clone()], by_both);
    run_suggest_case(
        true,
        &[by_package.clone(), by_suggestion.clone(), no_dev.clone()],
        "vendor1/package1 suggests:
 - vendor3/suggested: helpful for vendor1/package1

------------------------------------------------------------------------------
vendor3/suggested is suggested by:
 - vendor1/package1: helpful for vendor1/package1

1 additional suggestions by transitive dependencies can be shown with --all",
    );
    run_suggest_case(
        false,
        &[by_package.clone(), by_suggestion.clone(), no_dev.clone()],
        by_both,
    );

    // show suggested for package
    let for_pkg = "vendor2/package2 suggests:
 - vendor4/dev-suggested: helpful for vendor2/package2";
    run_suggest_case(
        true,
        &[(
            "packages",
            PhpMixed::List(vec![PhpMixed::from("vendor2/package2")]),
        )],
        for_pkg,
    );
    run_suggest_case(
        false,
        &[(
            "packages",
            PhpMixed::List(vec![PhpMixed::from("vendor2/package2")]),
        )],
        for_pkg,
    );

    // list suggested
    let list_out = "vendor3/suggested
vendor4/dev-suggested";
    run_suggest_case(true, std::slice::from_ref(&list), list_out);
    run_suggest_case(false, std::slice::from_ref(&list), list_out);
    run_suggest_case(true, &[list.clone(), no_dev.clone()], "vendor3/suggested");
    run_suggest_case(false, &[list.clone(), no_dev.clone()], list_out);

    let list_all = "vendor3/suggested
vendor4/dev-suggested
vendor7/transitive
vendor8/dev-transitive";
    run_suggest_case(true, &[list.clone(), all.clone()], list_all);
    run_suggest_case(false, &[list.clone(), all.clone()], list_all);
    run_suggest_case(
        true,
        &[list.clone(), all.clone(), no_dev.clone()],
        "vendor3/suggested
vendor7/transitive",
    );
    run_suggest_case(
        false,
        &[list.clone(), all.clone(), no_dev.clone()],
        list_all,
    );
}
