//! ref: composer/tests/Composer/Test/Command/SearchCommandTest.php

use crate::test_case::{RunOptions, get_application_tester, init_temp_composer};
use serial_test::serial;
use shirabe_symfony_console::input::InputValue;
use shirabe_symfony_console::input::ParameterName;

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
fn run_search_case(command: Vec<(ParameterName, InputValue)>, expected: &str) {
    let _tear_down = init_temp_composer(Some(&repositories_json()), None, None, true);

    let mut input: Vec<(ParameterName, InputValue)> =
        vec![(ParameterName::of("command"), InputValue::from("search"))];
    input.extend(command);

    let mut app_tester = get_application_tester();
    app_tester.run(input, RunOptions::default()).unwrap();
    assert_eq!(expected.trim(), app_tester.get_display().trim());
}

#[test]
#[serial]
fn test_search() {
    // 'by name and description'
    run_search_case(
        vec![(
            ParameterName::of("tokens"),
            InputValue::Array(vec!["fancy".to_string()]),
        )],
        "bar/baz                <warning>! Abandoned !</warning> fancy baz\nvendor-2/fancy-package",
    );

    // 'by name and description with multiple tokens'
    run_search_case(
        vec![(
            ParameterName::of("tokens"),
            InputValue::Array(vec!["fancy".to_string(), "vendor".to_string()]),
        )],
        "vendor-1/package-1     generic description\nbar/baz                <warning>! Abandoned !</warning> fancy baz\nvendor-2/fancy-package",
    );

    // 'by name only'
    run_search_case(
        vec![
            (
                ParameterName::of("tokens"),
                InputValue::Array(vec!["fancy".to_string()]),
            ),
            (ParameterName::of("--only-name"), InputValue::from(true)),
        ],
        "vendor-2/fancy-package",
    );

    // 'by vendor only'
    run_search_case(
        vec![
            (
                ParameterName::of("tokens"),
                InputValue::Array(vec!["bar".to_string()]),
            ),
            (ParameterName::of("--only-vendor"), InputValue::from(true)),
        ],
        "bar",
    );

    // 'by type'
    run_search_case(
        vec![
            (
                ParameterName::of("tokens"),
                InputValue::Array(vec!["vendor".to_string()]),
            ),
            (ParameterName::of("--type"), InputValue::from("foo")),
        ],
        "vendor-2/fancy-package",
    );

    // 'json format'
    run_search_case(
        vec![
            (
                ParameterName::of("tokens"),
                InputValue::Array(vec!["vendor-2/fancy".to_string()]),
            ),
            (ParameterName::of("--format"), InputValue::from("json")),
        ],
        "[\n    {\n        \"name\": \"vendor-2/fancy-package\",\n        \"description\": null\n    }\n]",
    );

    // 'no results'
    run_search_case(
        vec![(
            ParameterName::of("tokens"),
            InputValue::Array(vec!["invalid-package-name".to_string()]),
        )],
        "",
    );
}

#[test]
#[serial]
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
                    InputValue::Array(vec!["test".to_string()]),
                ),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(1, result);
    assert_eq!(
        "Unsupported format \"test-format\". See help for supported formats.",
        app_tester.get_display().trim()
    );
}

#[test]
#[serial]
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
                    InputValue::Array(vec!["test".to_string()]),
                ),
            ],
            RunOptions::default(),
        )
        .expect_err("expected InvalidArgumentException");
    assert!(
        err.to_string()
            .contains("--only-name and --only-vendor cannot be used together"),
        "got: {:?}",
        err
    );
}
