//! composer/installers E2E compatibility check: upstream Composer and Shirabe each install a
//! project whose packages are placed by the plugin's `LibraryInstaller` subclass, and the
//! resulting project trees are compared.
//!
//! The plugin is fetched by `fixtures/e2e-installers/fetch` into a git-ignored directory; the
//! test skips itself while that directory, the PHP runtime or the Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::plugin_installer_test::{lock_php_worker, php_runtime_available};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e-installers")
}

fn plugin_fetched() -> bool {
    fixture_dir()
        .join("ext/composer-installers-2.3.0/src/Composer/Installers/Installer.php")
        .is_file()
}

struct CommandRun {
    exit_code: i32,
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

/// Runs `install` in a fresh copy of the fixture and returns the run plus the resulting tree.
fn install(program: &str, prefix: &[&str]) -> (CommandRun, Vec<(String, String)>, TempDir) {
    let work = TempDir::new().unwrap();
    copy_dir(&fixture_dir(), work.path());
    let run = run_command(work.path(), program, prefix, &["install"]);
    let tree = tree(&work.path().join("project"));
    (run, tree, work)
}

#[test]
fn test_composer_installers_matches_upstream_composer() {
    if !php_runtime_available() || !plugin_fetched() {
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

    // The paths alone carry the plugin's whole contribution: `acme/hello-module` and
    // `acme/hello-theme` land under modules/ and themes/ rather than vendor/, which only the
    // plugin-provided installer's getInstallPath can decide.
    let paths = |files: &[(String, String)]| -> Vec<String> {
        files
            .iter()
            .map(|(path, _)| path.clone())
            .filter(|path| !path.starts_with("vendor/composer/installers/"))
            .collect()
    };
    assert!(
        paths(&shirabe_tree).contains(&"modules/hello-module/composer.json".to_string()),
        "the plugin-provided installer must place the module outside vendor/: {:?}",
        paths(&shirabe_tree)
    );
    assert_eq!(paths(&upstream_tree), paths(&shirabe_tree));
    assert_eq!(upstream_tree, shirabe_tree);
}
