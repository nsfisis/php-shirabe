//! ref: composer/tests/Composer/Test/Command/SearchCommandTest.php sha1:94362ef761078fdebc57ef6dfaaf2f902d3231b8

use crate::test_case::{RunOptions, get_application_tester, init_temp_composer};
use bstr::{BStr, ByteSlice as _};
use shirabe_php_shim::{error_message, trim};
use shirabe_symfony_console::input::{InputValue, ParameterName};

fn repositories_json() -> serde_json::Value {
    serde_json::json!({
        "repositories": [
            { "packagist.org": false },
            {
                "type": "package",
                "package": [
                    { "name": "vendor-1/package-1", "description": "generic description", "version": "1.0.0" },
                    { "name": "foo/bar", "description": "generic description", "version": "1.0.0" },
                    { "name": "bar/baz", "description": "fancy baz", "version": "1.0.0", "abandoned": true },
                    { "name": "vendor-2/fancy-package", "fancy description": null, "version": "1.0.0", "type": "foo" },
                ],
            },
        ],
    })
}

/// ref: SearchCommandTest::testSearch (data provider rolled into one body).
fn run_search_case(command: Vec<(ParameterName, InputValue)>, expected: &BStr) {
    let _tear_down = init_temp_composer(Some(&repositories_json()), None, None, true);

    let mut input: Vec<(ParameterName, InputValue)> =
        vec![(ParameterName::of("command"), InputValue::from("search"))];
    input.extend(command);

    let mut app_tester = get_application_tester();
    app_tester.run(input, RunOptions::default()).unwrap();
    assert_eq!(trim(expected), trim(app_tester.get_display()));
}

#[test]
#[serial_test::serial]
fn test_search() {
    // 'by name and description'
    run_search_case(
        vec![(
            ParameterName::of("tokens"),
            InputValue::Array(vec!["fancy".into()]),
        )],
        BStr::new(
            "bar/baz                <warning>! Abandoned !</warning> fancy baz\nvendor-2/fancy-package",
        ),
    );

    // 'by name and description with multiple tokens'
    run_search_case(
        vec![(
            ParameterName::of("tokens"),
            InputValue::Array(vec!["fancy".into(), "vendor".into()]),
        )],
        BStr::new(
            "vendor-1/package-1     generic description\nbar/baz                <warning>! Abandoned !</warning> fancy baz\nvendor-2/fancy-package",
        ),
    );

    // 'by name only'
    run_search_case(
        vec![
            (
                ParameterName::of("tokens"),
                InputValue::Array(vec!["fancy".into()]),
            ),
            (ParameterName::of("--only-name"), InputValue::from(true)),
        ],
        BStr::new("vendor-2/fancy-package"),
    );

    // 'by vendor only'
    run_search_case(
        vec![
            (
                ParameterName::of("tokens"),
                InputValue::Array(vec!["bar".into()]),
            ),
            (ParameterName::of("--only-vendor"), InputValue::from(true)),
        ],
        BStr::new("bar"),
    );

    // 'by type'
    run_search_case(
        vec![
            (
                ParameterName::of("tokens"),
                InputValue::Array(vec!["vendor".into()]),
            ),
            (ParameterName::of("--type"), InputValue::from("foo")),
        ],
        BStr::new("vendor-2/fancy-package"),
    );

    // 'json format'
    run_search_case(
        vec![
            (
                ParameterName::of("tokens"),
                InputValue::Array(vec!["vendor-2/fancy".into()]),
            ),
            (ParameterName::of("--format"), InputValue::from("json")),
        ],
        BStr::new(
            "[\n    {\n        \"name\": \"vendor-2/fancy-package\",\n        \"description\": null\n    }\n]",
        ),
    );

    // 'no results'
    run_search_case(
        vec![(
            ParameterName::of("tokens"),
            InputValue::Array(vec!["invalid-package-name".into()]),
        )],
        BStr::new(""),
    );
}

#[test]
#[serial_test::serial]
fn test_invalid_format() {
    let _tear_down = init_temp_composer(
        Some(&serde_json::json!({ "repositories": { "packagist.org": false } })),
        None,
        None,
        true,
    );

    let mut app_tester = get_application_tester();
    let result = app_tester
        .run(
            vec![
                (ParameterName::of("command"), InputValue::from("search")),
                (
                    ParameterName::of("--format"),
                    InputValue::from("test-format"),
                ),
                (
                    ParameterName::of("tokens"),
                    InputValue::Array(vec!["test".into()]),
                ),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(1, result);
    assert_eq!(
        "Unsupported format \"test-format\". See help for supported formats.",
        trim(app_tester.get_display())
    );
}

#[test]
#[serial_test::serial]
fn test_invalid_flags() {
    let _tear_down = init_temp_composer(
        Some(&serde_json::json!({ "repositories": { "packagist.org": false } })),
        None,
        None,
        true,
    );

    let mut app_tester = get_application_tester();
    let err = app_tester
        .run(
            vec![
                (ParameterName::of("command"), InputValue::from("search")),
                (ParameterName::of("--only-vendor"), InputValue::from(true)),
                (ParameterName::of("--only-name"), InputValue::from(true)),
                (
                    ParameterName::of("tokens"),
                    InputValue::Array(vec!["test".into()]),
                ),
            ],
            RunOptions::default(),
        )
        .expect_err("expected InvalidArgumentException");
    assert!(
        error_message(&err).contains_str("--only-name and --only-vendor cannot be used together"),
        "got: {:?}",
        err
    );
}
