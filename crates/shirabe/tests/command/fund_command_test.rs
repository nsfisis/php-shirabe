//! ref: composer/tests/Composer/Test/Command/FundCommandTest.php

use crate::test_case::{
    RunOptions, create_installed_json, get_application_tester, get_complete_package,
    init_temp_composer,
};
use indexmap::IndexMap;
use serial_test::serial;
use shirabe::package::handle::{CompletePackageHandle, PackageInterfaceHandle};
use shirabe_php_shim::PhpMixed;

fn funding_entry(r#type: &str, url: &str) -> IndexMap<String, PhpMixed> {
    let mut m = IndexMap::new();
    m.insert("type".to_string(), PhpMixed::from(r#type));
    m.insert("url".to_string(), PhpMixed::from(url));
    m
}

/// Runs one `useCaseProvider` case.
fn run_fund_case(
    composer_json: serde_json::Value,
    command: &[(&str, PhpMixed)],
    funding: &[(&str, IndexMap<String, PhpMixed>)],
    expected: &str,
) {
    let tear_down = init_temp_composer(Some(&composer_json), None, None, true);

    let first = get_complete_package("first/pkg", "2.3.4");
    let stable = get_complete_package("stable/pkg", "1.0.0");
    let dev = get_complete_package("dev/pkg", "2.3.4.5");

    let by_name: IndexMap<&str, &CompletePackageHandle> = [
        ("first/pkg", &first),
        ("stable/pkg", &stable),
        ("dev/pkg", &dev),
    ]
    .into_iter()
    .collect();

    for (pkg_name, info) in funding {
        if let Some(pkg) = by_name.get(pkg_name) {
            pkg.set_funding(vec![info.clone()]);
        }
    }

    let packages: Vec<PackageInterfaceHandle> = vec![first.into(), stable.into()];
    let dev_packages: Vec<PackageInterfaceHandle> = vec![dev.into()];

    create_installed_json(&packages, &dev_packages, true);

    let mut input: Vec<(PhpMixed, PhpMixed)> =
        vec![(PhpMixed::from("command"), PhpMixed::from("fund"))];
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
fn test_fund_command() {
    // 'no funding links present, locally or remotely'
    run_fund_case(
        serde_json::json!({
            "repositories": [],
            "require": { "first/pkg": "^2.0" },
            "require-dev": { "dev/pkg": "~4.0" },
        }),
        &[],
        &[],
        "No funding links were found in your package dependencies. This doesn't mean they don't need your support!",
    );

    // 'funding links set locally are used as fallback if not found remotely'
    run_fund_case(
        serde_json::json!({
            "repositories": [],
            "require": { "first/pkg": "^2.0" },
            "require-dev": { "dev/pkg": "~4.0" },
        }),
        &[],
        &[
            (
                "first/pkg",
                funding_entry("github", "https://github.com/composer-test-data"),
            ),
            (
                "dev/pkg",
                funding_entry("github", "https://github.com/composer-test-data-dev"),
            ),
        ],
        "The following packages were found in your dependencies which publish funding information:

dev
  pkg
    https://github.com/sponsors/composer-test-data-dev

first
    https://github.com/sponsors/composer-test-data

Please consider following these links and sponsoring the work of package authors!
Thank you!",
    );

    // 'funding links set remotely are used as primary if found'
    run_fund_case(
        serde_json::json!({
            "repositories": [
                {
                    "type": "package",
                    "package": [
                        { "name": "first/pkg", "version": "dev-foo", "funding": [{ "type": "github", "url": "https://github.com/test-should-not-be-used" }] },
                        { "name": "first/pkg", "version": "dev-main", "default-branch": true, "funding": [{ "type": "custom", "url": "https://example.org" }] },
                        { "name": "dev/pkg", "version": "dev-foo", "default-branch": true, "funding": [{ "type": "github", "url": "https://github.com/org" }] },
                        { "name": "stable/pkg", "version": "1.0.0", "funding": [{ "type": "github", "url": "org2" }] },
                    ],
                },
            ],
            "require": { "first/pkg": "^2.0", "stable/pkg": "^1.0" },
            "require-dev": { "dev/pkg": "~4.0" },
        }),
        &[],
        &[
            (
                "first/pkg",
                funding_entry("github", "https://github.com/composer-test-data"),
            ),
            (
                "dev/pkg",
                funding_entry("github", "https://github.com/composer-test-data-dev"),
            ),
            (
                "stable/pkg",
                funding_entry("github", "https://github.com/composer-test-data-stable"),
            ),
        ],
        "The following packages were found in your dependencies which publish funding information:

dev
  pkg
    https://github.com/sponsors/org

first
    https://example.org

stable
    https://github.com/sponsors/composer-test-data-stable

Please consider following these links and sponsoring the work of package authors!
Thank you!",
    );

    // 'format funding links as JSON'
    run_fund_case(
        serde_json::json!({
            "repositories": [],
            "require": { "first/pkg": "^2.0" },
            "require-dev": { "dev/pkg": "~4.0" },
        }),
        &[("--format", PhpMixed::from("json"))],
        &[
            (
                "first/pkg",
                funding_entry("github", "https://github.com/composer-test-data"),
            ),
            (
                "dev/pkg",
                funding_entry("github", "https://github.com/composer-test-data-dev"),
            ),
        ],
        r#"{
    "dev": {
        "https://github.com/sponsors/composer-test-data-dev": [
            "pkg"
        ]
    },
    "first": {
        "https://github.com/sponsors/composer-test-data": [
            "pkg"
        ]
    }
}"#,
    );
}
