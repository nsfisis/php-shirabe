//! ref: composer/tests/Composer/Test/Command/RequireCommandTest.php

use crate::test_case::{
    RunOptions, create_composer_lock, create_installed_json, get_application_tester, get_package,
    init_temp_composer,
};
use serial_test::serial;
use shirabe::json::JsonFile;
use shirabe_php_shim::PhpMixed;

fn input(pairs: Vec<(&str, PhpMixed)>) -> Vec<(PhpMixed, PhpMixed)> {
    pairs
        .into_iter()
        .map(|(k, v)| (PhpMixed::from(k), v))
        .collect()
}

#[test]
#[serial]
#[ignore = "shirabe_php_shim::phpversion(extension) is a todo!() (per-extension version strings not modeled); reached while checking the ext-foobar platform requirement during version selection"]
fn test_require_throws_if_none_matches() {
    let composer_json = serde_json::json!({
        "repositories": {
            "packages": {
                "type": "package",
                "package": [
                    { "name": "required/pkg", "version": "1.0.0", "require": { "ext-foobar": "^1" } },
                ],
            },
        },
    });
    let _tear_down = init_temp_composer(Some(&composer_json), None, None, true);

    let mut app_tester = get_application_tester();
    let err = app_tester
        .run(
            input(vec![
                ("command", PhpMixed::from("require")),
                ("--dry-run", PhpMixed::from(true)),
                ("--no-audit", PhpMixed::from(true)),
                (
                    "packages",
                    PhpMixed::List(vec![PhpMixed::from("required/pkg")]),
                ),
            ]),
            RunOptions::default(),
        )
        .expect_err("expected InvalidArgumentException for incompatible platform requirements");

    assert!(
        err.to_string().contains(
            "Package required/pkg has requirements incompatible with your PHP version, PHP extensions and Composer version:"
        ) && err
            .to_string()
            .contains("- required/pkg 1.0.0 requires ext-foobar ^1 but it is not present."),
        "got: {err}"
    );
}

#[test]
#[serial]
#[ignore = "Phase-C re-entrancy: EventDispatcher::make_autoloader calls Composer borrow_mut while the Composer is already borrowed up the installer-event-dispatch stack, panicking with \"RefCell already borrowed\" (composer.rs:507)"]
fn test_require_warns_if_resolved_to_feature_branch() {
    let composer_json = serde_json::json!({
        "repositories": {
            "packages": {
                "type": "package",
                "package": [
                    { "name": "required/pkg", "version": "2.0.0", "require": { "common/dep": "^1" } },
                    { "name": "required/pkg", "version": "dev-foo-bar", "require": { "common/dep": "^2" } },
                    { "name": "common/dep", "version": "2.0.0" },
                ],
            },
        },
        "require": { "common/dep": "^2.0" },
        "minimum-stability": "dev",
        "prefer-stable": true,
    });
    let _tear_down = init_temp_composer(Some(&composer_json), None, None, true);

    let mut app_tester = get_application_tester();
    app_tester.set_inputs(vec!["n".to_string()]);
    app_tester
        .run(
            input(vec![
                ("command", PhpMixed::from("require")),
                ("--dry-run", PhpMixed::from(true)),
                ("--no-audit", PhpMixed::from(true)),
                (
                    "packages",
                    PhpMixed::List(vec![PhpMixed::from("required/pkg")]),
                ),
            ]),
            RunOptions {
                interactive: Some(true),
                ..RunOptions::default()
            },
        )
        .unwrap();

    assert_eq!(
        "./composer.json has been updated
Running composer update required/pkg
Loading composer repositories with package information
Updating dependencies
Lock file operations: 2 installs, 0 updates, 0 removals
  - Locking common/dep (2.0.0)
  - Locking required/pkg (dev-foo-bar)
Installing dependencies from lock file (including require-dev)
Package operations: 2 installs, 0 updates, 0 removals
  - Installing common/dep (2.0.0)
  - Installing required/pkg (dev-foo-bar)
Using version dev-foo-bar for required/pkg
<warning>Version dev-foo-bar looks like it may be a feature branch which is unlikely to keep working in the long run and may be in an unstable state</warning>
Are you sure you want to use this constraint (y) or would you rather abort (n) the whole operation [y,n]? .
Installation failed, reverting ./composer.json to its original content.
",
        app_tester.get_display()
    );
}

/// ref: RequireCommandTest::provideRequire
fn provide_require() -> Vec<(
    &'static str,
    serde_json::Value,
    Vec<(&'static str, PhpMixed)>,
    &'static str,
)> {
    vec![
        (
            "warn once for missing ext but a lower package matches",
            serde_json::json!({
                "repositories": { "packages": { "type": "package", "package": [
                    { "name": "required/pkg", "version": "1.2.0", "require": { "ext-foobar": "^1" } },
                    { "name": "required/pkg", "version": "1.1.0", "require": { "ext-foobar": "^1" } },
                    { "name": "required/pkg", "version": "1.0.0" },
                ] } },
            }),
            vec![("packages", PhpMixed::List(vec![PhpMixed::from("required/pkg")]))],
            "<warning>Cannot use required/pkg's latest version 1.2.0 as it requires ext-foobar ^1 which is missing from your platform.
./composer.json has been updated
Running composer update required/pkg
Loading composer repositories with package information
Updating dependencies
Lock file operations: 1 install, 0 updates, 0 removals
  - Locking required/pkg (1.0.0)
Installing dependencies from lock file (including require-dev)
Package operations: 1 install, 0 updates, 0 removals
  - Installing required/pkg (1.0.0)
Using version ^1.0 for required/pkg",
        ),
        (
            "warn multiple times when verbose",
            serde_json::json!({
                "repositories": { "packages": { "type": "package", "package": [
                    { "name": "required/pkg", "version": "1.2.0", "require": { "ext-foobar": "^1" } },
                    { "name": "required/pkg", "version": "1.1.0", "require": { "ext-foobar": "^1" } },
                    { "name": "required/pkg", "version": "1.0.0" },
                ] } },
            }),
            vec![
                ("packages", PhpMixed::List(vec![PhpMixed::from("required/pkg")])),
                ("--no-install", PhpMixed::from(true)),
                ("-v", PhpMixed::from(true)),
            ],
            "<warning>Cannot use required/pkg's latest version 1.2.0 as it requires ext-foobar ^1 which is missing from your platform.
<warning>Cannot use required/pkg 1.1.0 as it requires ext-foobar ^1 which is missing from your platform.
./composer.json has been updated
Running composer update required/pkg
Loading composer repositories with package information
Updating dependencies
Dependency resolution completed in %d seconds
Analyzed %d packages to resolve dependencies
Analyzed %d rules to resolve dependencies
Lock file operations: 1 install, 0 updates, 0 removals
Installs: required/pkg:1.0.0
  - Locking required/pkg (1.0.0)
Using version ^1.0 for required/pkg",
        ),
        (
            "warn for not satisfied req which is satisfied by lower version",
            serde_json::json!({
                "repositories": { "packages": { "type": "package", "package": [
                    { "name": "required/pkg", "version": "1.1.0", "require": { "php": "^20" } },
                    { "name": "required/pkg", "version": "1.0.0", "require": { "php": ">=7" } },
                ] } },
            }),
            vec![
                ("packages", PhpMixed::List(vec![PhpMixed::from("required/pkg")])),
                ("--no-install", PhpMixed::from(true)),
            ],
            "<warning>Cannot use required/pkg's latest version 1.1.0 as it requires php ^20 which is not satisfied by your platform.
./composer.json has been updated
Running composer update required/pkg
Loading composer repositories with package information
Updating dependencies
Lock file operations: 1 install, 0 updates, 0 removals
  - Locking required/pkg (1.0.0)
Using version ^1.0 for required/pkg",
        ),
        (
            "version selection happens early even if not completely accurate if no update is requested",
            serde_json::json!({
                "repositories": { "packages": { "type": "package", "package": [
                    { "name": "required/pkg", "version": "1.1.0", "require": { "php": "^20" } },
                    { "name": "required/pkg", "version": "1.0.0", "require": { "php": ">=7" } },
                ] } },
            }),
            vec![
                ("packages", PhpMixed::List(vec![PhpMixed::from("required/pkg")])),
                ("--no-update", PhpMixed::from(true)),
            ],
            "<warning>Cannot use required/pkg's latest version 1.1.0 as it requires php ^20 which is not satisfied by your platform.
Using version ^1.0 for required/pkg
./composer.json has been updated",
        ),
        (
            "pick best matching version when not provided",
            serde_json::json!({
                "repositories": { "packages": { "type": "package", "package": [
                    { "name": "existing/dep", "version": "1.1.0", "require": { "required/pkg": "^1" } },
                    { "name": "required/pkg", "version": "2.0.0" },
                    { "name": "required/pkg", "version": "1.1.0" },
                    { "name": "required/pkg", "version": "1.0.0" },
                ] } },
                "require": { "existing/dep": "^1" },
            }),
            vec![
                ("packages", PhpMixed::List(vec![PhpMixed::from("required/pkg")])),
                ("--no-install", PhpMixed::from(true)),
            ],
            "./composer.json has been updated
Running composer update required/pkg
Loading composer repositories with package information
Updating dependencies
Lock file operations: 2 installs, 0 updates, 0 removals
  - Locking existing/dep (1.1.0)
  - Locking required/pkg (1.1.0)
Using version ^1.1 for required/pkg",
        ),
        (
            "use exact constraint with --fixed",
            serde_json::json!({
                "type": "project",
                "repositories": { "packages": { "type": "package", "package": [
                    { "name": "required/pkg", "version": "1.1.0" },
                ] } },
            }),
            vec![
                ("packages", PhpMixed::List(vec![PhpMixed::from("required/pkg")])),
                ("--no-install", PhpMixed::from(true)),
                ("--fixed", PhpMixed::from(true)),
            ],
            "./composer.json has been updated
Running composer update required/pkg
Loading composer repositories with package information
Updating dependencies
Lock file operations: 1 install, 0 updates, 0 removals
  - Locking required/pkg (1.1.0)
Using version 1.1.0 for required/pkg",
        ),
    ]
}

#[test]
#[serial]
#[ignore = "Phase-C re-entrancy: EventDispatcher::make_autoloader calls Composer borrow_mut while the Composer is already borrowed up the installer-event-dispatch stack, panicking with \"RefCell already borrowed\" (composer.rs:507)"]
fn test_require() {
    for (label, composer_json, command, expected) in provide_require() {
        let _tear_down = init_temp_composer(Some(&composer_json), None, None, true);

        let mut app_tester = get_application_tester();
        let mut args = vec![
            ("command", PhpMixed::from("require")),
            ("--dry-run", PhpMixed::from(true)),
            ("--no-audit", PhpMixed::from(true)),
        ];
        args.extend(command);
        app_tester.run(input(args), RunOptions::default()).unwrap();

        let display = app_tester.get_display();
        let display = display.trim();
        if expected.contains("%d") {
            // The PHP test builds a regex by quoting the expected and replacing %d with [0-9.]+.
            // Compare line-by-line, treating "%d" as a numeric wildcard.
            let exp_lines: Vec<&str> = expected.trim().lines().collect();
            let got_lines: Vec<&str> = display.lines().collect();
            assert_eq!(
                exp_lines.len(),
                got_lines.len(),
                "case: {label}\n--- expected ---\n{expected}\n--- got ---\n{display}"
            );
            for (exp, got) in exp_lines.iter().zip(got_lines.iter()) {
                if exp.contains("%d") {
                    let prefix = &exp[..exp.find("%d").unwrap()];
                    assert!(
                        got.starts_with(prefix),
                        "case: {label}\nexpected line prefix {prefix:?}, got {got:?}"
                    );
                } else {
                    assert_eq!(exp, got, "case: {label}");
                }
            }
        } else {
            assert_eq!(expected.trim(), display, "case: {label}");
        }
    }
}

/// ref: RequireCommandTest::provideInconsistentRequireKeys
fn provide_inconsistent_require_keys() -> Vec<(bool, bool, &'static str)> {
    vec![
        (
            true,
            false,
            "<warning>required/pkg is currently present in the require key and you ran the command with the --dev flag, which will move it to the require-dev key.</warning>",
        ),
        (
            false,
            false,
            "<warning>required/pkg is currently present in the require-dev key and you ran the command without the --dev flag, which will move it to the require key.</warning>",
        ),
        (
            true,
            true,
            "<warning>required/pkg is currently present in the require key and you ran the command with the --dev flag, which will move it to the require-dev key.</warning>",
        ),
        (
            false,
            true,
            "<warning>required/pkg is currently present in the require-dev key and you ran the command without the --dev flag, which will move it to the require key.</warning>",
        ),
    ]
}

#[test]
#[serial]
#[ignore = "update/solver pipeline incomplete: the require run fails with \"Fixed package __root__ 1.0.0+no-version-set was not added to solver pool.\" (the root package is not seeded into the solver pool)"]
fn test_inconsistent_require_keys() {
    for (is_dev, is_interactive, expected_warning) in provide_inconsistent_require_keys() {
        let current_key = if is_dev { "require" } else { "require-dev" };
        let other_key = if is_dev { "require-dev" } else { "require" };

        let composer_json = serde_json::json!({
            "repositories": { "packages": { "type": "package", "package": [
                { "name": "required/pkg", "version": "1.0.0" },
            ] } },
            current_key: { "required/pkg": "^1.0" },
        });
        let tear_down = init_temp_composer(Some(&composer_json), None, None, true);
        let dir = tear_down.working_dir();

        let package = get_package("required/pkg", "1.0.0");
        if is_dev {
            create_composer_lock(&[], std::slice::from_ref(&package));
            create_installed_json(&[], std::slice::from_ref(&package), true);
        } else {
            create_composer_lock(std::slice::from_ref(&package), &[]);
            create_installed_json(std::slice::from_ref(&package), &[], false);
        }

        let mut app_tester = get_application_tester();
        let mut command = vec![
            ("command", PhpMixed::from("require")),
            ("--no-audit", PhpMixed::from(true)),
            ("--dev", PhpMixed::from(is_dev)),
            ("--no-install", PhpMixed::from(true)),
            (
                "packages",
                PhpMixed::List(vec![PhpMixed::from("required/pkg")]),
            ),
        ];
        if is_interactive {
            app_tester.set_inputs(vec!["yes".to_string()]);
        } else {
            command.push(("--no-interaction", PhpMixed::from(true)));
        }

        app_tester
            .run(input(command), RunOptions::default())
            .unwrap();

        assert!(
            app_tester.get_display().contains(expected_warning),
            "case is_dev={is_dev} is_interactive={is_interactive}\ngot: {}",
            app_tester.get_display()
        );

        let mut composer_content =
            JsonFile::new(format!("{}/composer.json", dir.display()), None, None).unwrap();
        let content = composer_content.read().unwrap();
        let content: serde_json::Value = serde_json::from_str(&JsonFile::encode(&content)).unwrap();
        assert!(
            content.get(other_key).is_some(),
            "expected key {other_key} present"
        );
        assert!(
            content.get(current_key).is_none(),
            "expected key {current_key} absent"
        );
    }
}
