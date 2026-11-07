//! php-http/discovery E2E compatibility check: upstream Composer and Shirabe each run the same
//! command sequence on a project that requires the plugin, and the resulting output and project
//! trees are compared. The plugin's `post-update-cmd` listener walks the local repository for
//! packages that need an HTTP implementation, and its `pre-autoload-dump` listener turns the
//! implementations pinned in the root package's `extra.discovery` into a generated discovery
//! strategy it adds to the root package's class map.
//!
//! The plugin is fetched by `fixtures/e2e-http-discovery/fetch` into a git-ignored directory; the
//! test skips itself while that directory, the PHP runtime or the Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::e2e_installers_test::{implementations, run_command, tree};
use crate::php_worker::lock_php_worker;
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_shim::php_str;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e-http-discovery")
}

fn plugin_fetched() -> bool {
    fixture_dir()
        .join("ext/discovery-1.20.0/src/Composer/Plugin.php")
        .is_file()
}

fn edit(path: &Path, from: &BStr, to: &BStr) {
    let text = BString::from(std::fs::read(path).unwrap());
    assert!(
        text.contains_str(from),
        "{path:?} does not contain {from:?}"
    );
    std::fs::write(path, text.replace(from, to)).unwrap();
}

const PINNED_EXTRA: &BStr = php_str(
    b"    \"extra\": {
        \"discovery\": {
            \"psr/http-factory-implementation\": \"Acme\\\\Psr7\\\\Factory\",
            \"Psr\\\\Http\\\\Client\\\\ClientInterface\": \"Acme\\\\Client\\\\Client\"
        }
    },
",
);

/// `update` and `install` both generate the discovery strategy for the pinned implementations,
/// and a `dump-autoload` after the pins are dropped deletes it again.
#[test]
fn test_http_discovery_matches_upstream_composer() {
    if !plugin_fetched() {
        return;
    }
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();

    let mut results = Vec::new();
    for (program, prefix) in &implementations(composer_bin.as_os_str()) {
        let work = TempDir::new().unwrap();
        copy_dir(&fixture_dir(), work.path());
        let update = run_command(work.path(), program, prefix, &[BStr::new("update")]);
        let update_tree = tree(&work.path().join("project"));
        std::fs::remove_dir_all(work.path().join("project/vendor")).unwrap();
        let install = run_command(work.path(), program, prefix, &[BStr::new("install")]);
        let install_tree = tree(&work.path().join("project"));
        edit(
            &work.path().join("project/composer.json"),
            PINNED_EXTRA,
            BStr::new(""),
        );
        let dump = run_command(work.path(), program, prefix, &[BStr::new("dump-autoload")]);
        let dump_tree = tree(&work.path().join("project"));
        results.push((
            update,
            update_tree,
            install,
            install_tree,
            dump,
            dump_tree,
            work,
        ));
    }
    let [upstream, shirabe] = <[_; 2]>::try_from(results).ok().unwrap();

    for (label, upstream, shirabe) in [
        ("update", &upstream.0, &shirabe.0),
        ("install", &upstream.2, &shirabe.2),
        ("dump-autoload", &upstream.4, &shirabe.4),
    ] {
        assert_eq!(0, upstream.exit_code, "upstream {label} must succeed");
        assert_eq!(upstream.exit_code, shirabe.exit_code, "{label} exit code");
        assert_eq!(upstream.stdout, shirabe.stdout, "{label} output");
    }
    let strategy = "vendor/composer/GeneratedDiscoveryStrategy.php";
    for (label, tree) in [("update", &shirabe.1), ("install", &shirabe.3)] {
        assert!(
            tree.iter().any(|(path, _)| path == strategy),
            "the plugin must generate {strategy} on {label}"
        );
    }
    assert!(
        !shirabe.5.iter().any(|(path, _)| path == strategy),
        "the plugin must delete {strategy} once nothing is pinned"
    );
    assert_eq!(upstream.1, shirabe.1, "update tree");
    assert_eq!(upstream.3, shirabe.3, "install tree");
    assert_eq!(upstream.5, shirabe.5, "dump-autoload tree");
}

// TODO(plugin): with nothing pinned, `acme/client` needs a PSR-17 implementation the project
// lacks, so the plugin's `post-update-cmd` listener adds one to composer.json and looks for the
// running `Composer\Installer` with `debug_backtrace()` to rerun it, then restores composer.json
// when that run fails. The Installer is Rust-owned, so no PHP frame can expose it to the
// backtrace.
#[ignore = "the plugin reruns the Composer\\Installer it finds through debug_backtrace(), which is Rust-owned; see the TODO(plugin) above"]
#[test]
fn test_http_discovery_auto_install_matches_upstream_composer() {
    if !plugin_fetched() {
        return;
    }
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();

    let mut results = Vec::new();
    for (program, prefix) in &implementations(composer_bin.as_os_str()) {
        let work = TempDir::new().unwrap();
        copy_dir(&fixture_dir(), work.path());
        let composer_json = work.path().join("project/composer.json");
        edit(&composer_json, PINNED_EXTRA, BStr::new(""));
        edit(
            &composer_json,
            BStr::new(",\n        \"acme/psr7\": \"1.0.0\""),
            BStr::new(""),
        );
        let update = run_command(work.path(), program, prefix, &[BStr::new("update")]);
        let tree = tree(&work.path().join("project"));
        results.push((update, tree, work));
    }
    let [upstream, shirabe] = <[_; 2]>::try_from(results).ok().unwrap();

    assert_eq!(upstream.0.exit_code, shirabe.0.exit_code);
    assert_eq!(upstream.0.stdout, shirabe.0.stdout);
    assert_eq!(upstream.1, shirabe.1);
}
