//! composer/installers E2E compatibility check: upstream Composer and Shirabe each run the same
//! command sequences on a project whose packages are placed by the plugin's `LibraryInstaller`
//! subclass, and the resulting output and project trees are compared.
//!
//! The plugin is fetched by `fixtures/e2e-installers/fetch` into a git-ignored directory; the
//! test skips itself while that directory, the PHP runtime or the Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::php_worker::{lock_php_worker, php_runtime_available};
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

/// The two implementations under comparison: upstream Composer first, Shirabe second.
fn implementations(composer_bin: &str) -> [(&str, Vec<&str>); 2] {
    [
        ("php", vec![composer_bin]),
        (env!("CARGO_BIN_EXE_shirabe"), vec![]),
    ]
}

struct CommandRun {
    exit_code: i32,
    stdout: Vec<String>,
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
        stdout: normalize_output(&String::from_utf8_lossy(&output.stdout), work),
    }
}

/// Output lines the two implementations must agree on: the working directory (which the plugin
/// prints absolutely when it deletes a package) is replaced by a placeholder, and progress bar
/// frames are dropped — Shirabe renders those differently from upstream for every install,
/// including projects with no plugin at all, so they say nothing about this plugin.
fn normalize_output(stdout: &str, work: &Path) -> Vec<String> {
    let work = work.to_str().unwrap();
    stdout
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !(trimmed.starts_with(|c: char| c.is_ascii_digit()) && trimmed.contains('['))
        })
        .map(|line| line.replace(work, "<work>"))
        .collect()
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

fn edit(path: &Path, from: &str, to: &str) {
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.contains(from), "{path:?} does not contain {from:?}");
    std::fs::write(path, text.replace(from, to)).unwrap();
}

// TODO(plugin): the plugin's Installer extends Composer\Installer\LibraryInstaller, whose
// constructor the guard class the worker loads rejects with an explicit error: the Rust side owns
// LibraryInstaller and has no proxy a plugin can subclass. The same applies to the two tests
// below.
#[ignore = "LibraryInstaller is Rust-owned and has no proxy a plugin can subclass; see the TODO(plugin) above"]
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

    let mut results = Vec::new();
    for (program, prefix) in &implementations(&composer_bin) {
        let work = TempDir::new().unwrap();
        copy_dir(&fixture_dir(), work.path());
        let run = run_command(work.path(), program, prefix, &["install"]);
        let tree = tree(&work.path().join("project"));
        results.push((run, tree, work));
    }
    let [upstream, shirabe] = <[_; 2]>::try_from(results).ok().unwrap();

    assert_eq!(0, upstream.0.exit_code, "upstream install must succeed");
    assert_eq!(upstream.0.exit_code, shirabe.0.exit_code);
    assert_eq!(upstream.0.stdout, shirabe.0.stdout);

    // `acme/hello-module` and `acme/hello-theme` land under modules/ and themes/ rather than
    // vendor/, which only the plugin-provided installer's getInstallPath can decide.
    assert!(
        shirabe
            .1
            .iter()
            .any(|(path, _)| path == "modules/hello-module/composer.json"),
        "the plugin-provided installer must place the module outside vendor/"
    );
    assert_eq!(upstream.1, shirabe.1);
}

/// The rest of the installer contract: `update` reinstalls a package in place, a second `install`
/// runs over an already-installed tree, and `remove` reaches the plugin's own `uninstall()`
/// override — the one that chains onto the promise `LibraryInstaller::uninstall` returns.
#[ignore = "LibraryInstaller is Rust-owned and has no proxy a plugin can subclass; see the TODO(plugin) above"]
#[test]
fn test_composer_installers_update_and_remove_match_upstream_composer() {
    if !php_runtime_available() || !plugin_fetched() {
        return;
    }
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();
    let composer_bin = composer_bin.to_str().unwrap().to_string();

    let mut results = Vec::new();
    for (program, prefix) in &implementations(&composer_bin) {
        let work = TempDir::new().unwrap();
        copy_dir(&fixture_dir(), work.path());
        let install = run_command(work.path(), program, prefix, &["install"]);

        let module = work.path().join("packages/hello-module");
        edit(
            &module.join("composer.json"),
            "\"version\": \"1.0.0\"",
            "\"version\": \"1.1.0\"",
        );
        std::fs::write(module.join("src/hello.txt"), "hello module payload v1.1\n").unwrap();
        edit(
            &work.path().join("project/composer.json"),
            "\"acme/hello-module\": \"1.0.0\"",
            "\"acme/hello-module\": \"1.1.0\"",
        );

        let update = run_command(
            work.path(),
            program,
            prefix,
            &["update", "acme/hello-module"],
        );
        let reinstall = run_command(work.path(), program, prefix, &["install"]);
        let remove = run_command(
            work.path(),
            program,
            prefix,
            &["remove", "acme/hello-theme"],
        );
        let tree = tree(&work.path().join("project"));
        results.push((install, update, reinstall, remove, tree, work));
    }
    let [upstream, shirabe] = <[_; 2]>::try_from(results).ok().unwrap();

    for (label, upstream, shirabe) in [
        ("install", &upstream.0, &shirabe.0),
        ("update", &upstream.1, &shirabe.1),
        ("reinstall", &upstream.2, &shirabe.2),
        ("remove", &upstream.3, &shirabe.3),
    ] {
        assert_eq!(0, upstream.exit_code, "upstream {label} must succeed");
        assert_eq!(upstream.exit_code, shirabe.exit_code, "{label} exit code");
        assert_eq!(upstream.stdout, shirabe.stdout, "{label} output");
    }
    assert_eq!(upstream.4, shirabe.4);
}

/// The plugin's configuration surface: `installer-paths` in the root package's extra (both the
/// `type:` and the package-name matcher, with `{$name}` templating) and `installer-name` in the
/// installed package's own extra. Both are read back through the package proxy.
#[ignore = "LibraryInstaller is Rust-owned and has no proxy a plugin can subclass; see the TODO(plugin) above"]
#[test]
fn test_composer_installers_custom_paths_match_upstream_composer() {
    if !php_runtime_available() || !plugin_fetched() {
        return;
    }
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();
    let composer_bin = composer_bin.to_str().unwrap().to_string();

    let mut results = Vec::new();
    for (program, prefix) in &implementations(&composer_bin) {
        let work = TempDir::new().unwrap();
        copy_dir(&fixture_dir(), work.path());
        edit(
            &work.path().join("project/composer.json"),
            "    \"require\": {",
            "    \"extra\": {\n        \"installer-paths\": {\n            \
             \"web/custom/{$name}/\": [\"type:drupal-theme\"],\n            \
             \"web/mods/{$name}/\": [\"acme/hello-module\"]\n        }\n    },\n    \"require\": {",
        );
        edit(
            &work.path().join("packages/hello-module/composer.json"),
            "    \"type\": \"drupal-module\",",
            "    \"type\": \"drupal-module\",\n    \"extra\": {\"installer-name\": \"renamed-mod\"},",
        );
        let run = run_command(work.path(), program, prefix, &["install"]);
        let tree = tree(&work.path().join("project"));
        results.push((run, tree, work));
    }
    let [upstream, shirabe] = <[_; 2]>::try_from(results).ok().unwrap();

    assert_eq!(0, upstream.0.exit_code, "upstream install must succeed");
    assert_eq!(upstream.0.exit_code, shirabe.0.exit_code);
    assert_eq!(upstream.0.stdout, shirabe.0.stdout);
    for expected in [
        "web/custom/hello-theme/composer.json",
        "web/mods/renamed-mod/composer.json",
    ] {
        assert!(
            shirabe.1.iter().any(|(path, _)| path == expected),
            "expected {expected} in {:?}",
            shirabe.1.iter().map(|(path, _)| path).collect::<Vec<_>>()
        );
    }
    assert_eq!(upstream.1, shirabe.1);
}
