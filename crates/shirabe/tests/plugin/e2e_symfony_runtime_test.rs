//! symfony/runtime E2E compatibility check: upstream Composer and Shirabe each run the same
//! command sequence on a project whose `post-autoload-dump` listener writes
//! `vendor/autoload_runtime.php` from the root package's `extra.runtime` options, and the resulting
//! output and project trees are compared.
//!
//! The plugin is fetched by `fixtures/e2e-symfony-runtime/fetch` into a git-ignored directory;
//! the test skips itself while that directory, the PHP runtime or the Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::e2e_installers_test::{implementations, run_command, tree};
use crate::php_worker::lock_php_worker;
use bstr::BStr;

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/plugin/fixtures/e2e-symfony-runtime")
}

fn plugin_fetched() -> bool {
    fixture_dir()
        .join("ext/symfony-runtime-8.1.11/Internal/ComposerPlugin.php")
        .is_file()
}

/// `install` runs the listener once the autoloader is dumped, which resolves the project directory
/// through `Factory::getComposerFile()`, and `dump-autoload` runs it again over the file it wrote.
#[test]
fn test_symfony_runtime_matches_upstream_composer() {
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
        let install = run_command(work.path(), program, prefix, &[BStr::new("install")]);
        let install_tree = tree(&work.path().join("project"));
        let dump = run_command(work.path(), program, prefix, &[BStr::new("dump-autoload")]);
        let dump_tree = tree(&work.path().join("project"));
        results.push((install, install_tree, dump, dump_tree, work));
    }
    let [upstream, shirabe] = <[_; 2]>::try_from(results).ok().unwrap();

    assert_eq!(0, upstream.0.exit_code, "upstream install must succeed");
    assert_eq!(upstream.0.exit_code, shirabe.0.exit_code);
    assert_eq!(upstream.0.stdout, shirabe.0.stdout);
    assert!(
        shirabe
            .1
            .iter()
            .any(|(path, _)| path == "vendor/autoload_runtime.php"),
        "the plugin must generate vendor/autoload_runtime.php"
    );
    assert_eq!(upstream.1, shirabe.1);

    assert_eq!(
        0, upstream.2.exit_code,
        "upstream dump-autoload must succeed"
    );
    assert_eq!(upstream.2.exit_code, shirabe.2.exit_code);
    assert_eq!(upstream.2.stdout, shirabe.2.stdout);
    assert_eq!(upstream.3, shirabe.3);
}
