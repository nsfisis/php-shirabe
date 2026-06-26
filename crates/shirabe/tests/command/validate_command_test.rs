//! ref: composer/tests/Composer/Test/Command/ValidateCommandTest.php

use crate::test_case::{
    RunOptions, create_composer_lock, get_application_tester, init_temp_composer,
};
use serial_test::serial;
use shirabe::util::platform::Platform;
use shirabe_php_shim::PhpMixed;

/// ref: ValidateCommandTest::MINIMAL_VALID_CONFIGURATION
fn minimal_valid_configuration() -> serde_json::Value {
    serde_json::json!({
        "name": "test/suite",
        "type": "library",
        "description": "A generical test suite",
        "license": "MIT",
        "repositories": {
            "packages": {
                "type": "package",
                "package": [
                    {"name": "root/req", "version": "1.0.0", "require": {"dep/pkg": "^1"}},
                    {"name": "dep/pkg", "version": "1.0.0"},
                    {"name": "dep/pkg", "version": "1.0.1"},
                    {"name": "dep/pkg", "version": "1.0.2"}
                ]
            }
        },
        "require": {
            "root/req": "1.*"
        }
    })
}

fn validate_input(command: Vec<(PhpMixed, PhpMixed)>) -> Vec<(PhpMixed, PhpMixed)> {
    let mut input = vec![(PhpMixed::from("command"), PhpMixed::from("validate"))];
    input.extend(command);
    input
}

struct ValidateCase {
    name: &'static str,
    composer_json: serde_json::Value,
    command: Vec<(PhpMixed, PhpMixed)>,
    expected: &'static str,
}

/// ref: provideValidateTests
fn provide_validate_tests() -> Vec<ValidateCase> {
    // $publishDataStripped = array_diff_key(MINIMAL_VALID_CONFIGURATION, ['name','type','description','license'])
    let publish_data_stripped = serde_json::json!({
        "repositories": {
            "packages": {
                "type": "package",
                "package": [
                    {"name": "root/req", "version": "1.0.0", "require": {"dep/pkg": "^1"}},
                    {"name": "dep/pkg", "version": "1.0.0"},
                    {"name": "dep/pkg", "version": "1.0.1"},
                    {"name": "dep/pkg", "version": "1.0.2"}
                ]
            }
        },
        "require": {
            "root/req": "1.*"
        }
    });

    vec![
        ValidateCase {
            name: "validation passing",
            composer_json: minimal_valid_configuration(),
            command: vec![],
            expected: "<warning>Composer could not detect the root package (test/suite) version, defaulting to '1.0.0'. See https://getcomposer.org/root-version</warning>\n<warning>Composer could not detect the root package (test/suite) version, defaulting to '1.0.0'. See https://getcomposer.org/root-version</warning>\n./composer.json is valid",
        },
        ValidateCase {
            // WORDING NOTE: upstream asserts justinrainbow's property-prefixed strings
            // "- name : The property name is required" / "- description : The property description
            // is required". The jsonschema crate phrases the message differently, but the property
            // prefix is reconstructed, so the shape matches:
            // "- name : \"name\" is a required property". Matches the divergence already accepted in
            // json_file_test.
            name: "passing but with warnings",
            composer_json: publish_data_stripped.clone(),
            command: vec![],
            expected: "./composer.json is valid for simple usage with Composer but has\nstrict errors that make it unable to be published as a package\n<warning>See https://getcomposer.org/doc/04-schema.md for details on the schema</warning>\n# Publish errors\n- name : \"name\" is a required property\n- description : \"description\" is a required property\n<warning># General warnings</warning>\n- No license specified, it is recommended to do so. For closed-source software you may use \"proprietary\" as license.",
        },
        ValidateCase {
            name: "passing without publish-check",
            composer_json: publish_data_stripped,
            command: vec![(PhpMixed::from("--no-check-publish"), PhpMixed::Bool(true))],
            expected: "./composer.json is valid, but with a few warnings\n<warning>See https://getcomposer.org/doc/04-schema.md for details on the schema</warning>\n<warning># General warnings</warning>\n- No license specified, it is recommended to do so. For closed-source software you may use \"proprietary\" as license.",
        },
    ]
}

#[test]
#[serial]
fn test_validate() {
    for case in provide_validate_tests() {
        let _tear_down = init_temp_composer(Some(&case.composer_json), None, None, true);

        let mut app_tester = get_application_tester();
        app_tester
            .run(validate_input(case.command), RunOptions::default())
            .unwrap_or_else(|e| panic!("case {:?}: run failed: {:?}", case.name, e));

        assert_eq!(
            case.expected.trim(),
            app_tester.get_display().trim(),
            "case {:?}",
            case.name
        );
    }
}

#[test]
#[serial]
fn test_validate_on_file_issues() {
    let tear_down = init_temp_composer(Some(&minimal_valid_configuration()), None, None, true);
    std::fs::remove_file(tear_down.working_dir().join("composer.json")).unwrap();

    let mut app_tester = get_application_tester();
    app_tester
        .run(validate_input(vec![]), RunOptions::default())
        .unwrap();

    assert_eq!(
        "./composer.json not found.",
        app_tester.get_display().trim()
    );

    drop(tear_down);
}

#[test]
#[serial]
fn test_with_composer_lock() {
    let tear_down = init_temp_composer(Some(&minimal_valid_configuration()), None, None, true);
    create_composer_lock(&[], &[]);

    let mut app_tester = get_application_tester();
    app_tester
        .run(validate_input(vec![]), RunOptions::default())
        .unwrap();

    let expected = "<warning>Composer could not detect the root package (test/suite) version, defaulting to '1.0.0'. See https://getcomposer.org/root-version</warning>\n<warning>Composer could not detect the root package (test/suite) version, defaulting to '1.0.0'. See https://getcomposer.org/root-version</warning>\n./composer.json is valid but your composer.lock has some errors\n# Lock file errors\n- Required package \"root/req\" is not present in the lock file.\nThis usually happens when composer files are incorrectly merged or the composer.json file is manually edited.\nRead more about correctly resolving merge conflicts https://getcomposer.org/doc/articles/resolving-merge-conflicts.md\nand prefer using the \"require\" command over editing the composer.json file directly https://getcomposer.org/doc/03-cli.md#require-r";

    assert_eq!(expected.trim(), app_tester.get_display().trim());

    drop(tear_down);
}

#[test]
#[serial]
fn test_unaccessible_file() {
    if Platform::is_windows() {
        // ref: $this->markTestSkipped('Does not run on windows');
        return;
    }
    if shirabe_php_shim::function_exists("posix_getuid") && shirabe_php_shim::posix_getuid() == 0 {
        // ref: $this->markTestSkipped('Cannot run as root');
        return;
    }

    let tear_down = init_temp_composer(Some(&minimal_valid_configuration()), None, None, true);
    let composer_json = tear_down.working_dir().join("composer.json");
    shirabe_php_shim::chmod(&composer_json.to_string_lossy(), 0o200);

    let mut app_tester = get_application_tester();
    app_tester
        .run(validate_input(vec![]), RunOptions::default())
        .unwrap();

    assert_eq!(
        "./composer.json is not readable.",
        app_tester.get_display().trim()
    );
    assert_eq!(3, app_tester.get_status_code());

    shirabe_php_shim::chmod(&composer_json.to_string_lossy(), 0o700);
    drop(tear_down);
}
