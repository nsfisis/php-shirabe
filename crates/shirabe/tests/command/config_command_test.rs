//! ref: composer/tests/Composer/Test/Command/ConfigCommandTest.php sha1:8ce0a50b7daa739dbb09c9460556d5b63a2db11d

use crate::test_case::{RunOptions, get_application_tester, init_temp_composer};
use bstr::{BStr, ByteSlice};
use serial_test::serial;
use shirabe_php_shim::{error_message, trim};
use shirabe_symfony_console::input::{InputValue, ParameterName};

/// `['command' => 'config'] + $command`, with the command name prepended.
fn config_input(command: Vec<(ParameterName, InputValue)>) -> Vec<(ParameterName, InputValue)> {
    let mut input = vec![(ParameterName::of("command"), InputValue::from("config"))];
    input.extend(command);
    input
}

fn key(setting_key: &BStr) -> (ParameterName, InputValue) {
    (
        ParameterName::of("setting-key"),
        InputValue::from(setting_key),
    )
}

fn value(values: &[&BStr]) -> (ParameterName, InputValue) {
    (
        ParameterName::of("setting-value"),
        InputValue::Array(values.iter().map(|v| (*v).into()).collect()),
    )
}

fn flag(name: &BStr) -> (ParameterName, InputValue) {
    (ParameterName::of(name), InputValue::Bool(true))
}

/// Reads CWD's composer.json as a `serde_json::Value` (mirrors PHP's `json_decode(..., true)`).
fn read_composer_json() -> serde_json::Value {
    let contents = bstr::BString::from(std::fs::read("composer.json").unwrap());
    serde_json::from_slice(&contents).unwrap()
}

struct UpdateCase {
    name: &'static BStr,
    before: serde_json::Value,
    command: Vec<(ParameterName, InputValue)>,
    expected: serde_json::Value,
}

#[test]
#[serial]
fn test_config_updates() {
    let cases: Vec<UpdateCase> = vec![
        UpdateCase {
            name: BStr::new("set scripts"),
            before: serde_json::json!({}),
            command: vec![
                key(BStr::new("scripts.test")),
                value(&[BStr::new("foo bar")]),
            ],
            expected: serde_json::json!({"scripts": {"test": "foo bar"}}),
        },
        UpdateCase {
            name: BStr::new("unset scripts"),
            before: serde_json::json!({"scripts": {"test": "foo bar", "lala": "baz"}}),
            command: vec![key(BStr::new("scripts.lala")), flag(BStr::new("--unset"))],
            expected: serde_json::json!({"scripts": {"test": "foo bar"}}),
        },
        UpdateCase {
            name: BStr::new("set single config with bool normalizer"),
            before: serde_json::json!({}),
            command: vec![key(BStr::new("use-github-api")), value(&[BStr::new("1")])],
            expected: serde_json::json!({"config": {"use-github-api": true}}),
        },
        UpdateCase {
            name: BStr::new("set multi config"),
            before: serde_json::json!({}),
            command: vec![
                key(BStr::new("github-protocols")),
                value(&[BStr::new("https"), BStr::new("git")]),
            ],
            expected: serde_json::json!({"config": {"github-protocols": ["https", "git"]}}),
        },
        UpdateCase {
            name: BStr::new("set version"),
            before: serde_json::json!({}),
            command: vec![key(BStr::new("version")), value(&[BStr::new("1.0.0")])],
            expected: serde_json::json!({"version": "1.0.0"}),
        },
        UpdateCase {
            name: BStr::new("unset version"),
            before: serde_json::json!({"version": "1.0.0"}),
            command: vec![key(BStr::new("version")), flag(BStr::new("--unset"))],
            expected: serde_json::json!({}),
        },
        UpdateCase {
            name: BStr::new("unset arbitrary property"),
            before: serde_json::json!({"random-prop": "1.0.0"}),
            command: vec![key(BStr::new("random-prop")), flag(BStr::new("--unset"))],
            expected: serde_json::json!({}),
        },
        UpdateCase {
            name: BStr::new("set preferred-install"),
            before: serde_json::json!({}),
            command: vec![
                key(BStr::new("preferred-install.foo/*")),
                value(&[BStr::new("source")]),
            ],
            expected: serde_json::json!({"config": {"preferred-install": {"foo/*": "source"}}}),
        },
        UpdateCase {
            name: BStr::new("unset preferred-install"),
            before: serde_json::json!({"config": {"preferred-install": {"foo/*": "source"}}}),
            command: vec![
                key(BStr::new("preferred-install.foo/*")),
                flag(BStr::new("--unset")),
            ],
            expected: serde_json::json!({"config": {"preferred-install": {}}}),
        },
        UpdateCase {
            name: BStr::new("unset platform"),
            before: serde_json::json!({"config": {"platform": {"php": "7.2.5"}, "platform-check": false}}),
            command: vec![key(BStr::new("platform.php")), flag(BStr::new("--unset"))],
            expected: serde_json::json!({"config": {"platform": {}, "platform-check": false}}),
        },
        UpdateCase {
            name: BStr::new("set extra with merge"),
            before: serde_json::json!({}),
            command: vec![
                key(BStr::new("extra.patches.foo/bar")),
                value(&[BStr::new("{\"123\":\"value\"}")]),
                flag(BStr::new("--json")),
                flag(BStr::new("--merge")),
            ],
            expected: serde_json::json!({"extra": {"patches": {"foo/bar": {"123": "value"}}}}),
        },
        UpdateCase {
            name: BStr::new("combine extra with merge"),
            before: serde_json::json!({"extra": {"patches": {"foo/bar": {"5": "oldvalue"}}}}),
            command: vec![
                key(BStr::new("extra.patches.foo/bar")),
                value(&[BStr::new("{\"123\":\"value\"}")]),
                flag(BStr::new("--json")),
                flag(BStr::new("--merge")),
            ],
            expected: serde_json::json!({"extra": {"patches": {"foo/bar": {"123": "value", "5": "oldvalue"}}}}),
        },
        UpdateCase {
            name: BStr::new("combine extra with list"),
            before: serde_json::json!({"extra": {"patches": {"foo/bar": ["oldvalue"]}}}),
            command: vec![
                key(BStr::new("extra.patches.foo/bar")),
                value(&[BStr::new("{\"123\":\"value\"}")]),
                flag(BStr::new("--json")),
                flag(BStr::new("--merge")),
            ],
            expected: serde_json::json!({"extra": {"patches": {"foo/bar": {"123": "value", "0": "oldvalue"}}}}),
        },
        UpdateCase {
            name: BStr::new("overwrite extra with merge"),
            before: serde_json::json!({"extra": {"patches": {"foo/bar": {"123": "oldvalue"}}}}),
            command: vec![
                key(BStr::new("extra.patches.foo/bar")),
                value(&[BStr::new("{\"123\":\"value\"}")]),
                flag(BStr::new("--json")),
                flag(BStr::new("--merge")),
            ],
            expected: serde_json::json!({"extra": {"patches": {"foo/bar": {"123": "value"}}}}),
        },
        UpdateCase {
            name: BStr::new("unset autoload"),
            before: serde_json::json!({"autoload": {"psr-4": ["test"], "classmap": ["test"]}}),
            command: vec![key(BStr::new("autoload.psr-4")), flag(BStr::new("--unset"))],
            expected: serde_json::json!({"autoload": {"classmap": ["test"]}}),
        },
        UpdateCase {
            name: BStr::new("unset autoload-dev"),
            before: serde_json::json!({"autoload-dev": {"psr-4": ["test"], "classmap": ["test"]}}),
            command: vec![
                key(BStr::new("autoload-dev.psr-4")),
                flag(BStr::new("--unset")),
            ],
            expected: serde_json::json!({"autoload-dev": {"classmap": ["test"]}}),
        },
        UpdateCase {
            name: BStr::new("set audit.ignore-unreachable"),
            before: serde_json::json!({}),
            command: vec![
                key(BStr::new("audit.ignore-unreachable")),
                value(&[BStr::new("true")]),
            ],
            expected: serde_json::json!({"config": {"audit": {"ignore-unreachable": true}}}),
        },
        UpdateCase {
            name: BStr::new("set audit.block-insecure"),
            before: serde_json::json!({}),
            command: vec![
                key(BStr::new("audit.block-insecure")),
                value(&[BStr::new("false")]),
            ],
            expected: serde_json::json!({"config": {"audit": {"block-insecure": false}}}),
        },
        UpdateCase {
            name: BStr::new("set audit.block-abandoned"),
            before: serde_json::json!({}),
            command: vec![
                key(BStr::new("audit.block-abandoned")),
                value(&[BStr::new("true")]),
            ],
            expected: serde_json::json!({"config": {"audit": {"block-abandoned": true}}}),
        },
        UpdateCase {
            name: BStr::new("unset audit.ignore-unreachable"),
            before: serde_json::json!({"config": {"audit": {"ignore-unreachable": true}}}),
            command: vec![
                key(BStr::new("audit.ignore-unreachable")),
                flag(BStr::new("--unset")),
            ],
            expected: serde_json::json!({"config": {"audit": {}}}),
        },
        UpdateCase {
            name: BStr::new("set audit.ignore-severity"),
            before: serde_json::json!({}),
            command: vec![
                key(BStr::new("audit.ignore-severity")),
                value(&[BStr::new("low"), BStr::new("medium")]),
            ],
            expected: serde_json::json!({"config": {"audit": {"ignore-severity": ["low", "medium"]}}}),
        },
        UpdateCase {
            name: BStr::new("set audit.ignore as array"),
            before: serde_json::json!({}),
            command: vec![
                key(BStr::new("audit.ignore")),
                value(&[BStr::new("[\"CVE-2024-1234\",\"GHSA-xxxx-yyyy\"]")]),
                flag(BStr::new("--json")),
            ],
            expected: serde_json::json!({"config": {"audit": {"ignore": ["CVE-2024-1234", "GHSA-xxxx-yyyy"]}}}),
        },
        UpdateCase {
            name: BStr::new("set audit.ignore as object"),
            before: serde_json::json!({}),
            command: vec![
                key(BStr::new("audit.ignore")),
                value(&[BStr::new(
                    "{\"CVE-2024-1234\":\"False positive\",\"GHSA-xxxx-yyyy\":\"Not applicable\"}",
                )]),
                flag(BStr::new("--json")),
            ],
            expected: serde_json::json!({"config": {"audit": {"ignore": {"CVE-2024-1234": "False positive", "GHSA-xxxx-yyyy": "Not applicable"}}}}),
        },
        UpdateCase {
            name: BStr::new("merge audit.ignore array"),
            before: serde_json::json!({"config": {"audit": {"ignore": ["CVE-2024-1234"]}}}),
            command: vec![
                key(BStr::new("audit.ignore")),
                value(&[BStr::new("[\"CVE-2024-5678\"]")]),
                flag(BStr::new("--json")),
                flag(BStr::new("--merge")),
            ],
            expected: serde_json::json!({"config": {"audit": {"ignore": ["CVE-2024-1234", "CVE-2024-5678"]}}}),
        },
        UpdateCase {
            name: BStr::new("merge audit.ignore object"),
            before: serde_json::json!({"config": {"audit": {"ignore": {"CVE-2024-1234": "Old reason"}}}}),
            command: vec![
                key(BStr::new("audit.ignore")),
                value(&[BStr::new("{\"CVE-2024-5678\":\"New advisory\"}")]),
                flag(BStr::new("--json")),
                flag(BStr::new("--merge")),
            ],
            expected: serde_json::json!({"config": {"audit": {"ignore": {"CVE-2024-5678": "New advisory", "CVE-2024-1234": "Old reason"}}}}),
        },
        UpdateCase {
            name: BStr::new("overwrite audit.ignore key with merge"),
            before: serde_json::json!({"config": {"audit": {"ignore": {"CVE-2024-1234": "Old reason"}}}}),
            command: vec![
                key(BStr::new("audit.ignore")),
                value(&[BStr::new("{\"CVE-2024-1234\":\"New reason\"}")]),
                flag(BStr::new("--json")),
                flag(BStr::new("--merge")),
            ],
            expected: serde_json::json!({"config": {"audit": {"ignore": {"CVE-2024-1234": "New reason"}}}}),
        },
        UpdateCase {
            name: BStr::new("set audit.ignore-abandoned as array"),
            before: serde_json::json!({}),
            command: vec![
                key(BStr::new("audit.ignore-abandoned")),
                value(&[BStr::new("[\"vendor/package1\",\"vendor/package2\"]")]),
                flag(BStr::new("--json")),
            ],
            expected: serde_json::json!({"config": {"audit": {"ignore-abandoned": ["vendor/package1", "vendor/package2"]}}}),
        },
        UpdateCase {
            name: BStr::new("set audit.ignore-abandoned as object"),
            before: serde_json::json!({}),
            command: vec![
                key(BStr::new("audit.ignore-abandoned")),
                value(&[BStr::new(
                    "{\"vendor/package1\":\"Still maintained\",\"vendor/package2\":\"Fork available\"}",
                )]),
                flag(BStr::new("--json")),
            ],
            expected: serde_json::json!({"config": {"audit": {"ignore-abandoned": {"vendor/package1": "Still maintained", "vendor/package2": "Fork available"}}}}),
        },
        UpdateCase {
            name: BStr::new("merge audit.ignore-abandoned array"),
            before: serde_json::json!({"config": {"audit": {"ignore-abandoned": ["vendor/package1"]}}}),
            command: vec![
                key(BStr::new("audit.ignore-abandoned")),
                value(&[BStr::new("[\"vendor/package2\"]")]),
                flag(BStr::new("--json")),
                flag(BStr::new("--merge")),
            ],
            expected: serde_json::json!({"config": {"audit": {"ignore-abandoned": ["vendor/package1", "vendor/package2"]}}}),
        },
        UpdateCase {
            name: BStr::new("merge audit.ignore-abandoned object"),
            before: serde_json::json!({"config": {"audit": {"ignore-abandoned": {"vendor/package1": "Old reason"}}}}),
            command: vec![
                key(BStr::new("audit.ignore-abandoned")),
                value(&[BStr::new("{\"vendor/package2\":\"New reason\"}")]),
                flag(BStr::new("--json")),
                flag(BStr::new("--merge")),
            ],
            expected: serde_json::json!({"config": {"audit": {"ignore-abandoned": {"vendor/package2": "New reason", "vendor/package1": "Old reason"}}}}),
        },
        UpdateCase {
            name: BStr::new("unset audit.ignore"),
            before: serde_json::json!({"config": {"audit": {"ignore": ["CVE-2024-1234"]}}}),
            command: vec![key(BStr::new("audit.ignore")), flag(BStr::new("--unset"))],
            expected: serde_json::json!({"config": {"audit": {}}}),
        },
        UpdateCase {
            name: BStr::new("unset audit.ignore-abandoned"),
            before: serde_json::json!({"config": {"audit": {"ignore-abandoned": ["vendor/package1"]}}}),
            command: vec![
                key(BStr::new("audit.ignore-abandoned")),
                flag(BStr::new("--unset")),
            ],
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
            app_tester.get_display().display_lossy()
        );

        assert_eq!(case.expected, read_composer_json(), "case {:?}", case.name);
    }
}

struct ReadCase {
    name: &'static BStr,
    composer_json: serde_json::Value,
    command: Vec<(ParameterName, InputValue)>,
    expected: &'static BStr,
}

#[test]
#[serial]
fn test_config_reads() {
    let cases: Vec<ReadCase> = vec![
        ReadCase {
            name: BStr::new("read description"),
            composer_json: serde_json::json!({"description": "foo bar"}),
            command: vec![key(BStr::new("description"))],
            expected: BStr::new("foo bar"),
        },
        ReadCase {
            name: BStr::new("read vendor-dir with source"),
            composer_json: serde_json::json!({"config": {"vendor-dir": "lala"}}),
            command: vec![key(BStr::new("vendor-dir")), flag(BStr::new("--source"))],
            expected: BStr::new("lala (./composer.json)"),
        },
        ReadCase {
            name: BStr::new("read default vendor-dir"),
            composer_json: serde_json::json!({}),
            command: vec![key(BStr::new("vendor-dir"))],
            expected: BStr::new("vendor"),
        },
        ReadCase {
            name: BStr::new("read repos by named key"),
            composer_json: serde_json::json!({"repositories": {"foo": {"type": "vcs", "url": "https://example.org"}, "packagist.org": {"type": "composer", "url": "https://repo.packagist.org"}}}),
            command: vec![key(BStr::new("repositories.foo"))],
            expected: BStr::new("{\"type\":\"vcs\",\"url\":\"https://example.org\"}"),
        },
        ReadCase {
            name: BStr::new("read all repos includes the default packagist"),
            composer_json: serde_json::json!({"repositories": {"foo": {"type": "vcs", "url": "https://example.org"}, "packagist.org": {"type": "composer", "url": "https://repo.packagist.org"}}}),
            command: vec![key(BStr::new("repos"))],
            expected: BStr::new(
                "{\"foo\":{\"type\":\"vcs\",\"url\":\"https://example.org\"},\"packagist.org\":{\"type\":\"composer\",\"url\":\"https://repo.packagist.org\"}}",
            ),
        },
        ReadCase {
            name: BStr::new("read all repos does not include the disabled packagist"),
            composer_json: serde_json::json!({"repositories": {"foo": {"type": "vcs", "url": "https://example.org"}, "packagist.org": false}}),
            command: vec![key(BStr::new("repos"))],
            expected: BStr::new("{\"foo\":{\"type\":\"vcs\",\"url\":\"https://example.org\"}}"),
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
            trim(app_tester.get_display()),
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
fn test_config_reads_repos_by_numeric_index() {
    let composer_json = serde_json::json!({"repositories": [{"type": "vcs", "url": "https://example.org"}, {"type": "composer", "url": "https://repo.packagist.org"}]});
    let _tear_down = init_temp_composer(Some(&composer_json), None, None, false);

    let mut app_tester = get_application_tester();
    app_tester
        .run(
            config_input(vec![key(BStr::new("repos.0"))]),
            RunOptions::default(),
        )
        .unwrap();

    assert_eq!(0, app_tester.get_status_code());
    assert_eq!(
        "{\"type\":\"vcs\",\"url\":\"https://example.org\"}",
        trim(app_tester.get_display())
    );
}

#[test]
#[serial]
fn test_config_throws_for_invalid_arg_combination() {
    let mut app_tester = get_application_tester();
    let result = app_tester.run(
        config_input(vec![
            (
                ParameterName::of("--file"),
                InputValue::from("alt.composer.json"),
            ),
            flag(BStr::new("--global")),
        ]),
        RunOptions::default(),
    );

    let err = result.expect_err("expected RuntimeException");
    assert!(
        error_message(&err).contains_str("--file and --global can not be combined"),
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
            key(BStr::new("audit.ignore-severity")),
            value(&[BStr::new("low"), BStr::new("invalid")]),
        ]),
        RunOptions::default(),
    );

    let err = result.expect_err("expected RuntimeException");
    assert!(
        error_message(&err).contains_str("valid severities include: low, medium, high, critical"),
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
            key(BStr::new("audit.ignore")),
            value(&[BStr::new("{\"CVE-2024-5678\":\"reason\"}")]),
            flag(BStr::new("--json")),
            flag(BStr::new("--merge")),
        ]),
        RunOptions::default(),
    );

    let err = result.expect_err("expected RuntimeException");
    assert!(
        error_message(&err).contains_str("Cannot merge array and object"),
        "got: {:?}",
        err
    );
}
