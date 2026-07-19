//! ref: composer/tests/Composer/Test/Package/Version/VersionGuesserTest.php

use crate::process_executor_mock::{cmd, cmd_full, get_process_executor_mock};
use indexmap::IndexMap;
use serial_test::serial;
use shirabe::config::Config;
use shirabe::package::version::{VersionGuesser, VersionParser};
use shirabe::util::Git as GitUtil;
use shirabe::util::platform::Platform;
use shirabe::util::process_executor::{MockExpectation, MockHandler, ProcessExecutor};
use shirabe_php_shim::PhpMixed;

// Mirrors VersionGuesserTest::setUp/tearDown: reset GitUtil's cached `version`
// static so each test re-runs `git --version` against its own mock.
fn set_up() {
    GitUtil::__reset_version();
}

struct TearDown;

impl Drop for TearDown {
    fn drop(&mut self) {
        GitUtil::__reset_version();
    }
}

// `$config = new Config; $config->merge(['repositories' => ['packagist' => false]]);`
fn make_config() -> std::rc::Rc<std::cell::RefCell<Config>> {
    let mut config = Config::new(true, None);
    let mut repositories: IndexMap<String, PhpMixed> = IndexMap::new();
    repositories.insert("packagist".to_string(), PhpMixed::Bool(false));
    let mut merge: IndexMap<String, PhpMixed> = IndexMap::new();
    merge.insert("repositories".to_string(), PhpMixed::Array(repositories));
    config.merge(&merge, Config::SOURCE_UNKNOWN);
    std::rc::Rc::new(std::cell::RefCell::new(config))
}

fn make_guesser(
    config: std::rc::Rc<std::cell::RefCell<Config>>,
    process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
) -> VersionGuesser {
    VersionGuesser::new(config, process, VersionParser::new(), None)
}

// Helper to build a `['key' => 'value']` package config used in the tests.
fn package_config(entries: &[(&str, PhpMixed)]) -> IndexMap<String, PhpMixed> {
    let mut map: IndexMap<String, PhpMixed> = IndexMap::new();
    for (k, v) in entries {
        map.insert(k.to_string(), v.clone());
    }
    map
}

fn string_list(items: &[&str]) -> PhpMixed {
    PhpMixed::List(
        items
            .iter()
            .map(|s| PhpMixed::String(s.to_string()))
            .collect(),
    )
}

#[test]
#[serial]
fn test_hg_guess_version_returns_data() {
    set_up();
    let _td = TearDown;
    let branch = "default";

    let expectations: Vec<MockExpectation> = vec![
        cmd_full(["git", "--version"], 0, "git version 2.33.0", ""),
        cmd_full(
            ["git", "branch", "-a", "--no-color", "--no-abbrev", "-v"],
            128,
            "",
            "",
        ),
        cmd_full(["git", "describe", "--exact-match", "--tags"], 128, "", ""),
        cmd_full(
            [
                "git",
                "rev-list",
                "--no-commit-header",
                "--format=%H",
                "-n1",
                "HEAD",
                "--no-show-signature",
            ],
            128,
            "",
            "",
        ),
        cmd_full(["hg", "branch"], 0, branch, ""),
        cmd_full(["hg", "branches"], 0, "", ""),
        cmd_full(["hg", "bookmarks"], 0, "", ""),
    ];
    let (process, _guard) = get_process_executor_mock(expectations, true, MockHandler::default());

    let config = make_config();
    let mut guesser = make_guesser(config, process);
    let version_data = guesser
        .guess_version(&IndexMap::new(), "dummy/path")
        .unwrap()
        .expect("expected version data");

    assert_eq!(format!("dev-{}", branch), version_data.version.unwrap());
    assert_eq!(
        format!("dev-{}", branch),
        version_data.pretty_version.unwrap()
    );
    assert!(version_data.commit.as_deref().unwrap_or("").is_empty());
}

#[test]
#[serial]
fn test_guess_version_returns_data() {
    set_up();
    let _td = TearDown;
    let commit_hash = "03a15d220da53c52eddd5f32ffca64a7b3801bea";
    let another_commit_hash = "03a15d220da53c52eddd5f32ffca64a7b3801bea";

    let expectations: Vec<MockExpectation> = vec![
        cmd_full(["git", "--version"], 0, "git version 2.52.0", ""),
        cmd_full(
            ["git", "branch", "-a", "--no-color", "--no-abbrev", "-v"],
            0,
            format!(
                "* master {} Commit message\n(no branch) {} Commit message\n",
                commit_hash, another_commit_hash
            ),
            "",
        ),
    ];
    let (process, _guard) = get_process_executor_mock(expectations, true, MockHandler::default());

    let config = make_config();
    let mut guesser = make_guesser(config, process);
    let version_data = guesser
        .guess_version(&IndexMap::new(), "dummy/path")
        .unwrap()
        .expect("expected version data");

    assert_eq!("dev-master", version_data.version.unwrap());
    assert_eq!("dev-master", version_data.pretty_version.unwrap());
    assert!(version_data.feature_version.is_none());
    assert!(version_data.feature_pretty_version.is_none());
    assert_eq!(commit_hash, version_data.commit.unwrap());
}

#[test]
#[serial]
fn test_guess_version_does_not_see_custom_default_branch_as_non_feature_branch() {
    set_up();
    let _td = TearDown;
    let commit_hash = "03a15d220da53c52eddd5f32ffca64a7b3801bea";
    let another_commit_hash = "13a15d220da53c52eddd5f32ffca64a7b3801bea";

    let expectations: Vec<MockExpectation> = vec![
        cmd_full(["git", "--version"], 0, "git version 2.52.0", ""),
        cmd_full(
            ["git", "branch", "-a", "--no-color", "--no-abbrev", "-v"],
            0,
            // Assumption here is that arbitrary would be the default branch
            format!(
                "  arbitrary {} Commit message\n* current {} Another message\n",
                commit_hash, another_commit_hash
            ),
            "",
        ),
    ];
    let (process, _guard) = get_process_executor_mock(expectations, true, MockHandler::default());

    let config = make_config();
    let mut guesser = make_guesser(config, process);
    let version_data = guesser
        .guess_version(
            &package_config(&[("version", PhpMixed::String("self.version".to_string()))]),
            "dummy/path",
        )
        .unwrap()
        .expect("expected version data");

    assert_eq!("dev-current", version_data.version.unwrap());
    assert_eq!(another_commit_hash, version_data.commit.unwrap());
}

#[test]
#[serial]
fn test_guess_version_reads_and_respects_non_feature_branches_configuration_for_arbitrary_naming() {
    set_up();
    let _td = TearDown;
    let commit_hash = "03a15d220da53c52eddd5f32ffca64a7b3801bea";
    let another_commit_hash = "13a15d220da53c52eddd5f32ffca64a7b3801bea";

    let expectations: Vec<MockExpectation> = vec![
        cmd_full(["git", "--version"], 0, "git version 2.52.0", ""),
        cmd_full(
            ["git", "branch", "-a", "--no-color", "--no-abbrev", "-v"],
            0,
            format!(
                "  arbitrary {} Commit message\n* feature {} Another message\n",
                commit_hash, another_commit_hash
            ),
            "",
        ),
        cmd_full(
            ["git", "rev-list", "arbitrary..feature"],
            0,
            format!("{}\n", another_commit_hash),
            "",
        ),
    ];
    let (process, _guard) = get_process_executor_mock(expectations, true, MockHandler::default());

    let config = make_config();
    let mut guesser = make_guesser(config, process);
    let version_data = guesser
        .guess_version(
            &package_config(&[
                ("version", PhpMixed::String("self.version".to_string())),
                ("non-feature-branches", string_list(&["arbitrary"])),
            ]),
            "dummy/path",
        )
        .unwrap()
        .expect("expected version data");

    assert_eq!("dev-arbitrary", version_data.version.unwrap());
    assert_eq!(another_commit_hash, version_data.commit.unwrap());
    assert_eq!(
        "dev-feature",
        version_data.feature_version.as_deref().unwrap()
    );
    assert_eq!(
        "dev-feature",
        version_data.feature_pretty_version.as_deref().unwrap()
    );
}

#[test]
#[serial]
fn test_guess_version_reads_and_respects_non_feature_branches_configuration_for_arbitrary_naming_regex()
 {
    set_up();
    let _td = TearDown;
    let commit_hash = "03a15d220da53c52eddd5f32ffca64a7b3801bea";
    let another_commit_hash = "13a15d220da53c52eddd5f32ffca64a7b3801bea";

    let expectations: Vec<MockExpectation> = vec![
        cmd_full(["git", "--version"], 0, "git version 2.52.0", ""),
        cmd_full(
            ["git", "branch", "-a", "--no-color", "--no-abbrev", "-v"],
            0,
            format!(
                "  latest-testing {} Commit message\n* feature {} Another message\n",
                commit_hash, another_commit_hash
            ),
            "",
        ),
        cmd_full(
            ["git", "rev-list", "latest-testing..feature"],
            0,
            format!("{}\n", another_commit_hash),
            "",
        ),
    ];
    let (process, _guard) = get_process_executor_mock(expectations, true, MockHandler::default());

    let config = make_config();
    let mut guesser = make_guesser(config, process);
    let version_data = guesser
        .guess_version(
            &package_config(&[
                ("version", PhpMixed::String("self.version".to_string())),
                ("non-feature-branches", string_list(&["latest-.*"])),
            ]),
            "dummy/path",
        )
        .unwrap()
        .expect("expected version data");

    assert_eq!("dev-latest-testing", version_data.version.unwrap());
    assert_eq!(another_commit_hash, version_data.commit.unwrap());
    assert_eq!(
        "dev-feature",
        version_data.feature_version.as_deref().unwrap()
    );
    assert_eq!(
        "dev-feature",
        version_data.feature_pretty_version.as_deref().unwrap()
    );
}

#[test]
#[serial]
fn test_guess_version_reads_and_respects_non_feature_branches_configuration_for_arbitrary_naming_when_on_non_feature_branch()
 {
    set_up();
    let _td = TearDown;
    let commit_hash = "03a15d220da53c52eddd5f32ffca64a7b3801bea";
    let another_commit_hash = "13a15d220da53c52eddd5f32ffca64a7b3801bea";

    let expectations: Vec<MockExpectation> = vec![
        cmd_full(["git", "--version"], 0, "git version 2.52.0", ""),
        cmd_full(
            ["git", "branch", "-a", "--no-color", "--no-abbrev", "-v"],
            0,
            format!(
                "* latest-testing {} Commit message\n  current {} Another message\n  master {} Another message\n",
                commit_hash, another_commit_hash, another_commit_hash
            ),
            "",
        ),
    ];
    let (process, _guard) = get_process_executor_mock(expectations, true, MockHandler::default());

    let config = make_config();
    let mut guesser = make_guesser(config, process);
    let version_data = guesser
        .guess_version(
            &package_config(&[
                ("version", PhpMixed::String("self.version".to_string())),
                ("non-feature-branches", string_list(&["latest-.*"])),
            ]),
            "dummy/path",
        )
        .unwrap()
        .expect("expected version data");

    assert_eq!("dev-latest-testing", version_data.version.unwrap());
    assert_eq!(commit_hash, version_data.commit.unwrap());
    assert!(version_data.feature_version.is_none());
    assert!(version_data.feature_pretty_version.is_none());
}

#[test]
#[serial]
fn test_detached_head_becomes_dev_hash() {
    set_up();
    let _td = TearDown;
    let commit_hash = "03a15d220da53c52eddd5f32ffca64a7b3801bea";

    let expectations: Vec<MockExpectation> = vec![
        cmd_full(["git", "--version"], 0, "git version 2.52.0", ""),
        cmd_full(
            ["git", "branch", "-a", "--no-color", "--no-abbrev", "-v"],
            0,
            format!("* (no branch) {} Commit message\n", commit_hash),
            "",
        ),
        cmd(["git", "describe", "--exact-match", "--tags"]),
    ];
    let (process, _guard) = get_process_executor_mock(expectations, true, MockHandler::default());

    let config = make_config();
    let mut guesser = make_guesser(config, process);
    let version_data = guesser
        .guess_version(&IndexMap::new(), "dummy/path")
        .unwrap()
        .expect("expected version data");

    assert_eq!(
        format!("dev-{}", commit_hash),
        version_data.version.unwrap()
    );
}

#[test]
#[serial]
fn test_detached_fetch_head_becomes_dev_hash_git2() {
    set_up();
    let _td = TearDown;
    let commit_hash = "03a15d220da53c52eddd5f32ffca64a7b3801bea";

    let expectations: Vec<MockExpectation> = vec![
        cmd_full(["git", "--version"], 0, "git version 2.52.0", ""),
        cmd_full(
            ["git", "branch", "-a", "--no-color", "--no-abbrev", "-v"],
            0,
            format!(
                "* (HEAD detached at FETCH_HEAD) {} Commit message\n",
                commit_hash
            ),
            "",
        ),
        cmd(["git", "describe", "--exact-match", "--tags"]),
    ];
    let (process, _guard) = get_process_executor_mock(expectations, true, MockHandler::default());

    let config = make_config();
    let mut guesser = make_guesser(config, process);
    let version_data = guesser
        .guess_version(&IndexMap::new(), "dummy/path")
        .unwrap()
        .expect("expected version data");

    assert_eq!(
        format!("dev-{}", commit_hash),
        version_data.version.unwrap()
    );
}

#[test]
#[serial]
fn test_detached_commit_head_becomes_dev_hash_git2() {
    set_up();
    let _td = TearDown;
    let commit_hash = "03a15d220da53c52eddd5f32ffca64a7b3801bea";

    let expectations: Vec<MockExpectation> = vec![
        cmd_full(["git", "--version"], 0, "git version 2.52.0", ""),
        cmd_full(
            ["git", "branch", "-a", "--no-color", "--no-abbrev", "-v"],
            0,
            format!(
                "* (HEAD detached at 03a15d220) {} Commit message\n",
                commit_hash
            ),
            "",
        ),
        cmd(["git", "describe", "--exact-match", "--tags"]),
    ];
    let (process, _guard) = get_process_executor_mock(expectations, true, MockHandler::default());

    let config = make_config();
    let mut guesser = make_guesser(config, process);
    let version_data = guesser
        .guess_version(&IndexMap::new(), "dummy/path")
        .unwrap()
        .expect("expected version data");

    assert_eq!(
        format!("dev-{}", commit_hash),
        version_data.version.unwrap()
    );
}

#[test]
#[serial]
fn test_tag_becomes_version() {
    set_up();
    let _td = TearDown;

    let expectations: Vec<MockExpectation> = vec![
        cmd_full(["git", "--version"], 0, "git version 2.52.0", ""),
        cmd_full(
            ["git", "branch", "-a", "--no-color", "--no-abbrev", "-v"],
            0,
            "* (HEAD detached at v2.0.5-alpha2) 433b98d4218c181bae01865901aac045585e8a1a Commit message\n",
            "",
        ),
        cmd_full(
            ["git", "describe", "--exact-match", "--tags"],
            0,
            "v2.0.5-alpha2",
            "",
        ),
    ];
    let (process, _guard) = get_process_executor_mock(expectations, true, MockHandler::default());

    let config = make_config();
    let mut guesser = make_guesser(config, process);
    let version_data = guesser
        .guess_version(&IndexMap::new(), "dummy/path")
        .unwrap()
        .expect("expected version data");

    assert_eq!("2.0.5.0-alpha2", version_data.version.unwrap());
}

#[test]
#[serial]
fn test_tag_becomes_pretty_version() {
    set_up();
    let _td = TearDown;

    let expectations: Vec<MockExpectation> = vec![
        cmd_full(["git", "--version"], 0, "git version 2.52.0", ""),
        cmd_full(
            ["git", "branch", "-a", "--no-color", "--no-abbrev", "-v"],
            0,
            "* (HEAD detached at 1.0.0) c006f0c12bbbf197b5c071ffb1c0e9812bb14a4d Commit message\n",
            "",
        ),
        cmd_full(
            ["git", "describe", "--exact-match", "--tags"],
            0,
            "1.0.0",
            "",
        ),
    ];
    let (process, _guard) = get_process_executor_mock(expectations, true, MockHandler::default());

    let config = make_config();
    let mut guesser = make_guesser(config, process);
    let version_data = guesser
        .guess_version(&IndexMap::new(), "dummy/path")
        .unwrap()
        .expect("expected version data");

    assert_eq!("1.0.0.0", version_data.version.unwrap());
    assert_eq!("1.0.0", version_data.pretty_version.unwrap());
}

#[test]
#[serial]
fn test_invalid_tag_becomes_version() {
    set_up();
    let _td = TearDown;

    let expectations: Vec<MockExpectation> = vec![
        cmd_full(["git", "--version"], 0, "git version 2.52.0", ""),
        cmd_full(
            ["git", "branch", "-a", "--no-color", "--no-abbrev", "-v"],
            0,
            "* foo 03a15d220da53c52eddd5f32ffca64a7b3801bea Commit message\n",
            "",
        ),
    ];
    let (process, _guard) = get_process_executor_mock(expectations, true, MockHandler::default());

    let config = make_config();
    let mut guesser = make_guesser(config, process);
    let version_data = guesser
        .guess_version(&IndexMap::new(), "dummy/path")
        .unwrap()
        .expect("expected version data");

    assert_eq!("dev-foo", version_data.version.unwrap());
}

#[test]
#[serial]
fn test_numeric_branches_show_nicely() {
    set_up();
    let _td = TearDown;

    let expectations: Vec<MockExpectation> = vec![
        cmd_full(["git", "--version"], 0, "git version 2.52.0", ""),
        cmd_full(
            ["git", "branch", "-a", "--no-color", "--no-abbrev", "-v"],
            0,
            "* 1.5 03a15d220da53c52eddd5f32ffca64a7b3801bea Commit message\n",
            "",
        ),
    ];
    let (process, _guard) = get_process_executor_mock(expectations, true, MockHandler::default());

    let config = make_config();
    let mut guesser = make_guesser(config, process);
    let version_data = guesser
        .guess_version(&IndexMap::new(), "dummy/path")
        .unwrap()
        .expect("expected version data");

    assert_eq!("1.5.x-dev", version_data.pretty_version.unwrap());
    assert_eq!("1.5.9999999.9999999-dev", version_data.version.unwrap());
}

#[test]
#[serial]
fn test_remote_branches_are_selected() {
    set_up();
    let _td = TearDown;

    let expectations: Vec<MockExpectation> = vec![
        cmd_full(["git", "--version"], 0, "git version 2.52.0", ""),
        cmd_full(
            ["git", "branch", "-a", "--no-color", "--no-abbrev", "-v"],
            0,
            "* feature-branch 03a15d220da53c52eddd5f32ffca64a7b3801bea Commit message\n\
             remotes/origin/1.5 03a15d220da53c52eddd5f32ffca64a7b3801bea Commit message\n",
            "",
        ),
        cmd_full(
            ["git", "rev-list", "remotes/origin/1.5..feature-branch"],
            0,
            "\n",
            "",
        ),
    ];
    let (process, _guard) = get_process_executor_mock(expectations, true, MockHandler::default());

    let config = make_config();
    let mut guesser = make_guesser(config, process);
    let version_data = guesser
        .guess_version(
            &package_config(&[("version", PhpMixed::String("self.version".to_string()))]),
            "dummy/path",
        )
        .unwrap()
        .expect("expected version data");

    assert_eq!("1.5.x-dev", version_data.pretty_version.unwrap());
    assert_eq!("1.5.9999999.9999999-dev", version_data.version.unwrap());
}

#[test]
#[serial]
fn test_get_root_version_from_env() {
    // @dataProvider rootEnvVersionsProvider
    let root_env_versions: Vec<(&str, &str)> = vec![
        ("1.0-dev", "1.0.x-dev"),
        ("1.0.x-dev", "1.0.x-dev"),
        ("1-dev", "1.x-dev"),
        ("1.x-dev", "1.x-dev"),
        ("1.0.0", "1.0.0"),
    ];

    for (env, expected_version) in root_env_versions {
        Platform::put_env("COMPOSER_ROOT_VERSION", env);
        let config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(true, None)));
        let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(None)));
        let guesser = VersionGuesser::new(config, process, VersionParser::new(), None);
        assert_eq!(
            expected_version,
            guesser.get_root_version_from_env().unwrap()
        );
        Platform::clear_env("COMPOSER_ROOT_VERSION");
    }
}
