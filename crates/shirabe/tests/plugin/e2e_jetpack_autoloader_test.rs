//! automattic/jetpack-autoloader E2E compatibility check: upstream Composer and Shirabe each run
//! the same command sequence on a project whose `post-autoload-dump` listener builds a second
//! autoloader from the package map `Composer::getAutoloadGenerator()` computes, and the resulting
//! output and project trees are compared.
//!
//! The plugin is fetched by `fixtures/e2e-jetpack-autoloader/fetch` into a git-ignored directory;
//! the test skips itself while that directory, the PHP runtime or the Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::e2e_installers_test::{implementations, run_command, tree};
use crate::php_worker::{lock_php_worker, php_runtime_available};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e-jetpack-autoloader")
}

fn plugin_fetched() -> bool {
    fixture_dir()
        .join("ext/jetpack-autoloader-5.0.22/src/CustomAutoloaderPlugin.php")
        .is_file()
}

/// `install` dumps an unoptimized autoloader, and `dump-autoload --optimize` hands the plugin the
/// `optimize` flag, under which it scans the PSR-4 directories into its class map as well.
#[test]
fn test_jetpack_autoloader_matches_upstream_composer() {
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
        let install_tree = tree(&work.path().join("project"));
        let dump = run_command(
            work.path(),
            program,
            prefix,
            &["dump-autoload", "--optimize"],
        );
        let dump_tree = tree(&work.path().join("project"));
        results.push((install, install_tree, dump, dump_tree, work));
    }
    let [upstream, shirabe] = <[_; 2]>::try_from(results).ok().unwrap();

    assert_eq!(0, upstream.0.exit_code, "upstream install must succeed");
    assert_eq!(upstream.0.exit_code, shirabe.0.exit_code);
    assert_eq!(upstream.0.stdout, shirabe.0.stdout);
    // The files only the plugin writes, so a run where it never got past its listener's first
    // call into the generator cannot pass.
    for generated in [
        "vendor/autoload_packages.php",
        "vendor/composer/jetpack_autoload_classmap.php",
        "vendor/composer/jetpack_autoload_psr4.php",
        "vendor/composer/jetpack_autoload_filemap.php",
    ] {
        assert!(
            shirabe.1.iter().any(|(path, _)| path == generated),
            "the plugin must generate {generated}"
        );
    }
    assert_eq!(upstream.1, shirabe.1);

    assert_eq!(
        0, upstream.2.exit_code,
        "upstream dump-autoload must succeed"
    );
    assert_eq!(upstream.2.exit_code, shirabe.2.exit_code);
    assert_eq!(upstream.2.stdout, shirabe.2.stdout);
    assert_ne!(
        shirabe.1, shirabe.3,
        "the optimized dump must change the generated class map"
    );
    assert_eq!(upstream.3, shirabe.3);
}
