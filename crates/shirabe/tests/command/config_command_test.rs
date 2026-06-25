//! ref: composer/tests/Composer/Test/Command/ConfigCommandTest.php

use crate::test_case::{RunOptions, get_application_tester, init_temp_composer};
use serial_test::serial;
use shirabe_php_shim::PhpMixed;

/// `['command' => 'config'] + $command`, with the command name prepended.
fn config_input(command: Vec<(PhpMixed, PhpMixed)>) -> Vec<(PhpMixed, PhpMixed)> {
    let mut input = vec![(PhpMixed::from("command"), PhpMixed::from("config"))];
    input.extend(command);
    input
}

fn key(setting_key: &str) -> (PhpMixed, PhpMixed) {
    (PhpMixed::from("setting-key"), PhpMixed::from(setting_key))
}

fn value(values: &[&str]) -> (PhpMixed, PhpMixed) {
    (
        PhpMixed::from("setting-value"),
        PhpMixed::List(values.iter().map(|v| PhpMixed::from(*v)).collect()),
    )
}

fn flag(name: &str) -> (PhpMixed, PhpMixed) {
    (PhpMixed::from(name), PhpMixed::Bool(true))
}

/// Reads CWD's composer.json as a `serde_json::Value` (mirrors PHP's `json_decode(..., true)`).
fn read_composer_json() -> serde_json::Value {
    let contents = std::fs::read_to_string("composer.json").unwrap();
    serde_json::from_str(&contents).unwrap()
}

struct UpdateCase {
    name: &'static str,
    before: serde_json::Value,
    command: Vec<(PhpMixed, PhpMixed)>,
    expected: serde_json::Value,
}

#[test]
#[serial]
fn test_config_updates() {
    let cases: Vec<UpdateCase> = vec![
        UpdateCase {
            name: "set scripts",
            before: serde_json::json!({}),
            command: vec![key("scripts.test"), value(&["foo bar"])],
            expected: serde_json::json!({"scripts": {"test": "foo bar"}}),
        },
        UpdateCase {
            name: "unset scripts",
            before: serde_json::json!({"scripts": {"test": "foo bar", "lala": "baz"}}),
            command: vec![key("scripts.lala"), flag("--unset")],
            expected: serde_json::json!({"scripts": {"test": "foo bar"}}),
        },
        UpdateCase {
            name: "set single config with bool normalizer",
            before: serde_json::json!({}),
            command: vec![key("use-github-api"), value(&["1"])],
            expected: serde_json::json!({"config": {"use-github-api": true}}),
        },
        UpdateCase {
            name: "set multi config",
            before: serde_json::json!({}),
            command: vec![key("github-protocols"), value(&["https", "git"])],
            expected: serde_json::json!({"config": {"github-protocols": ["https", "git"]}}),
        },
        UpdateCase {
            name: "set version",
            before: serde_json::json!({}),
            command: vec![key("version"), value(&["1.0.0"])],
            expected: serde_json::json!({"version": "1.0.0"}),
        },
        UpdateCase {
            name: "unset version",
            before: serde_json::json!({"version": "1.0.0"}),
            command: vec![key("version"), flag("--unset")],
            expected: serde_json::json!({}),
        },
        UpdateCase {
            name: "unset arbitrary property",
            before: serde_json::json!({"random-prop": "1.0.0"}),
            command: vec![key("random-prop"), flag("--unset")],
            expected: serde_json::json!({}),
        },
        UpdateCase {
            name: "set preferred-install",
            before: serde_json::json!({}),
            command: vec![key("preferred-install.foo/*"), value(&["source"])],
            expected: serde_json::json!({"config": {"preferred-install": {"foo/*": "source"}}}),
        },
        UpdateCase {
            name: "unset preferred-install",
            before: serde_json::json!({"config": {"preferred-install": {"foo/*": "source"}}}),
            command: vec![key("preferred-install.foo/*"), flag("--unset")],
            expected: serde_json::json!({"config": {"preferred-install": {}}}),
        },
        UpdateCase {
            name: "unset platform",
            before: serde_json::json!({"config": {"platform": {"php": "7.2.5"}, "platform-check": false}}),
            command: vec![key("platform.php"), flag("--unset")],
            expected: serde_json::json!({"config": {"platform": {}, "platform-check": false}}),
        },
        UpdateCase {
            name: "set extra with merge",
            before: serde_json::json!({}),
            command: vec![
                key("extra.patches.foo/bar"),
                value(&["{\"123\":\"value\"}"]),
                flag("--json"),
                flag("--merge"),
            ],
            expected: serde_json::json!({"extra": {"patches": {"foo/bar": {"123": "value"}}}}),
        },
        UpdateCase {
            name: "combine extra with merge",
            before: serde_json::json!({"extra": {"patches": {"foo/bar": {"5": "oldvalue"}}}}),
            command: vec![
                key("extra.patches.foo/bar"),
                value(&["{\"123\":\"value\"}"]),
                flag("--json"),
                flag("--merge"),
            ],
            expected: serde_json::json!({"extra": {"patches": {"foo/bar": {"123": "value", "5": "oldvalue"}}}}),
        },
        UpdateCase {
            name: "combine extra with list",
            before: serde_json::json!({"extra": {"patches": {"foo/bar": ["oldvalue"]}}}),
            command: vec![
                key("extra.patches.foo/bar"),
                value(&["{\"123\":\"value\"}"]),
                flag("--json"),
                flag("--merge"),
            ],
            expected: serde_json::json!({"extra": {"patches": {"foo/bar": {"123": "value", "0": "oldvalue"}}}}),
        },
        UpdateCase {
            name: "overwrite extra with merge",
            before: serde_json::json!({"extra": {"patches": {"foo/bar": {"123": "oldvalue"}}}}),
            command: vec![
                key("extra.patches.foo/bar"),
                value(&["{\"123\":\"value\"}"]),
                flag("--json"),
                flag("--merge"),
            ],
            expected: serde_json::json!({"extra": {"patches": {"foo/bar": {"123": "value"}}}}),
        },
        UpdateCase {
            name: "unset autoload",
            before: serde_json::json!({"autoload": {"psr-4": ["test"], "classmap": ["test"]}}),
            command: vec![key("autoload.psr-4"), flag("--unset")],
            expected: serde_json::json!({"autoload": {"classmap": ["test"]}}),
        },
        UpdateCase {
            name: "unset autoload-dev",
            before: serde_json::json!({"autoload-dev": {"psr-4": ["test"], "classmap": ["test"]}}),
            command: vec![key("autoload-dev.psr-4"), flag("--unset")],
            expected: serde_json::json!({"autoload-dev": {"classmap": ["test"]}}),
        },
        UpdateCase {
            name: "set audit.ignore-unreachable",
            before: serde_json::json!({}),
            command: vec![key("audit.ignore-unreachable"), value(&["true"])],
            expected: serde_json::json!({"config": {"audit": {"ignore-unreachable": true}}}),
        },
        UpdateCase {
            name: "set audit.block-insecure",
            before: serde_json::json!({}),
            command: vec![key("audit.block-insecure"), value(&["false"])],
            expected: serde_json::json!({"config": {"audit": {"block-insecure": false}}}),
        },
        UpdateCase {
            name: "set audit.block-abandoned",
            before: serde_json::json!({}),
            command: vec![key("audit.block-abandoned"), value(&["true"])],
            expected: serde_json::json!({"config": {"audit": {"block-abandoned": true}}}),
        },
        UpdateCase {
            name: "unset audit.ignore-unreachable",
            before: serde_json::json!({"config": {"audit": {"ignore-unreachable": true}}}),
            command: vec![key("audit.ignore-unreachable"), flag("--unset")],
            expected: serde_json::json!({"config": {"audit": {}}}),
        },
        UpdateCase {
            name: "set audit.ignore-severity",
            before: serde_json::json!({}),
            command: vec![key("audit.ignore-severity"), value(&["low", "medium"])],
            expected: serde_json::json!({"config": {"audit": {"ignore-severity": ["low", "medium"]}}}),
        },
        UpdateCase {
            name: "set audit.ignore as array",
            before: serde_json::json!({}),
            command: vec![
                key("audit.ignore"),
                value(&["[\"CVE-2024-1234\",\"GHSA-xxxx-yyyy\"]"]),
                flag("--json"),
            ],
            expected: serde_json::json!({"config": {"audit": {"ignore": ["CVE-2024-1234", "GHSA-xxxx-yyyy"]}}}),
        },
        UpdateCase {
            name: "set audit.ignore as object",
            before: serde_json::json!({}),
            command: vec![
                key("audit.ignore"),
                value(&[
                    "{\"CVE-2024-1234\":\"False positive\",\"GHSA-xxxx-yyyy\":\"Not applicable\"}",
                ]),
                flag("--json"),
            ],
            expected: serde_json::json!({"config": {"audit": {"ignore": {"CVE-2024-1234": "False positive", "GHSA-xxxx-yyyy": "Not applicable"}}}}),
        },
        UpdateCase {
            name: "merge audit.ignore array",
            before: serde_json::json!({"config": {"audit": {"ignore": ["CVE-2024-1234"]}}}),
            command: vec![
                key("audit.ignore"),
                value(&["[\"CVE-2024-5678\"]"]),
                flag("--json"),
                flag("--merge"),
            ],
            expected: serde_json::json!({"config": {"audit": {"ignore": ["CVE-2024-1234", "CVE-2024-5678"]}}}),
        },
        UpdateCase {
            name: "merge audit.ignore object",
            before: serde_json::json!({"config": {"audit": {"ignore": {"CVE-2024-1234": "Old reason"}}}}),
            command: vec![
                key("audit.ignore"),
                value(&["{\"CVE-2024-5678\":\"New advisory\"}"]),
                flag("--json"),
                flag("--merge"),
            ],
            expected: serde_json::json!({"config": {"audit": {"ignore": {"CVE-2024-5678": "New advisory", "CVE-2024-1234": "Old reason"}}}}),
        },
        UpdateCase {
            name: "overwrite audit.ignore key with merge",
            before: serde_json::json!({"config": {"audit": {"ignore": {"CVE-2024-1234": "Old reason"}}}}),
            command: vec![
                key("audit.ignore"),
                value(&["{\"CVE-2024-1234\":\"New reason\"}"]),
                flag("--json"),
                flag("--merge"),
            ],
            expected: serde_json::json!({"config": {"audit": {"ignore": {"CVE-2024-1234": "New reason"}}}}),
        },
        UpdateCase {
            name: "set audit.ignore-abandoned as array",
            before: serde_json::json!({}),
            command: vec![
                key("audit.ignore-abandoned"),
                value(&["[\"vendor/package1\",\"vendor/package2\"]"]),
                flag("--json"),
            ],
            expected: serde_json::json!({"config": {"audit": {"ignore-abandoned": ["vendor/package1", "vendor/package2"]}}}),
        },
        UpdateCase {
            name: "set audit.ignore-abandoned as object",
            before: serde_json::json!({}),
            command: vec![
                key("audit.ignore-abandoned"),
                value(&[
                    "{\"vendor/package1\":\"Still maintained\",\"vendor/package2\":\"Fork available\"}",
                ]),
                flag("--json"),
            ],
            expected: serde_json::json!({"config": {"audit": {"ignore-abandoned": {"vendor/package1": "Still maintained", "vendor/package2": "Fork available"}}}}),
        },
        UpdateCase {
            name: "merge audit.ignore-abandoned array",
            before: serde_json::json!({"config": {"audit": {"ignore-abandoned": ["vendor/package1"]}}}),
            command: vec![
                key("audit.ignore-abandoned"),
                value(&["[\"vendor/package2\"]"]),
                flag("--json"),
                flag("--merge"),
            ],
            expected: serde_json::json!({"config": {"audit": {"ignore-abandoned": ["vendor/package1", "vendor/package2"]}}}),
        },
        UpdateCase {
            name: "merge audit.ignore-abandoned object",
            before: serde_json::json!({"config": {"audit": {"ignore-abandoned": {"vendor/package1": "Old reason"}}}}),
            command: vec![
                key("audit.ignore-abandoned"),
                value(&["{\"vendor/package2\":\"New reason\"}"]),
                flag("--json"),
                flag("--merge"),
            ],
            expected: serde_json::json!({"config": {"audit": {"ignore-abandoned": {"vendor/package2": "New reason", "vendor/package1": "Old reason"}}}}),
        },
        UpdateCase {
            name: "unset audit.ignore",
            before: serde_json::json!({"config": {"audit": {"ignore": ["CVE-2024-1234"]}}}),
            command: vec![key("audit.ignore"), flag("--unset")],
            expected: serde_json::json!({"config": {"audit": {}}}),
        },
        UpdateCase {
            name: "unset audit.ignore-abandoned",
            before: serde_json::json!({"config": {"audit": {"ignore-abandoned": ["vendor/package1"]}}}),
            command: vec![key("audit.ignore-abandoned"), flag("--unset")],
            expected: serde_json::json!({"config": {"audit": {}}}),
        },
    ];

    for case in cases {
        let _tear_down = init_temp_composer(Some(&case.before), None, None, false);

        let mut app_tester = get_application_tester();
        app_tester
            .run(config_input(case.command), RunOptions::default())
            .unwrap_or_else(|e| panic!("case {:?}: run failed: {:?}", case.name, e));

        assert_eq!(
            0,
            app_tester.get_status_code(),
            "case {:?}: display: {}",
            case.name,
            app_tester.get_display()
        );

        assert_eq!(case.expected, read_composer_json(), "case {:?}", case.name);
    }
}

struct ReadCase {
    name: &'static str,
    composer_json: serde_json::Value,
    command: Vec<(PhpMixed, PhpMixed)>,
    expected: &'static str,
}

#[test]
#[serial]
fn test_config_reads() {
    let cases: Vec<ReadCase> = vec![
        ReadCase {
            name: "read description",
            composer_json: serde_json::json!({"description": "foo bar"}),
            command: vec![key("description")],
            expected: "foo bar",
        },
        ReadCase {
            name: "read vendor-dir with source",
            composer_json: serde_json::json!({"config": {"vendor-dir": "lala"}}),
            command: vec![key("vendor-dir"), flag("--source")],
            expected: "lala (./composer.json)",
        },
        ReadCase {
            name: "read default vendor-dir",
            composer_json: serde_json::json!({}),
            command: vec![key("vendor-dir")],
            expected: "vendor",
        },
        ReadCase {
            name: "read repos by named key",
            composer_json: serde_json::json!({"repositories": {"foo": {"type": "vcs", "url": "https://example.org"}, "packagist.org": {"type": "composer", "url": "https://repo.packagist.org"}}}),
            command: vec![key("repositories.foo")],
            expected: "{\"type\":\"vcs\",\"url\":\"https://example.org\"}",
        },
        ReadCase {
            name: "read all repos includes the default packagist",
            composer_json: serde_json::json!({"repositories": {"foo": {"type": "vcs", "url": "https://example.org"}, "packagist.org": {"type": "composer", "url": "https://repo.packagist.org"}}}),
            command: vec![key("repos")],
            expected: "{\"foo\":{\"type\":\"vcs\",\"url\":\"https://example.org\"},\"packagist.org\":{\"type\":\"composer\",\"url\":\"https://repo.packagist.org\"}}",
        },
        ReadCase {
            name: "read all repos does not include the disabled packagist",
            composer_json: serde_json::json!({"repositories": {"foo": {"type": "vcs", "url": "https://example.org"}, "packagist.org": false}}),
            command: vec![key("repos")],
            expected: "{\"foo\":{\"type\":\"vcs\",\"url\":\"https://example.org\"}}",
        },
    ];

    for case in cases {
        let _tear_down = init_temp_composer(Some(&case.composer_json), None, None, false);

        let mut app_tester = get_application_tester();
        app_tester
            .run(config_input(case.command), RunOptions::default())
            .unwrap_or_else(|e| panic!("case {:?}: run failed: {:?}", case.name, e));

        assert_eq!(0, app_tester.get_status_code(), "case {:?}", case.name);

        assert_eq!(
            case.expected,
            app_tester.get_display().trim(),
            "case {:?}",
            case.name
        );

        // The composer.json should not be modified by config reads.
        assert_eq!(
            case.composer_json,
            read_composer_json(),
            "case {:?}: composer.json must not be modified by config reads",
            case.name
        );
    }
}

/// ref: provideConfigReads 'read repos by numeric index'.
///
/// Split out from the rest because list-form `repositories` makes `Config::all()` normalize the
/// repository through RepositoryFactory, which instantiates a repository and reaches
/// shirabe-php-shim stream_set_blocking (stream.rs todo!(), requires fcntl(2)).
#[test]
#[serial]
#[ignore = "list-form repositories drive Config::all -> RepositoryFactory, reaching \
            shirabe-php-shim stream_set_blocking (stream.rs todo!(), requires fcntl(2))"]
fn test_config_reads_repos_by_numeric_index() {
    let composer_json = serde_json::json!({"repositories": [{"type": "vcs", "url": "https://example.org"}, {"type": "composer", "url": "https://repo.packagist.org"}]});
    let _tear_down = init_temp_composer(Some(&composer_json), None, None, false);

    let mut app_tester = get_application_tester();
    app_tester
        .run(config_input(vec![key("repos.0")]), RunOptions::default())
        .unwrap();

    assert_eq!(0, app_tester.get_status_code());
    assert_eq!(
        "{\"type\":\"vcs\",\"url\":\"https://example.org\"}",
        app_tester.get_display().trim()
    );
}

#[test]
#[serial]
fn test_config_throws_for_invalid_arg_combination() {
    let mut app_tester = get_application_tester();
    let result = app_tester.run(
        config_input(vec![
            (
                PhpMixed::from("--file"),
                PhpMixed::from("alt.composer.json"),
            ),
            flag("--global"),
        ]),
        RunOptions::default(),
    );

    let err = result.expect_err("expected RuntimeException");
    assert!(
        err.to_string()
            .contains("--file and --global can not be combined"),
        "got: {:?}",
        err
    );
}

#[test]
#[serial]
fn test_config_throws_for_invalid_severity() {
    let _tear_down = init_temp_composer(Some(&serde_json::json!({})), None, None, false);

    let mut app_tester = get_application_tester();
    let result = app_tester.run(
        config_input(vec![
            key("audit.ignore-severity"),
            value(&["low", "invalid"]),
        ]),
        RunOptions::default(),
    );

    let err = result.expect_err("expected RuntimeException");
    assert!(
        err.to_string()
            .contains("valid severities include: low, medium, high, critical"),
        "got: {:?}",
        err
    );
}

#[test]
#[serial]
fn test_config_throws_when_merging_array_with_object() {
    let _tear_down = init_temp_composer(
        Some(&serde_json::json!({"config": {"audit": {"ignore": ["CVE-2024-1234"]}}})),
        None,
        None,
        false,
    );

    let mut app_tester = get_application_tester();
    let result = app_tester.run(
        config_input(vec![
            key("audit.ignore"),
            value(&["{\"CVE-2024-5678\":\"reason\"}"]),
            flag("--json"),
            flag("--merge"),
        ]),
        RunOptions::default(),
    );

    let err = result.expect_err("expected RuntimeException");
    assert!(
        err.to_string().contains("Cannot merge array and object"),
        "got: {:?}",
        err
    );
}
