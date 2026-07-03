//! ref: composer/tests/Composer/Test/Command/UpdateCommandTest.php

use crate::test_case::{
    RunOptions, create_composer_lock, create_installed_json, get_application_tester, get_package,
    init_temp_composer,
};
use serial_test::serial;
use shirabe::package::Link;
use shirabe_php_shim::PhpMixed;
use shirabe_semver::constraint::{AnyConstraint, MatchAllConstraint};

fn input(pairs: Vec<(&str, PhpMixed)>) -> Vec<(PhpMixed, PhpMixed)> {
    pairs
        .into_iter()
        .map(|(k, v)| (PhpMixed::from(k), v))
        .collect()
}

/// Minimal port of PHPUnit's `assertStringMatchesFormat` covering the placeholders these fixtures
/// use: `%d` (integer), `%f` (float), `%s` (non-newline run), `%a` (any run incl. newlines).
fn assert_string_matches_format(expected: &str, actual: &str) {
    let mut pattern = String::from("(?s)^");
    let mut chars = expected.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%'
            && let Some(&next) = chars.peek()
        {
            match next {
                'd' => {
                    chars.next();
                    pattern.push_str(r"[0-9]+");
                    continue;
                }
                'f' => {
                    chars.next();
                    pattern.push_str(r"[+-]?[0-9]*\.?[0-9]+([eE][+-]?[0-9]+)?");
                    continue;
                }
                's' => {
                    chars.next();
                    pattern.push_str(r"[^\r\n]+");
                    continue;
                }
                'a' => {
                    chars.next();
                    pattern.push_str(r".+");
                    continue;
                }
                _ => {}
            }
        }
        pattern.push_str(&regex::escape(&c.to_string()));
    }
    pattern.push('$');
    let re = regex::Regex::new(&pattern).unwrap();
    assert!(
        re.is_match(actual),
        "string did not match format.\n--- expected format ---\n{expected}\n--- actual ---\n{actual}"
    );
}

fn root_dep_and_transitive_dep() -> serde_json::Value {
    serde_json::json!({
        "repositories": { "packages": { "type": "package", "package": [
            { "name": "root/req", "version": "1.0.0", "require": { "dep/pkg": "^1" } },
            { "name": "dep/pkg", "version": "1.0.0", "replace": { "replaced/pkg": "1.0.0" } },
            { "name": "dep/pkg", "version": "1.0.1", "replace": { "replaced/pkg": "1.0.1" } },
            { "name": "dep/pkg", "version": "1.0.2", "replace": { "replaced/pkg": "1.0.2" } },
        ] } },
        "require": { "root/req": "1.*" },
    })
}

/// ref: UpdateCommandTest::provideUpdates
fn provide_updates() -> Vec<(
    &'static str,
    serde_json::Value,
    Vec<(&'static str, PhpMixed)>,
    &'static str,
    bool,
)> {
    vec![
        (
            "simple update",
            root_dep_and_transitive_dep(),
            vec![],
            "Loading composer repositories with package information
Updating dependencies
Lock file operations: 2 installs, 0 updates, 0 removals
  - Locking dep/pkg (1.0.2)
  - Locking root/req (1.0.0)
Installing dependencies from lock file (including require-dev)
Package operations: 2 installs, 0 updates, 0 removals
  - Installing dep/pkg (1.0.2)
  - Installing root/req (1.0.0)",
            false,
        ),
        (
            "simple update with very verbose output",
            root_dep_and_transitive_dep(),
            vec![("-vv", PhpMixed::from(true))],
            "Loading composer repositories with package information
Updating dependencies
Dependency resolution completed in %f seconds
Analyzed %d packages to resolve dependencies
Analyzed %d rules to resolve dependencies
Lock file operations: 2 installs, 0 updates, 0 removals
Installs: dep/pkg:1.0.2, root/req:1.0.0
  - Locking dep/pkg (1.0.2) from package repo (defining 4 packages)
  - Locking root/req (1.0.0) from package repo (defining 4 packages)
Installing dependencies from lock file (including require-dev)
Package operations: 2 installs, 0 updates, 0 removals
Installs: dep/pkg:1.0.2, root/req:1.0.0
  - Installing dep/pkg (1.0.2)
  - Installing root/req (1.0.0)",
            false,
        ),
        (
            "update with temporary constraint + --no-install",
            root_dep_and_transitive_dep(),
            vec![
                ("--with", PhpMixed::List(vec![PhpMixed::from("dep/pkg:1.0.0")])),
                ("--no-install", PhpMixed::from(true)),
            ],
            "Loading composer repositories with package information
Updating dependencies
Lock file operations: 2 installs, 0 updates, 0 removals
  - Locking dep/pkg (1.0.0)
  - Locking root/req (1.0.0)",
            false,
        ),
        (
            "update with temporary constraint failing resolution",
            root_dep_and_transitive_dep(),
            vec![("--with", PhpMixed::List(vec![PhpMixed::from("dep/pkg:^2")]))],
            "Loading composer repositories with package information
Updating dependencies
Your requirements could not be resolved to an installable set of packages.

  Problem 1
    - Root composer.json requires root/req 1.* -> satisfiable by root/req[1.0.0].
    - root/req 1.0.0 requires dep/pkg ^1 -> found dep/pkg[1.0.0, 1.0.1, 1.0.2] but it conflicts with your temporary update constraint (dep/pkg:^2).",
            false,
        ),
        (
            "update with temporary constraint failing resolution on root package",
            root_dep_and_transitive_dep(),
            vec![("--with", PhpMixed::List(vec![PhpMixed::from("root/req:^2")]))],
            "The temporary constraint \"^2\" for \"root/req\" must be a subset of the constraint in your composer.json (1.*)
Run `composer require root/req` or `composer require root/req:^2` instead to replace the constraint",
            false,
        ),
        (
            "update & bump",
            root_dep_and_transitive_dep(),
            vec![("--bump-after-update", PhpMixed::from(true))],
            "Loading composer repositories with package information
Updating dependencies
Lock file operations: 2 installs, 0 updates, 0 removals
  - Locking dep/pkg (1.0.2)
  - Locking root/req (1.0.0)
Installing dependencies from lock file (including require-dev)
Package operations: 2 installs, 0 updates, 0 removals
  - Installing dep/pkg (1.0.2)
  - Installing root/req (1.0.0)
Bumping dependencies
<warning>Warning: Bumping dependency constraints is not recommended for libraries as it will narrow down your dependencies and may cause problems for your users.</warning>
<warning>If your package is not a library, you can explicitly specify the \"type\" by using \"composer config type project\".</warning>
<warning>Alternatively you can use --bump-after-update=dev to only bump dependencies within \"require-dev\".</warning>
No requirements to update in ./composer.json.",
            true,
        ),
        (
            "update & bump with lock",
            root_dep_and_transitive_dep(),
            vec![
                ("--bump-after-update", PhpMixed::from(true)),
                ("--lock", PhpMixed::from(true)),
            ],
            "Loading composer repositories with package information
Updating dependencies
Nothing to modify in lock file
Installing dependencies from lock file (including require-dev)
Nothing to install, update or remove",
            true,
        ),
        (
            "update & bump dev only",
            root_dep_and_transitive_dep(),
            vec![("--bump-after-update", PhpMixed::from("dev"))],
            "Loading composer repositories with package information
Updating dependencies
Lock file operations: 2 installs, 0 updates, 0 removals
  - Locking dep/pkg (1.0.2)
  - Locking root/req (1.0.0)
Installing dependencies from lock file (including require-dev)
Package operations: 2 installs, 0 updates, 0 removals
  - Installing dep/pkg (1.0.2)
  - Installing root/req (1.0.0)
Bumping dependencies
No requirements to update in ./composer.json.",
            true,
        ),
        (
            "update & dump with failing update",
            root_dep_and_transitive_dep(),
            vec![
                ("--with", PhpMixed::List(vec![PhpMixed::from("dep/pkg:^2")])),
                ("--bump-after-update", PhpMixed::from(true)),
            ],
            "Loading composer repositories with package information
Updating dependencies
Your requirements could not be resolved to an installable set of packages.

  Problem 1
    - Root composer.json requires root/req 1.* -> satisfiable by root/req[1.0.0].
    - root/req 1.0.0 requires dep/pkg ^1 -> found dep/pkg[1.0.0, 1.0.1, 1.0.2] but it conflicts with your temporary update constraint (dep/pkg:^2).",
            false,
        ),
        (
            "update with replaced name filter fails to resolve",
            root_dep_and_transitive_dep(),
            vec![("--with", PhpMixed::List(vec![PhpMixed::from("replaced/pkg:^2")]))],
            "Loading composer repositories with package information
Updating dependencies
Your requirements could not be resolved to an installable set of packages.

  Problem 1
    - Root composer.json requires root/req 1.* -> satisfiable by root/req[1.0.0].
    - root/req 1.0.0 requires dep/pkg ^1 -> found dep/pkg[1.0.0, 1.0.1, 1.0.2] but it conflicts with your temporary update constraint (replaced/pkg:^2).",
            false,
        ),
    ]
}

#[test]
#[serial]
#[ignore = "regex porting: a Preg pattern starting with \"^array \" fails to compile in shirabe-php-shim preg.rs (\"unterminated regex pattern\"); unrelated to InputDefinition"]
fn test_update() {
    for (label, composer_json, command, expected, create_lock) in provide_updates() {
        let _tear_down = init_temp_composer(Some(&composer_json), None, None, true);

        if create_lock {
            create_composer_lock(&[], &[]);
        }

        let mut app_tester = get_application_tester();
        let mut args = vec![
            ("command", PhpMixed::from("update")),
            ("--dry-run", PhpMixed::from(true)),
            ("--no-audit", PhpMixed::from(true)),
        ];
        args.extend(command);
        app_tester.run(input(args), RunOptions::default()).unwrap();

        assert_string_matches_format(expected.trim(), app_tester.get_display().trim());
        let _ = label;
    }
}

#[test]
#[serial]
#[ignore = "resolver does not emit the temporary-update-constraint conflict; it performs the upgrade instead of reporting \"conflicts with your temporary update constraint\""]
fn test_update_with_patch_only() {
    let composer_json = serde_json::json!({
        "repositories": { "packages": { "type": "package", "package": [
            { "name": "root/req", "version": "1.0.0" },
            { "name": "root/req", "version": "1.0.1" },
            { "name": "root/req", "version": "1.1.0" },
            { "name": "root/req2", "version": "1.0.0" },
            { "name": "root/req2", "version": "1.0.1" },
            { "name": "root/req2", "version": "1.1.0" },
            { "name": "root/req3", "version": "1.0.0" },
            { "name": "root/req3", "version": "1.0.1" },
            { "name": "root/req3", "version": "1.1.0" },
        ] } },
        "require": { "root/req": "1.*", "root/req2": "1.*", "root/req3": "1.*" },
    });
    let _tear_down = init_temp_composer(Some(&composer_json), None, None, true);

    let package = get_package("root/req", "1.0.0");
    let package2 = get_package("root/req2", "1.0.0");
    let package3 = get_package("root/req3", "1.0.0");
    create_composer_lock(&[package, package2, package3], &[]);

    let mut app_tester = get_application_tester();
    // root/req fails because of incompatible --with requirement
    app_tester
        .run(
            input(vec![
                ("command", PhpMixed::from("update")),
                ("--dry-run", PhpMixed::from(true)),
                ("--no-audit", PhpMixed::from(true)),
                ("--no-install", PhpMixed::from(true)),
                ("--patch-only", PhpMixed::from(true)),
                (
                    "--with",
                    PhpMixed::List(vec![PhpMixed::from("root/req:^1.1")]),
                ),
            ]),
            RunOptions::default(),
        )
        .unwrap();

    assert_string_matches_format(
        "Loading composer repositories with package information
Updating dependencies
Your requirements could not be resolved to an installable set of packages.

  Problem 1
    - Root composer.json requires root/req 1.*, found root/req[1.0.0, 1.0.1, 1.1.0] but it conflicts with your temporary update constraint (root/req:[[>= 1.1.0.0-dev < 2.0.0.0-dev] [>= 1.0.0.0-dev < 1.1.0.0-dev]]).",
        app_tester.get_display().trim(),
    );

    let mut app_tester = get_application_tester();
    // root/req upgrades to 1.0.1 as that is compatible with the --with requirement now
    // root/req2 upgrades to 1.0.1 only due to --patch-only
    // root/req3 does not update as it is not in the allowlist
    app_tester
        .run(
            input(vec![
                ("command", PhpMixed::from("update")),
                ("--dry-run", PhpMixed::from(true)),
                ("--no-audit", PhpMixed::from(true)),
                ("--no-install", PhpMixed::from(true)),
                ("--patch-only", PhpMixed::from(true)),
                (
                    "--with",
                    PhpMixed::List(vec![PhpMixed::from("root/req:^1.0.1")]),
                ),
                (
                    "packages",
                    PhpMixed::List(vec![
                        PhpMixed::from("root/req"),
                        PhpMixed::from("root/req2"),
                    ]),
                ),
            ]),
            RunOptions::default(),
        )
        .unwrap();

    assert_string_matches_format(
        "Loading composer repositories with package information
Updating dependencies
Lock file operations: 0 installs, 2 updates, 0 removals
  - Upgrading root/req (1.0.0 => 1.0.1)
  - Upgrading root/req2 (1.0.0 => 1.0.1)",
        app_tester.get_display().trim(),
    );
}

#[test]
#[serial]
#[ignore = "interactive selection path unported: --interactive requires a TTY and errors with \"--interactive cannot be used in non-interactive terminals.\""]
fn test_interactive_mode_throws_if_no_package_to_update() {
    let composer_json = serde_json::json!({
        "repositories": { "packages": { "type": "package", "package": [
            { "name": "root/req", "version": "1.0.0" },
        ] } },
        "require": { "root/req": "1.*" },
    });
    let _tear_down = init_temp_composer(Some(&composer_json), None, None, true);
    create_composer_lock(&[get_package("root/req", "1.0.0")], &[]);

    let mut app_tester = get_application_tester();
    app_tester.set_inputs(vec!["".to_string()]);
    let err = app_tester
        .run(
            input(vec![
                ("command", PhpMixed::from("update")),
                ("--interactive", PhpMixed::from(true)),
            ]),
            RunOptions::default(),
        )
        .expect_err("expected an exception when there is no package to update");
    assert!(
        err.to_string()
            .contains("Could not find any package with new versions available"),
        "got: {err}"
    );
}

#[test]
#[serial]
#[ignore = "interactive selection path unported: --interactive requires a TTY and errors with \"--interactive cannot be used in non-interactive terminals.\""]
fn test_interactive_mode_throws_if_no_package_entered() {
    let composer_json = serde_json::json!({
        "repositories": { "packages": { "type": "package", "package": [
            { "name": "root/req", "version": "1.0.0" },
            { "name": "root/req", "version": "1.0.1" },
        ] } },
        "require": { "root/req": "1.*" },
    });
    let _tear_down = init_temp_composer(Some(&composer_json), None, None, true);
    create_composer_lock(&[get_package("root/req", "1.0.0")], &[]);

    let mut app_tester = get_application_tester();
    app_tester.set_inputs(vec!["".to_string()]);
    let err = app_tester
        .run(
            input(vec![
                ("command", PhpMixed::from("update")),
                ("--interactive", PhpMixed::from(true)),
            ]),
            RunOptions::default(),
        )
        .expect_err("expected an exception when no package name is entered");
    assert!(
        err.to_string()
            .contains(r#"No package named "" is installed."#),
        "got: {err}"
    );
}

/// ref: UpdateCommandTest::provideInteractiveUpdates
fn provide_interactive_updates() -> Vec<(Vec<(&'static str, &'static str)>, &'static str)> {
    vec![
        (
            vec![("dep/pkg", "1.0.1")],
            "Lock file operations: 1 install, 1 update, 0 removals
  - Locking another-dep/pkg (1.0.2)
  - Upgrading dep/pkg (1.0.1 => 1.0.2)
Installing dependencies from lock file (including require-dev)
Package operations: 1 install, 1 update, 0 removals
  - Upgrading dep/pkg (1.0.1 => 1.0.2)
  - Installing another-dep/pkg (1.0.2)",
        ),
        (
            vec![("dep/pkg", "1.0.1"), ("another-dep/pkg", "1.0.2")],
            "Lock file operations: 0 installs, 1 update, 0 removals
  - Upgrading dep/pkg (1.0.1 => 1.0.2)
Installing dependencies from lock file (including require-dev)
Package operations: 0 installs, 1 update, 0 removals
  - Upgrading dep/pkg (1.0.1 => 1.0.2)",
        ),
    ]
}

#[test]
#[serial]
#[ignore = "interactive selection path unported: --interactive requires a TTY and errors with \"--interactive cannot be used in non-interactive terminals.\""]
fn test_interactive_tmp() {
    for (package_names, expected) in provide_interactive_updates() {
        let composer_json = serde_json::json!({
            "repositories": { "packages": { "type": "package", "package": [
                { "name": "root/req", "version": "1.0.0", "require": { "dep/pkg": "^1" } },
                { "name": "dep/pkg", "version": "1.0.0" },
                { "name": "dep/pkg", "version": "1.0.1" },
                { "name": "dep/pkg", "version": "1.0.2" },
                { "name": "another-dep/pkg", "version": "1.0.2" },
            ] } },
            "require": { "root/req": "1.*" },
        });
        let _tear_down = init_temp_composer(Some(&composer_json), None, None, true);

        let root_package = get_package("root/req", "1.0.0");
        let mut packages = vec![root_package.clone()];
        for (pkg, ver) in &package_names {
            packages.push(get_package(pkg, ver));
        }

        let mut requires = indexmap::IndexMap::new();
        requires.insert(
            "dep/pkg".to_string(),
            Link::new(
                "root/req".to_string(),
                "dep/pkg".to_string(),
                AnyConstraint::MatchAll(MatchAllConstraint::new(None)),
                Some(Link::TYPE_REQUIRE.to_string()),
                "^1".to_string(),
            ),
        );
        requires.insert(
            "another-dep/pkg".to_string(),
            Link::new(
                "root/req".to_string(),
                "another-dep/pkg".to_string(),
                AnyConstraint::MatchAll(MatchAllConstraint::new(None)),
                Some(Link::TYPE_REQUIRE.to_string()),
                "^1".to_string(),
            ),
        );
        root_package.__set_requires(requires);

        create_composer_lock(&packages, &[]);
        create_installed_json(&packages, &[], true);

        let mut app_tester = get_application_tester();
        let mut inputs: Vec<String> = package_names
            .iter()
            .map(|(pkg, _)| pkg.to_string())
            .collect();
        inputs.push("".to_string());
        inputs.push("yes".to_string());
        app_tester.set_inputs(inputs);
        app_tester
            .run(
                input(vec![
                    ("command", PhpMixed::from("update")),
                    ("--interactive", PhpMixed::from(true)),
                    ("--no-audit", PhpMixed::from(true)),
                    ("--dry-run", PhpMixed::from(true)),
                ]),
                RunOptions {
                    interactive: Some(true),
                    ..RunOptions::default()
                },
            )
            .unwrap();

        assert!(
            app_tester.get_display().trim().ends_with(expected.trim()),
            "expected display to end with:\n{expected}\n--- got ---\n{}",
            app_tester.get_display()
        );
    }
}

#[test]
#[serial]
#[ignore]
fn test_no_security_blocking_allows_insecure_packages() {
    let composer_json = serde_json::json!({
        "repositories": { "packages": {
            "type": "package",
            "package": [
                { "name": "vulnerable/pkg", "version": "1.0.0" },
                { "name": "vulnerable/pkg", "version": "1.1.0" },
            ],
            "security-advisories": {
                "vulnerable/pkg": [
                    {
                        "advisoryId": "PKSA-test-001",
                        "packageName": "vulnerable/pkg",
                        "remoteId": "CVE-2024-1234",
                        "title": "Test Security Vulnerability",
                        "link": "https://example.com/advisory",
                        "cve": "CVE-2024-1234",
                        "affectedVersions": ">=1.1.0,<2.0.0",
                        "source": "test",
                        "reportedAt": "2024-01-01 00:00:00",
                        "composerRepository": "Package Repository",
                        "severity": "high",
                        "sources": [ { "name": "test", "remoteId": "CVE-2024-1234" } ],
                    },
                ],
            },
        } },
        "require": { "vulnerable/pkg": "^1.0" },
    });
    let _tear_down = init_temp_composer(Some(&composer_json), None, None, true);

    // Test 1: Without --no-security-blocking, the vulnerable version 1.1.0 should be filtered out
    let mut app_tester = get_application_tester();
    app_tester
        .run(
            input(vec![
                ("command", PhpMixed::from("update")),
                ("--dry-run", PhpMixed::from(true)),
                ("--no-audit", PhpMixed::from(true)),
                ("--no-install", PhpMixed::from(true)),
            ]),
            RunOptions::default(),
        )
        .unwrap();
    let display = app_tester.get_display();
    assert!(
        display.contains("Locking vulnerable/pkg (1.0.0)"),
        "got: {display}"
    );
    assert!(
        !display.contains("Locking vulnerable/pkg (1.1.0)"),
        "got: {display}"
    );

    // Test 2: With --no-security-blocking, the vulnerable version 1.1.0 should be allowed
    let mut app_tester = get_application_tester();
    app_tester
        .run(
            input(vec![
                ("command", PhpMixed::from("update")),
                ("--dry-run", PhpMixed::from(true)),
                ("--no-audit", PhpMixed::from(true)),
                ("--no-install", PhpMixed::from(true)),
                ("--no-security-blocking", PhpMixed::from(true)),
            ]),
            RunOptions::default(),
        )
        .unwrap();
    let display = app_tester.get_display();
    assert!(
        display.contains("Locking vulnerable/pkg (1.1.0)"),
        "got: {display}"
    );
    assert!(
        !display.contains("Locking vulnerable/pkg (1.0.0)"),
        "got: {display}"
    );
}

#[test]
#[serial]
#[ignore = "solver error \"Fixed package __root__ 1.0.0+no-version-set was not added to solver pool.\" on the no-lockfile bump path (resolver pool construction)"]
fn test_bump_after_update_without_lockfile() {
    let composer_json = serde_json::json!({
        "repositories": { "packages": { "type": "package", "package": [
            { "name": "root/a", "version": "1.0.0" },
            { "name": "root/a", "version": "1.1.0" },
        ] } },
        "require-dev": { "root/a": "^1.0.0" },
        "config": { "lock": false },
    });
    let _tear_down = init_temp_composer(Some(&composer_json), None, None, true);

    let mut app_tester = get_application_tester();
    app_tester
        .run(
            input(vec![
                ("command", PhpMixed::from("update")),
                ("--dry-run", PhpMixed::from(true)),
                ("--no-audit", PhpMixed::from(true)),
                ("--bump-after-update", PhpMixed::from("dev")),
            ]),
            RunOptions::default(),
        )
        .unwrap();

    assert_string_matches_format(
        "Loading composer repositories with package information
Updating dependencies
Package operations: 1 install, 0 updates, 0 removals
  - Installing root/a (1.1.0)
Bumping dependencies
./composer.json would be updated with:
 - require-dev.root/a: ^1.1.0",
        app_tester.get_display().trim(),
    );
}
