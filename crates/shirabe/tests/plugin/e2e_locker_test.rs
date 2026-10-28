//! Locker E2E compatibility check: upstream Composer and Shirabe each run `install` twice in a
//! fixture project whose plugin reads the Locker `Composer::getLocker()` hands out and writes what
//! every call on it reports to a trace file. The first run resolves and writes the lock file, the
//! second replays it. Upstream has no test that drives the Locker from plugin code, so the whole
//! fixture is Shirabe-authored (`fixtures/e2e-locker/`) and nothing has to be fetched; the test
//! skips only while the PHP runtime or the Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::php_worker::{lock_php_worker, php_runtime_available};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e-locker")
}

struct Run {
    exit_codes: Vec<i32>,
    trace: String,
}

/// Runs `install` twice in a fresh copy of the fixture and returns both exit codes with the
/// plugin's trace.
fn install_twice(program: &str, prefix_args: &[&str]) -> Run {
    let work = TempDir::new().unwrap();
    copy_dir(&fixture_dir(), work.path());
    let project = work.path().join("project");
    let exit_codes = (0..2)
        .map(|_| {
            let output = std::process::Command::new(program)
                .args(prefix_args)
                .arg("install")
                .current_dir(&project)
                .env("COMPOSER_HOME", work.path().join("home"))
                .env("COMPOSER_CACHE_DIR", work.path().join("cache"))
                .env("COMPOSER_NO_INTERACTION", "1")
                .env("COLUMNS", "120")
                .env("LINES", "30")
                .output()
                .unwrap();
            output.status.code().unwrap_or(-1)
        })
        .collect();
    Run {
        exit_codes,
        trace: std::fs::read_to_string(project.join("locker-trace.txt")).unwrap_or_default(),
    }
}

#[test]
fn test_plugin_read_locker_matches_upstream_composer() {
    if !php_runtime_available() {
        return;
    }
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();
    let composer_bin = composer_bin.to_str().unwrap().to_string();

    let upstream = install_twice("php", &[composer_bin.as_str()]);
    let shirabe = install_twice(env!("CARGO_BIN_EXE_shirabe"), &[]);

    assert_eq!(
        vec![0, 0],
        upstream.exit_codes,
        "upstream installs must succeed"
    );
    assert_eq!(upstream.exit_codes, shirabe.exit_codes);
    assert_eq!(upstream.trace, shirabe.trace);

    // Pinned as well as compared, so a run where neither side wrote a trace cannot pass.
    assert_eq!(
        "\
installed isLocked=true
event=post-update-cmd
class=Composer\\Package\\Locker same=true
isLocked=true isFresh=true
lockedRepository dev=false class=Composer\\Repository\\LockArrayRepository [\"Composer\\\\Package\\\\CompletePackage:acme\\/lib-a@1.0.0\",\"Composer\\\\Package\\\\CompletePackage:acme\\/lib-b@dev-main\",\"Composer\\\\Package\\\\CompletePackage:shirabe-test\\/locker-probe@1.0.0\",\"Composer\\\\Package\\\\CompleteAliasPackage:acme\\/lib-b@1.0.0\"]
platformRequirements dev=false [\"php=Composer\\\\Package\\\\Link:>=7.0\"]
lockedRepository dev=true class=Composer\\Repository\\LockArrayRepository [\"Composer\\\\Package\\\\CompletePackage:acme\\/lib-a@1.0.0\",\"Composer\\\\Package\\\\CompletePackage:acme\\/lib-b@dev-main\",\"Composer\\\\Package\\\\CompletePackage:shirabe-test\\/locker-probe@1.0.0\",\"Composer\\\\Package\\\\CompletePackage:acme\\/dev-tool@1.0.0\",\"Composer\\\\Package\\\\CompleteAliasPackage:acme\\/lib-b@1.0.0\"]
platformRequirements dev=true [\"php=Composer\\\\Package\\\\Link:>=7.0\",\"ext-json=Composer\\\\Package\\\\Link:*\"]
devPackageNames [\"acme\\/dev-tool\"]
minimumStability \"beta\"
stabilityFlags {\"acme\\/lib-b\":20}
preferStable true
preferLowest false
platformOverrides {\"php\":\"8.1.0\"}
aliases [{\"package\":\"acme\\/lib-b\",\"version\":\"dev-main\",\"alias\":\"1.0.0\",\"alias_normalized\":\"1.0.0.0\"}]
pluginApi \"2.9.0\"
lockData keys [\"_readme\",\"content-hash\",\"packages\",\"packages-dev\",\"aliases\",\"minimum-stability\",\"stability-flags\",\"prefer-stable\",\"prefer-lowest\",\"platform\",\"platform-dev\",\"platform-overrides\",\"plugin-api-version\"]
lockData contentHash matches=true
missingRequirementInfo []
dump withLocker count=1
event=post-install-cmd
class=Composer\\Package\\Locker same=true
isLocked=true isFresh=true
lockedRepository dev=false class=Composer\\Repository\\LockArrayRepository [\"Composer\\\\Package\\\\CompletePackage:acme\\/lib-a@1.0.0\",\"Composer\\\\Package\\\\CompletePackage:acme\\/lib-b@dev-main\",\"Composer\\\\Package\\\\CompletePackage:shirabe-test\\/locker-probe@1.0.0\",\"Composer\\\\Package\\\\CompleteAliasPackage:acme\\/lib-b@1.0.0\"]
platformRequirements dev=false [\"php=Composer\\\\Package\\\\Link:>=7.0\"]
lockedRepository dev=true class=Composer\\Repository\\LockArrayRepository [\"Composer\\\\Package\\\\CompletePackage:acme\\/lib-a@1.0.0\",\"Composer\\\\Package\\\\CompletePackage:acme\\/lib-b@dev-main\",\"Composer\\\\Package\\\\CompletePackage:shirabe-test\\/locker-probe@1.0.0\",\"Composer\\\\Package\\\\CompletePackage:acme\\/dev-tool@1.0.0\",\"Composer\\\\Package\\\\CompleteAliasPackage:acme\\/lib-b@1.0.0\"]
platformRequirements dev=true [\"php=Composer\\\\Package\\\\Link:>=7.0\",\"ext-json=Composer\\\\Package\\\\Link:*\"]
devPackageNames [\"acme\\/dev-tool\"]
minimumStability \"beta\"
stabilityFlags {\"acme\\/lib-b\":20}
preferStable true
preferLowest false
platformOverrides {\"php\":\"8.1.0\"}
aliases [{\"package\":\"acme\\/lib-b\",\"version\":\"dev-main\",\"alias\":\"1.0.0\",\"alias_normalized\":\"1.0.0.0\"}]
pluginApi \"2.9.0\"
lockData keys [\"_readme\",\"content-hash\",\"packages\",\"packages-dev\",\"aliases\",\"minimum-stability\",\"stability-flags\",\"prefer-stable\",\"prefer-lowest\",\"platform\",\"platform-dev\",\"platform-overrides\",\"plugin-api-version\"]
lockData contentHash matches=true
missingRequirementInfo []
dump withLocker count=1
setLockData changed=true minimumStability=\"dev\" preferStable=false preferLowest=true isFresh=true
",
        upstream.trace
    );
}
