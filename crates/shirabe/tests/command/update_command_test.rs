//! ref: composer/tests/Composer/Test/Command/UpdateCommandTest.php sha1:0781a959603135520ba57851e2cf596b8975d812

use crate::test_case::{
    RunOptions, create_composer_lock, create_installed_json, get_application_tester, get_package,
    init_temp_composer,
};
use bstr::{BStr, BString, ByteSlice};
use serial_test::serial;
use shirabe::package::Link;
use shirabe_php_shim::{ToPhpString, error_message, trim, utf8_text};
use shirabe_semver::constraint::{AnyConstraint, MatchAllConstraint};
use shirabe_symfony_console::input::{InputValue, ParameterName};

fn input(pairs: Vec<(&BStr, InputValue)>) -> Vec<(ParameterName, InputValue)> {
    pairs
        .into_iter()
        .map(|(k, v)| (ParameterName::of(k), v))
        .collect()
}

/// Minimal port of PHPUnit's `assertStringMatchesFormat` covering the placeholders these fixtures
/// use: `%d` (integer), `%f` (float), `%s` (non-newline run), `%a` (any run incl. newlines).
#[expect(clippy::disallowed_types, reason = "regex takes a text pattern")]
fn assert_string_matches_format(expected: &BStr, actual: impl AsRef<[u8]>) {
    let actual = actual.as_ref();
    let mut pattern = String::from("(?s)^");
    let mut chars = utf8_text(expected).unwrap().chars().peekable();
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
        pattern.push_str(&regex::escape(c.encode_utf8(&mut [0; 4])));
    }
    pattern.push('$');
    let re = regex::bytes::Regex::new(&pattern).unwrap();
    assert!(
        re.is_match(actual),
        "string did not match format.\n--- expected format ---\n{expected}\n--- actual ---\n{}",
        actual.as_bstr().display_lossy(),
        expected = expected.display_lossy()
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
    &'static BStr,
    serde_json::Value,
    Vec<(&'static BStr, InputValue)>,
    &'static BStr,
    bool,
)> {
    vec![
        (
            BStr::new("simple update"),
            root_dep_and_transitive_dep(),
            vec![],
            BStr::new("Loading composer repositories with package information
Updating dependencies
Lock file operations: 2 installs, 0 updates, 0 removals
  - Locking dep/pkg (1.0.2)
  - Locking root/req (1.0.0)
Installing dependencies from lock file (including require-dev)
Package operations: 2 installs, 0 updates, 0 removals
  - Installing dep/pkg (1.0.2)
  - Installing root/req (1.0.0)"),
            false,
        ),
        (
            BStr::new("simple update with very verbose output"),
            root_dep_and_transitive_dep(),
            vec![(BStr::new("-vv"), InputValue::from(true))],
            BStr::new("Loading composer repositories with package information
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
  - Installing root/req (1.0.0)"),
            false,
        ),
        (
            BStr::new("update with temporary constraint + --no-install"),
            root_dep_and_transitive_dep(),
            vec![
                (BStr::new("--with"), InputValue::Array(vec!["dep/pkg:1.0.0".into()])),
                (BStr::new("--no-install"), InputValue::from(true)),
            ],
            BStr::new("Loading composer repositories with package information
Updating dependencies
Lock file operations: 2 installs, 0 updates, 0 removals
  - Locking dep/pkg (1.0.0)
  - Locking root/req (1.0.0)"),
            false,
        ),
        (
            BStr::new("update with temporary constraint failing resolution"),
            root_dep_and_transitive_dep(),
            vec![(BStr::new("--with"), InputValue::Array(vec!["dep/pkg:^2".into()]))],
            BStr::new("Loading composer repositories with package information
Updating dependencies
Your requirements could not be resolved to an installable set of packages.

  Problem 1
    - Root composer.json requires root/req 1.* -> satisfiable by root/req[1.0.0].
    - root/req 1.0.0 requires dep/pkg ^1 -> found dep/pkg[1.0.0, 1.0.1, 1.0.2] but it conflicts with your temporary update constraint (dep/pkg:^2)."),
            false,
        ),
        (
            BStr::new("update with temporary constraint failing resolution on root package"),
            root_dep_and_transitive_dep(),
            vec![(BStr::new("--with"), InputValue::Array(vec!["root/req:^2".into()]))],
            BStr::new("The temporary constraint \"^2\" for \"root/req\" must be a subset of the constraint in your composer.json (1.*)
Run `shirabe require root/req` or `shirabe require root/req:^2` instead to replace the constraint"),
            false,
        ),
        (
            BStr::new("update & bump"),
            root_dep_and_transitive_dep(),
            vec![(BStr::new("--bump-after-update"), InputValue::from(true))],
            BStr::new("Loading composer repositories with package information
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
<warning>If your package is not a library, you can explicitly specify the \"type\" by using \"shirabe config type project\".</warning>
<warning>Alternatively you can use --bump-after-update=dev to only bump dependencies within \"require-dev\".</warning>
No requirements to update in ./composer.json."),
            true,
        ),
        (
            BStr::new("update & bump with lock"),
            root_dep_and_transitive_dep(),
            vec![
                (BStr::new("--bump-after-update"), InputValue::from(true)),
                (BStr::new("--lock"), InputValue::from(true)),
            ],
            BStr::new("Loading composer repositories with package information
Updating dependencies
Nothing to modify in lock file
Installing dependencies from lock file (including require-dev)
Nothing to install, update or remove"),
            true,
        ),
        (
            BStr::new("update & bump dev only"),
            root_dep_and_transitive_dep(),
            vec![(BStr::new("--bump-after-update"), InputValue::from("dev"))],
            BStr::new("Loading composer repositories with package information
Updating dependencies
Lock file operations: 2 installs, 0 updates, 0 removals
  - Locking dep/pkg (1.0.2)
  - Locking root/req (1.0.0)
Installing dependencies from lock file (including require-dev)
Package operations: 2 installs, 0 updates, 0 removals
  - Installing dep/pkg (1.0.2)
  - Installing root/req (1.0.0)
Bumping dependencies
No requirements to update in ./composer.json."),
            true,
        ),
        (
            BStr::new("update & dump with failing update"),
            root_dep_and_transitive_dep(),
            vec![
                (BStr::new("--with"), InputValue::Array(vec!["dep/pkg:^2".into()])),
                (BStr::new("--bump-after-update"), InputValue::from(true)),
            ],
            BStr::new("Loading composer repositories with package information
Updating dependencies
Your requirements could not be resolved to an installable set of packages.

  Problem 1
    - Root composer.json requires root/req 1.* -> satisfiable by root/req[1.0.0].
    - root/req 1.0.0 requires dep/pkg ^1 -> found dep/pkg[1.0.0, 1.0.1, 1.0.2] but it conflicts with your temporary update constraint (dep/pkg:^2)."),
            false,
        ),
        (
            BStr::new("update with replaced name filter fails to resolve"),
            root_dep_and_transitive_dep(),
            vec![(BStr::new("--with"), InputValue::Array(vec!["replaced/pkg:^2".into()]))],
            BStr::new("Loading composer repositories with package information
Updating dependencies
Your requirements could not be resolved to an installable set of packages.

  Problem 1
    - Root composer.json requires root/req 1.* -> satisfiable by root/req[1.0.0].
    - root/req 1.0.0 requires dep/pkg ^1 -> found dep/pkg[1.0.0, 1.0.1, 1.0.2] but it conflicts with your temporary update constraint (replaced/pkg:^2)."),
            false,
        ),
    ]
}

#[test]
#[serial]
fn test_update() {
    for (label, composer_json, command, expected, create_lock) in provide_updates() {
        let _tear_down = init_temp_composer(Some(&composer_json), None, None, true);

        if create_lock {
            create_composer_lock(&[], &[]);
        }

        let mut app_tester = get_application_tester();
        let mut args = vec![
            (BStr::new("command"), InputValue::from("update")),
            (BStr::new("--dry-run"), InputValue::from(true)),
            (BStr::new("--no-audit"), InputValue::from(true)),
        ];
        args.extend(command);
        app_tester.run(input(args), RunOptions::default()).unwrap();

        assert_string_matches_format(trim(expected).as_bstr(), trim(app_tester.get_display()));
        let _ = label;
    }
}

#[test]
#[serial]
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
                (BStr::new("command"), InputValue::from("update")),
                (BStr::new("--dry-run"), InputValue::from(true)),
                (BStr::new("--no-audit"), InputValue::from(true)),
                (BStr::new("--no-install"), InputValue::from(true)),
                (BStr::new("--patch-only"), InputValue::from(true)),
                (
                    BStr::new("--with"),
                    InputValue::Array(vec!["root/req:^1.1".into()]),
                ),
            ]),
            RunOptions::default(),
        )
        .unwrap();

    assert_string_matches_format(
        BStr::new("Loading composer repositories with package information
Updating dependencies
Your requirements could not be resolved to an installable set of packages.

  Problem 1
    - Root composer.json requires root/req 1.*, found root/req[1.0.0, 1.0.1, 1.1.0] but it conflicts with your temporary update constraint (root/req:[[>= 1.1.0.0-dev < 2.0.0.0-dev] [>= 1.0.0.0-dev < 1.1.0.0-dev]])."),
        trim(app_tester.get_display()),
    );

    let mut app_tester = get_application_tester();
    // root/req upgrades to 1.0.1 as that is compatible with the --with requirement now
    // root/req2 upgrades to 1.0.1 only due to --patch-only
    // root/req3 does not update as it is not in the allowlist
    app_tester
        .run(
            input(vec![
                (BStr::new("command"), InputValue::from("update")),
                (BStr::new("--dry-run"), InputValue::from(true)),
                (BStr::new("--no-audit"), InputValue::from(true)),
                (BStr::new("--no-install"), InputValue::from(true)),
                (BStr::new("--patch-only"), InputValue::from(true)),
                (
                    BStr::new("--with"),
                    InputValue::Array(vec!["root/req:^1.0.1".into()]),
                ),
                (
                    BStr::new("packages"),
                    InputValue::Array(vec!["root/req".into(), "root/req2".into()]),
                ),
            ]),
            RunOptions::default(),
        )
        .unwrap();

    assert_string_matches_format(
        BStr::new(
            "Loading composer repositories with package information
Updating dependencies
Lock file operations: 0 installs, 2 updates, 0 removals
  - Upgrading root/req (1.0.0 => 1.0.1)
  - Upgrading root/req2 (1.0.0 => 1.0.1)",
        ),
        trim(app_tester.get_display()),
    );
}

#[test]
#[serial]
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
    app_tester.set_inputs(vec![BString::from("")]);
    let err = app_tester
        .run(
            input(vec![
                (BStr::new("command"), InputValue::from("update")),
                (BStr::new("--interactive"), InputValue::from(true)),
            ]),
            RunOptions::default(),
        )
        .expect_err("expected an exception when there is no package to update");
    assert!(
        error_message(&err).contains_str("Could not find any package with new versions available"),
        "got: {err}"
    );
}

#[test]
#[serial]
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
    app_tester.set_inputs(vec![BString::from("")]);
    let err = app_tester
        .run(
            input(vec![
                (BStr::new("command"), InputValue::from("update")),
                (BStr::new("--interactive"), InputValue::from(true)),
            ]),
            RunOptions::default(),
        )
        .expect_err("expected an exception when no package name is entered");
    assert!(
        error_message(&err).contains_str(r#"No package named "" is installed."#),
        "got: {err}"
    );
}

/// ref: UpdateCommandTest::provideInteractiveUpdates
fn provide_interactive_updates() -> Vec<(Vec<(&'static BStr, &'static BStr)>, &'static BStr)> {
    vec![
        (
            vec![(BStr::new("dep/pkg"), BStr::new("1.0.1"))],
            BStr::new(
                "Lock file operations: 1 install, 1 update, 0 removals
  - Locking another-dep/pkg (1.0.2)
  - Upgrading dep/pkg (1.0.1 => 1.0.2)
Installing dependencies from lock file (including require-dev)
Package operations: 1 install, 1 update, 0 removals
  - Upgrading dep/pkg (1.0.1 => 1.0.2)
  - Installing another-dep/pkg (1.0.2)",
            ),
        ),
        (
            vec![
                (BStr::new("dep/pkg"), BStr::new("1.0.1")),
                (BStr::new("another-dep/pkg"), BStr::new("1.0.2")),
            ],
            BStr::new(
                "Lock file operations: 0 installs, 1 update, 0 removals
  - Upgrading dep/pkg (1.0.1 => 1.0.2)
Installing dependencies from lock file (including require-dev)
Package operations: 0 installs, 1 update, 0 removals
  - Upgrading dep/pkg (1.0.1 => 1.0.2)",
            ),
        ),
    ]
}

#[test]
#[serial]
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
            BString::from("dep/pkg"),
            Link::new(
                BString::from("root/req"),
                BString::from("dep/pkg"),
                AnyConstraint::MatchAll(MatchAllConstraint::new(None)),
                Some(Link::TYPE_REQUIRE.to_owned()),
                Some(BString::from("^1")),
            ),
        );
        requires.insert(
            BString::from("another-dep/pkg"),
            Link::new(
                BString::from("root/req"),
                BString::from("another-dep/pkg"),
                AnyConstraint::MatchAll(MatchAllConstraint::new(None)),
                Some(Link::TYPE_REQUIRE.to_owned()),
                Some(BString::from("^1")),
            ),
        );
        root_package.__set_requires(requires.into_iter().collect::<indexmap::IndexMap<_, _>>());

        create_composer_lock(&packages, &[]);
        create_installed_json(&packages, &[], true);

        let mut app_tester = get_application_tester();
        let mut inputs: Vec<BString> = package_names
            .iter()
            .map(|(pkg, _)| pkg.to_php_string())
            .collect();
        inputs.push(BString::from(""));
        inputs.push(BString::from("yes"));
        app_tester.set_inputs(inputs);
        app_tester
            .run(
                input(vec![
                    (BStr::new("command"), InputValue::from("update")),
                    (BStr::new("--interactive"), InputValue::from(true)),
                    (BStr::new("--no-audit"), InputValue::from(true)),
                    (BStr::new("--dry-run"), InputValue::from(true)),
                ]),
                RunOptions {
                    interactive: Some(true),
                    ..RunOptions::default()
                },
            )
            .unwrap();

        assert!(
            trim(app_tester.get_display()).ends_with_str(trim(expected)),
            "expected display to end with:\n{expected}\n--- got ---\n{}",
            app_tester.get_display().display_lossy(),
            expected = expected.display_lossy()
        );
    }
}

#[test]
#[serial]
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
                (BStr::new("command"), InputValue::from("update")),
                (BStr::new("--dry-run"), InputValue::from(true)),
                (BStr::new("--no-audit"), InputValue::from(true)),
                (BStr::new("--no-install"), InputValue::from(true)),
            ]),
            RunOptions::default(),
        )
        .unwrap();
    let display = app_tester.get_display();
    assert!(
        display.contains_str("Locking vulnerable/pkg (1.0.0)"),
        "got: {}",
        display.display_lossy()
    );
    assert!(
        !display.contains_str("Locking vulnerable/pkg (1.1.0)"),
        "got: {}",
        display.display_lossy()
    );

    // Test 2: With --no-security-blocking, the vulnerable version 1.1.0 should be allowed
    let mut app_tester = get_application_tester();
    app_tester
        .run(
            input(vec![
                (BStr::new("command"), InputValue::from("update")),
                (BStr::new("--dry-run"), InputValue::from(true)),
                (BStr::new("--no-audit"), InputValue::from(true)),
                (BStr::new("--no-install"), InputValue::from(true)),
                (BStr::new("--no-security-blocking"), InputValue::from(true)),
            ]),
            RunOptions::default(),
        )
        .unwrap();
    let display = app_tester.get_display();
    assert!(
        display.contains_str("Locking vulnerable/pkg (1.1.0)"),
        "got: {}",
        display.display_lossy()
    );
    assert!(
        !display.contains_str("Locking vulnerable/pkg (1.0.0)"),
        "got: {}",
        display.display_lossy()
    );
}

#[test]
#[serial]
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
                (BStr::new("command"), InputValue::from("update")),
                (BStr::new("--dry-run"), InputValue::from(true)),
                (BStr::new("--no-audit"), InputValue::from(true)),
                (BStr::new("--bump-after-update"), InputValue::from("dev")),
            ]),
            RunOptions::default(),
        )
        .unwrap();

    assert_string_matches_format(
        BStr::new(
            "Loading composer repositories with package information
Updating dependencies
Package operations: 1 install, 0 updates, 0 removals
  - Installing root/a (1.1.0)
Bumping dependencies
./composer.json would be updated with:
 - require-dev.root/a: ^1.1.0",
        ),
        trim(app_tester.get_display()),
    );
}
