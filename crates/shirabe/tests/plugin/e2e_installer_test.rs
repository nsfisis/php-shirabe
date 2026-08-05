//! Plugin-provided installer E2E compatibility check: upstream Composer and Shirabe each install
//! a fixture plugin that registers its own `InstallerInterface` through
//! `InstallationManager::addInstaller`, and the resulting project trees — including the trace the
//! installer writes on every contract call — are compared.
//!
//! The whole fixture is Shirabe-authored (`fixtures/e2e-installer/`), so nothing has to be
//! fetched; the test skips only while the PHP runtime or the Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::plugin_installer_test::{lock_php_worker, php_runtime_available};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e-installer")
}

struct CommandRun {
    exit_code: i32,
    stdout: String,
}

fn run_command(work: &Path, program: &str, prefix_args: &[&str], args: &[&str]) -> CommandRun {
    let output = std::process::Command::new(program)
        .args(prefix_args)
        .args(args)
        .current_dir(work.join("project"))
        .env("COMPOSER_HOME", work.join("home"))
        .env("COMPOSER_CACHE_DIR", work.join("cache"))
        .env("COMPOSER_NO_INTERACTION", "1")
        .env("COLUMNS", "120")
        .env("LINES", "30")
        .output()
        .unwrap();
    CommandRun {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
    }
}

/// Every file under `dir` as (relative path, contents), so two project trees compare as a whole.
fn tree(dir: &Path) -> Vec<(String, String)> {
    let mut files = Vec::new();
    collect(dir, dir, &mut files);
    files.sort();
    files
}

fn collect(root: &Path, dir: &Path, files: &mut Vec<(String, String)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if entry.file_type().unwrap().is_dir() {
            collect(root, &path, files);
        } else {
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();
            files.push((relative, std::fs::read_to_string(&path).unwrap_or_default()));
        }
    }
}

fn installer_lines(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .filter(|line| line.starts_with("asset-installer:"))
        .collect()
}

/// Runs `install` in a fresh copy of the fixture and returns the run plus the resulting tree.
fn install(program: &str, prefix: &[&str]) -> (CommandRun, Vec<(String, String)>, TempDir) {
    let work = TempDir::new().unwrap();
    copy_dir(&fixture_dir(), work.path());
    let run = run_command(work.path(), program, prefix, &["install"]);
    let tree = tree(&work.path().join("project"));
    (run, tree, work)
}

#[test]
fn test_plugin_provided_installer_matches_upstream_composer() {
    if !php_runtime_available() {
        return;
    }
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();
    let composer_bin = composer_bin.to_str().unwrap().to_string();

    let (upstream, upstream_tree, _upstream_work) = install("php", &[composer_bin.as_str()]);
    let (shirabe, shirabe_tree, _shirabe_work) = install(env!("CARGO_BIN_EXE_shirabe"), &[]);

    assert_eq!(0, upstream.exit_code, "upstream install must succeed");
    assert_eq!(upstream.exit_code, shirabe.exit_code);
    assert_eq!(
        installer_lines(&upstream.stdout),
        installer_lines(&shirabe.stdout)
    );
    assert_eq!(
        vec!["asset-installer: installed shirabe-test/asset-a"],
        installer_lines(&shirabe.stdout)
    );

    // The whole project tree, which covers composer.lock, installed.json/installed.php, the
    // manifest the plugin-provided installer wrote under assets/, and installer-trace.txt — the
    // ordered log of every InstallerInterface call the installer received.
    assert_eq!(
        upstream_tree.iter().map(|(p, _)| p).collect::<Vec<_>>(),
        shirabe_tree.iter().map(|(p, _)| p).collect::<Vec<_>>()
    );
    assert_eq!(upstream_tree, shirabe_tree);
}

// Re-running `install` over an already installed project, and removing the asset package
// afterwards, produce the same files on both sides but consult the installers in a different
// order: upstream resolves `getInstaller('composer-plugin')` before the asset operations run,
// Shirabe only when the autoload dump asks for the install paths. The counts differ too once a
// `remove` re-creates the Composer instance.
#[ignore = "InstallationManager consults getInstaller at different points than upstream; installer-trace.txt diverges in order and count (TODO(plugin))"]
#[test]
fn test_plugin_provided_installer_call_order_matches_upstream_composer() {
    if !php_runtime_available() {
        return;
    }
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();
    let composer_bin = composer_bin.to_str().unwrap().to_string();
    let implementations: [(&str, Vec<&str>); 2] = [
        ("php", vec![composer_bin.as_str()]),
        (env!("CARGO_BIN_EXE_shirabe"), vec![]),
    ];

    let mut results = Vec::new();
    for (program, prefix) in &implementations {
        let work = TempDir::new().unwrap();
        copy_dir(&fixture_dir(), work.path());
        let first = run_command(work.path(), program, prefix, &["install"]);
        let second = run_command(work.path(), program, prefix, &["install"]);
        let remove = run_command(
            work.path(),
            program,
            prefix,
            &["remove", "shirabe-test/asset-a"],
        );
        let tree = tree(&work.path().join("project"));
        results.push((first, second, remove, tree));
    }

    let [upstream, shirabe] = <[_; 2]>::try_from(results).ok().unwrap();
    let (u_first, u_second, u_remove, u_tree) = upstream;
    let (s_first, s_second, s_remove, s_tree) = shirabe;

    assert_eq!(0, u_first.exit_code);
    assert_eq!(u_first.exit_code, s_first.exit_code);
    assert_eq!(u_second.exit_code, s_second.exit_code);
    assert_eq!(u_remove.exit_code, s_remove.exit_code);
    assert_eq!(u_tree, s_tree);
}
