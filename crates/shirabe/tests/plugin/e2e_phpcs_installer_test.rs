//! dealerdirect/phpcodesniffer-composer-installer E2E compatibility check: upstream Composer and
//! Shirabe each run the same command sequence on a project with a `phpcodesniffer-standard`
//! package, and the output and project trees are compared. The plugin registers the standard with
//! PHP_CodeSniffer at post-install/post-update by running `phpcs --config-set installed_paths`.
//!
//! The plugin and PHP_CodeSniffer are fetched by `fixtures/e2e-phpcs-installer/fetch` into a
//! git-ignored directory; the test skips itself while either of them, the PHP runtime or the
//! Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::e2e_installers_test::{implementations, run_command, tree};
use crate::php_worker::lock_php_worker;
use bstr::BStr;

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/plugin/fixtures/e2e-phpcs-installer")
}

fn plugin_fetched() -> bool {
    let ext = fixture_dir().join("ext");
    ext.join("phpcodesniffer-composer-installer-1.2.1/src/Plugin.php")
        .is_file()
        && ext.join("php_codesniffer-4.0.4/bin/phpcs").is_file()
}

/// `install` registers the standard, a second `install` finds nothing to change, and `remove`ing
/// the standard makes the plugin drop the stale path and delete the setting.
#[test]
fn test_phpcs_installer_matches_upstream_composer() {
    if !plugin_fetched() {
        return;
    }
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();

    let mut results = Vec::new();
    for (program, prefix) in &implementations(composer_bin.as_os_str()) {
        let work = tempfile::TempDir::new().unwrap();
        copy_dir(&fixture_dir(), work.path());
        let install = run_command(
            work.path(),
            program,
            prefix,
            &[BStr::new("install"), BStr::new("-v")],
        );
        let config = std::fs::read(
            work.path()
                .join("project/vendor/squizlabs/php_codesniffer/CodeSniffer.conf"),
        )
        .ok();
        let reinstall = run_command(
            work.path(),
            program,
            prefix,
            &[BStr::new("install"), BStr::new("-v")],
        );
        let remove = run_command(
            work.path(),
            program,
            prefix,
            &[
                BStr::new("remove"),
                BStr::new("acme/coding-standard"),
                BStr::new("-v"),
            ],
        );
        let tree = tree(&work.path().join("project"));
        results.push((install, reinstall, remove, config, tree, work));
    }
    let [upstream, shirabe] = <[_; 2]>::try_from(results).ok().unwrap();

    for (label, upstream, shirabe) in [
        ("install", &upstream.0, &shirabe.0),
        ("reinstall", &upstream.1, &shirabe.1),
        ("remove", &upstream.2, &shirabe.2),
    ] {
        assert_eq!(0, upstream.exit_code, "upstream {label} must succeed");
        assert_eq!(upstream.exit_code, shirabe.exit_code, "{label} exit code");
        assert_eq!(upstream.stdout, shirabe.stdout, "{label} output");
    }
    assert_eq!(
        Some(
            b"<?php\n $phpCodeSnifferConfig = array (\n  'installed_paths' => '../../acme/coding-standard',\n);\n?>"
                .to_vec()
        ),
        shirabe.3,
        "the plugin must register the standard with PHP_CodeSniffer"
    );
    assert_eq!(upstream.3, shirabe.3);
    assert_eq!(upstream.4, shirabe.4);
}
