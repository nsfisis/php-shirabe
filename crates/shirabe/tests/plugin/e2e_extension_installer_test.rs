//! Real-plugin E2E compatibility check: upstream Composer and Shirabe each
//! run `install` on a pristine copy of the pinned phpstan/extension-installer fixture project
//! at the same filesystem path, and every produced artifact (composer.lock and the whole
//! vendor tree, including the plugin-generated GeneratedConfig.php) must match byte for byte.
//!
//! Prerequisites: the PHP runtime, the Composer checkout, and the plugin under test in
//! `fixtures/e2e/ext/` — run `fixtures/e2e/fetch` once to populate it. The test skips while
//! any of these is missing. Test runs themselves are offline: the fixture project resolves
//! everything from local repositories.

use crate::php_worker::{lock_php_worker, php_runtime_available};
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{bconcat, getenv, os_str_from_bytes, path_to_bytes};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e")
}

/// The upstream Composer checkout used as the comparison oracle (and as the PHP runtime of
/// the worker). Absent checkout means the oracle cannot run; the test returns early,
/// following the convention of the non-mock tests in `shirabe-php-rpc`.
pub(crate) fn upstream_composer_bin() -> Option<PathBuf> {
    let root = match getenv("SHIRABE_COMPOSER_PHP_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../composer"),
    };
    let bin = root.join("bin/composer");
    if bin.is_file() && root.join("vendor/autoload.php").is_file() {
        Some(bin)
    } else {
        None
    }
}

pub(crate) fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// Every file under `dir` as relative path => contents.
fn snapshot_tree(dir: &Path) -> IndexMap<BString, Vec<u8>> {
    let mut files = IndexMap::new();
    fn walk(root: &Path, dir: &Path, files: &mut IndexMap<BString, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                walk(root, &entry.path(), files);
            } else {
                let path = entry.path();
                let relative = BString::from(path_to_bytes(path.strip_prefix(root).unwrap()));
                files.insert(relative, std::fs::read(entry.path()).unwrap());
            }
        }
    }
    walk(dir, dir, &mut files);
    files
}

struct InstallRun {
    exit_code: i32,
    /// The plugin's own IO lines, order preserved (progress rendering differs between the two
    /// implementations and is not compared).
    plugin_output: Vec<BString>,
    artifacts: IndexMap<BString, Vec<u8>>,
}

fn run_install(work: &Path, program: &BStr, args: &[&std::ffi::OsStr]) -> InstallRun {
    let project = work.join("project");
    copy_dir(&fixture_dir(), work);
    let output = std::process::Command::new(os_str_from_bytes(&program))
        .args(args)
        .arg("install")
        .current_dir(&project)
        .env("COMPOSER_HOME", work.join("home"))
        .env("COMPOSER_CACHE_DIR", work.join("cache"))
        .env("COMPOSER_NO_INTERACTION", "1")
        .output()
        .unwrap();
    let mut artifacts = snapshot_tree(&project.join("vendor"));
    artifacts.insert(
        "composer.lock".into(),
        std::fs::read(project.join("composer.lock")).unwrap_or_default(),
    );
    // Directory walk order is filesystem-dependent; a canonical order keys the comparison.
    artifacts.sort_keys();
    let text = bconcat!(&output.stdout, &output.stderr);
    let plugin_output = text
        .lines()
        .filter(|line| line.starts_with(b"phpstan/extension-installer:") || line.starts_with(b"> "))
        .map(BString::from)
        .collect();
    // The next run reuses the same path so absolute paths embedded in the artifacts
    // (GeneratedConfig.php's install_path) compare byte for byte.
    std::fs::remove_dir_all(&project).unwrap();
    InstallRun {
        exit_code: output.status.code().unwrap_or(-1),
        plugin_output,
        artifacts,
    }
}

#[test]
fn test_extension_installer_install_matches_upstream_composer() {
    if !php_runtime_available() {
        return;
    }
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    // Populated by `fixtures/e2e/fetch` (network, one-off).
    if !fixture_dir()
        .join("ext/phpstan-extension-installer-1.4.3/src/Plugin.php")
        .is_file()
    {
        return;
    }
    let _worker = lock_php_worker();

    let work = TempDir::new().unwrap();
    let upstream = run_install(work.path(), BStr::new("php"), &[composer_bin.as_os_str()]);
    let shirabe = run_install(work.path(), BStr::new(env!("CARGO_BIN_EXE_shirabe")), &[]);

    assert_eq!(0, upstream.exit_code, "upstream Composer must succeed");
    assert_eq!(upstream.exit_code, shirabe.exit_code);
    assert_eq!(upstream.plugin_output, shirabe.plugin_output);
    assert_eq!(
        vec![
            "phpstan/extension-installer: Extensions installed",
            "> acme/extension: installed",
            "> acme/phpstan-tools: not supported",
        ],
        upstream.plugin_output
    );

    let upstream_files: Vec<&BString> = upstream.artifacts.keys().collect();
    let shirabe_files: Vec<&BString> = shirabe.artifacts.keys().collect();
    assert_eq!(upstream_files, shirabe_files);
    for (path, contents) in &upstream.artifacts {
        assert_eq!(
            contents,
            &shirabe.artifacts[path],
            "artifact `{}` differs between upstream Composer and Shirabe",
            path.display_lossy()
        );
    }
}
