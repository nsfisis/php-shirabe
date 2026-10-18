//! Pre-pool-create E2E compatibility check: upstream Composer and Shirabe each install a fixture
//! project and then require one more package, with a plugin subscribed to
//! `PluginEvents::PRE_POOL_CREATE` appending what the event exposes to a trace file and dropping
//! one version of the required package from the pool. Upstream has no test that dispatches the
//! event through a real plugin, so the whole fixture is Shirabe-authored
//! (`fixtures/e2e-pre-pool-create/`) and nothing has to be fetched; the test skips only while the
//! PHP runtime or the Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::php_worker::{lock_php_worker, php_runtime_available};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e-pre-pool-create")
}

struct Run {
    exit_codes: Vec<i32>,
    trace: String,
    composer_json: String,
}

/// Runs `install` and then `require shirabe-test/pool-lib` in a fresh copy of the fixture: the
/// plugin is only active once the first command has installed it, so the event it records is the
/// one the second command dispatches.
fn install_then_require(program: &str, prefix_args: &[&str]) -> Run {
    let work = TempDir::new().unwrap();
    copy_dir(&fixture_dir(), work.path());
    let project = work.path().join("project");
    let commands: [&[&str]; 2] = [&["install"], &["require", "shirabe-test/pool-lib"]];
    let exit_codes = commands
        .iter()
        .map(|args| {
            std::process::Command::new(program)
                .args(prefix_args)
                .args(*args)
                .current_dir(&project)
                .env("COMPOSER_HOME", work.path().join("home"))
                .env("COMPOSER_CACHE_DIR", work.path().join("cache"))
                .env("COMPOSER_NO_INTERACTION", "1")
                .env("COLUMNS", "120")
                .env("LINES", "30")
                .output()
                .unwrap()
                .status
                .code()
                .unwrap_or(-1)
        })
        .collect();
    Run {
        exit_codes,
        trace: std::fs::read_to_string(project.join("pre-pool-create-trace.txt"))
            .unwrap_or_default(),
        composer_json: std::fs::read_to_string(project.join("composer.json")).unwrap(),
    }
}

#[test]
fn test_pre_pool_create_matches_upstream_composer() {
    if !php_runtime_available() {
        return;
    }
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();
    let composer_bin = composer_bin.to_str().unwrap().to_string();

    let upstream = install_then_require("php", &[composer_bin.as_str()]);
    let shirabe = install_then_require(env!("CARGO_BIN_EXE_shirabe"), &[]);

    assert_eq!(
        vec![0, 0],
        upstream.exit_codes,
        "upstream commands must succeed"
    );
    assert_eq!(upstream.exit_codes, shirabe.exit_codes);
    assert_eq!(upstream.trace, shirabe.trace);

    // The solver only picks 1.0.0 when the list the listener filtered is the one it reads.
    assert!(
        shirabe
            .composer_json
            .contains("\"shirabe-test/pool-lib\": \"^1.0\""),
        "{}",
        shirabe.composer_json
    );

    // Pinned as well as compared, so a run where neither side dispatches anything cannot pass.
    assert_eq!(
        "\
pre-pool-create class=Composer\\Plugin\\PrePoolCreateEvent packages=shirabe-test/pool-lib@1.0.0,shirabe-test/pool-lib@2.0.0,shirabe-test/pre-pool-create-recorder@1.0.0,shirabe/e2e-pre-pool-create@1.0.0+no-version-set kept=shirabe-test/pool-lib@1.0.0,shirabe-test/pre-pool-create-recorder@1.0.0,shirabe/e2e-pre-pool-create@1.0.0+no-version-set unacceptable=0 stabilities={\"stable\":0} flags={\"shirabe\\/e2e-pre-pool-create\":0} aliases=[] references=[]
",
        upstream.trace
    );
}
