//! ref: composer/tests/Composer/Test/Command/SuggestsCommandTest.php sha1:2686aecc24005eb0a205f1dd76fd401acbf18d71

use crate::test_case::{
    RunOptions, create_composer_lock, create_installed_json, get_application_tester,
    get_complete_package, get_version_constraint, init_temp_composer,
};
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe::package::{CompletePackageHandle, Link, PackageInterfaceHandle};
use shirabe_php_shim::{ToPhpString, trim};
use shirabe_symfony_console::input::{InputValue, ParameterName};

/// ref: SuggestsCommandTest::getPackageWithSuggestAndRequires
fn get_package_with_suggest_and_requires(
    name: &BStr,
    version: &BStr,
    suggests: &[(&BStr, &BStr)],
    requires: &[(&BStr, Link)],
    require_devs: &[(&BStr, Link)],
) -> CompletePackageHandle {
    let pkg = get_complete_package(name, version);
    let suggests_map: IndexMap<BString, BString> = suggests
        .iter()
        .map(|(k, v)| (k.to_php_string(), v.to_php_string()))
        .collect();
    pkg.__set_suggests(
        suggests_map
            .into_iter()
            .collect::<indexmap::IndexMap<_, _>>(),
    );
    let requires_map: IndexMap<BString, Link> = requires
        .iter()
        .map(|(k, v)| (k.to_php_string(), v.clone()))
        .collect();
    pkg.__set_requires(
        requires_map
            .into_iter()
            .collect::<indexmap::IndexMap<_, _>>(),
    );
    let dev_requires_map: IndexMap<BString, Link> = require_devs
        .iter()
        .map(|(k, v)| (k.to_php_string(), v.clone()))
        .collect();
    pkg.__set_dev_requires(
        dev_requires_map
            .into_iter()
            .collect::<indexmap::IndexMap<_, _>>(),
    );
    pkg
}

fn link(source: &BStr, target: &BStr, description: &bstr::BStr) -> Link {
    Link::new(
        source.to_php_string(),
        target.to_php_string(),
        get_version_constraint(">=", "1.0"),
        Some(description.to_owned()),
        Some(BString::from("^1.0")),
    )
}

#[test]
#[serial_test::serial]
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
            vec![(ParameterName::of("command"), InputValue::from("suggest"))],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code);
    assert!(
        trim(app_tester.get_display()).is_empty(),
        "expected empty display, got: {:?}",
        app_tester.get_display()
    );

    drop(tear_down);
}

/// Builds the package set shared by every `test_suggest` case.
fn suggest_packages() -> (Vec<PackageInterfaceHandle>, Vec<PackageInterfaceHandle>) {
    let packages: Vec<PackageInterfaceHandle> = vec![
        get_package_with_suggest_and_requires(
            BStr::new("vendor1/package1"),
            BStr::new("1.0.0"),
            &[(
                BStr::new("vendor3/suggested"),
                BStr::new("helpful for vendor1/package1"),
            )],
            &[(
                BStr::new("vendor6/package6"),
                link(
                    BStr::new("vendor1/package1"),
                    BStr::new("vendor6/package6"),
                    Link::TYPE_REQUIRE,
                ),
            )],
            &[
                (
                    BStr::new("vendor4/dev-suggested"),
                    link(
                        BStr::new("vendor1/package1"),
                        BStr::new("vendor4/dev-suggested"),
                        Link::TYPE_DEV_REQUIRE,
                    ),
                ),
                (
                    BStr::new("vendor3/suggested"),
                    link(
                        BStr::new("vendor1/package1"),
                        BStr::new("vendor3/suggested"),
                        Link::TYPE_DEV_REQUIRE,
                    ),
                ),
            ],
        )
        .into(),
        get_package_with_suggest_and_requires(
            BStr::new("vendor6/package6"),
            BStr::new("1.0.0"),
            &[(
                BStr::new("vendor7/transitive"),
                BStr::new("helpful for vendor6/package6"),
            )],
            &[],
            &[],
        )
        .into(),
    ];
    let dev_packages: Vec<PackageInterfaceHandle> = vec![
        get_package_with_suggest_and_requires(
            BStr::new("vendor2/package2"),
            BStr::new("1.0.0"),
            &[(
                BStr::new("vendor4/dev-suggested"),
                BStr::new("helpful for vendor2/package2"),
            )],
            &[(
                BStr::new("vendor5/dev-package"),
                link(
                    BStr::new("vendor2/package2"),
                    BStr::new("vendor5/dev-package"),
                    Link::TYPE_REQUIRE,
                ),
            )],
            &[],
        )
        .into(),
        get_package_with_suggest_and_requires(
            BStr::new("vendor5/dev-package"),
            BStr::new("1.0.0"),
            &[(
                BStr::new("vendor8/dev-transitive"),
                BStr::new("helpful for vendor5/dev-package"),
            )],
            &[],
            &[(
                BStr::new("vendor8/dev-transitive"),
                link(
                    BStr::new("vendor5/dev-package"),
                    BStr::new("vendor8/dev-transitive"),
                    Link::TYPE_DEV_REQUIRE,
                ),
            )],
        )
        .into(),
    ];
    (packages, dev_packages)
}

fn run_suggest_case(has_lock_file: bool, command: &[(&BStr, InputValue)], expected: &BStr) {
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

    let mut input: Vec<(ParameterName, InputValue)> =
        vec![(ParameterName::of("command"), InputValue::from("suggest"))];
    for (k, v) in command {
        input.push((ParameterName::of(k), v.clone()));
    }

    let mut app_tester = get_application_tester();
    let status_code = app_tester.run(input, RunOptions::default()).unwrap();
    assert_eq!(0, status_code);
    assert_eq!(trim(expected), trim(app_tester.get_display()));

    drop(tear_down);
}

#[test]
#[serial_test::serial]
fn test_suggest() {
    let t = InputValue::from(true);
    let by_package = (BStr::new("--by-package"), t.clone());
    let by_suggestion = (BStr::new("--by-suggestion"), t.clone());
    let no_dev = (BStr::new("--no-dev"), t.clone());
    let all = (BStr::new("--all"), t.clone());
    let list = (BStr::new("--list"), t);

    // 'with lockfile, show suggested' / 'without lockfile, show suggested'
    let basic = BStr::new(
        "vendor1/package1 suggests:
 - vendor3/suggested: helpful for vendor1/package1

vendor2/package2 suggests:
 - vendor4/dev-suggested: helpful for vendor2/package2

2 additional suggestions by transitive dependencies can be shown with --all",
    );
    run_suggest_case(true, &[], basic);
    run_suggest_case(false, &[], basic);

    // 'with lockfile, show suggested (excluding dev)'
    run_suggest_case(
        true,
        std::slice::from_ref(&no_dev),
        BStr::new(
            "vendor1/package1 suggests:
 - vendor3/suggested: helpful for vendor1/package1

1 additional suggestions by transitive dependencies can be shown with --all",
        ),
    );
    // 'without lockfile, show suggested (excluding dev)'
    run_suggest_case(false, std::slice::from_ref(&no_dev), basic);

    let all_suggested = BStr::new(
        "vendor1/package1 suggests:
 - vendor3/suggested: helpful for vendor1/package1

vendor2/package2 suggests:
 - vendor4/dev-suggested: helpful for vendor2/package2

vendor5/dev-package suggests:
 - vendor8/dev-transitive: helpful for vendor5/dev-package

vendor6/package6 suggests:
 - vendor7/transitive: helpful for vendor6/package6",
    );
    run_suggest_case(true, std::slice::from_ref(&all), all_suggested);
    run_suggest_case(false, std::slice::from_ref(&all), all_suggested);

    // 'with lockfile, show all suggested (excluding dev)'
    run_suggest_case(
        true,
        &[all.clone(), no_dev.clone()],
        BStr::new(
            "vendor1/package1 suggests:
 - vendor3/suggested: helpful for vendor1/package1

vendor6/package6 suggests:
 - vendor7/transitive: helpful for vendor6/package6",
        ),
    );
    run_suggest_case(false, &[all.clone(), no_dev.clone()], all_suggested);

    // grouped by package
    run_suggest_case(true, std::slice::from_ref(&by_package), basic);
    run_suggest_case(false, std::slice::from_ref(&by_package), basic);
    run_suggest_case(
        true,
        &[by_package.clone(), no_dev.clone()],
        BStr::new(
            "vendor1/package1 suggests:
 - vendor3/suggested: helpful for vendor1/package1

1 additional suggestions by transitive dependencies can be shown with --all",
        ),
    );
    run_suggest_case(false, &[by_package.clone(), no_dev.clone()], basic);

    // grouped by suggestion
    let by_suggestion_out = BStr::new(
        "vendor3/suggested is suggested by:
 - vendor1/package1: helpful for vendor1/package1

vendor4/dev-suggested is suggested by:
 - vendor2/package2: helpful for vendor2/package2

2 additional suggestions by transitive dependencies can be shown with --all",
    );
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
        BStr::new(
            "vendor3/suggested is suggested by:
 - vendor1/package1: helpful for vendor1/package1

1 additional suggestions by transitive dependencies can be shown with --all",
        ),
    );
    run_suggest_case(
        false,
        &[by_suggestion.clone(), no_dev.clone()],
        by_suggestion_out,
    );

    // grouped by package and suggestion
    let by_both = BStr::new(
        "vendor1/package1 suggests:
 - vendor3/suggested: helpful for vendor1/package1

vendor2/package2 suggests:
 - vendor4/dev-suggested: helpful for vendor2/package2

------------------------------------------------------------------------------
vendor3/suggested is suggested by:
 - vendor1/package1: helpful for vendor1/package1

vendor4/dev-suggested is suggested by:
 - vendor2/package2: helpful for vendor2/package2

2 additional suggestions by transitive dependencies can be shown with --all",
    );
    run_suggest_case(true, &[by_package.clone(), by_suggestion.clone()], by_both);
    run_suggest_case(false, &[by_package.clone(), by_suggestion.clone()], by_both);
    run_suggest_case(
        true,
        &[by_package.clone(), by_suggestion.clone(), no_dev.clone()],
        BStr::new(
            "vendor1/package1 suggests:
 - vendor3/suggested: helpful for vendor1/package1

------------------------------------------------------------------------------
vendor3/suggested is suggested by:
 - vendor1/package1: helpful for vendor1/package1

1 additional suggestions by transitive dependencies can be shown with --all",
        ),
    );
    run_suggest_case(
        false,
        &[by_package.clone(), by_suggestion.clone(), no_dev.clone()],
        by_both,
    );

    // show suggested for package
    let for_pkg = BStr::new(
        "vendor2/package2 suggests:
 - vendor4/dev-suggested: helpful for vendor2/package2",
    );
    run_suggest_case(
        true,
        &[(
            BStr::new("packages"),
            InputValue::Array(vec!["vendor2/package2".into()]),
        )],
        for_pkg,
    );
    run_suggest_case(
        false,
        &[(
            BStr::new("packages"),
            InputValue::Array(vec!["vendor2/package2".into()]),
        )],
        for_pkg,
    );

    // list suggested
    let list_out = BStr::new(
        "vendor3/suggested
vendor4/dev-suggested",
    );
    run_suggest_case(true, std::slice::from_ref(&list), list_out);
    run_suggest_case(false, std::slice::from_ref(&list), list_out);
    run_suggest_case(
        true,
        &[list.clone(), no_dev.clone()],
        BStr::new("vendor3/suggested"),
    );
    run_suggest_case(false, &[list.clone(), no_dev.clone()], list_out);

    let list_all = BStr::new(
        "vendor3/suggested
vendor4/dev-suggested
vendor7/transitive
vendor8/dev-transitive",
    );
    run_suggest_case(true, &[list.clone(), all.clone()], list_all);
    run_suggest_case(false, &[list.clone(), all.clone()], list_all);
    run_suggest_case(
        true,
        &[list.clone(), all.clone(), no_dev.clone()],
        BStr::new(
            "vendor3/suggested
vendor7/transitive",
        ),
    );
    run_suggest_case(
        false,
        &[list.clone(), all.clone(), no_dev.clone()],
        list_all,
    );
}
