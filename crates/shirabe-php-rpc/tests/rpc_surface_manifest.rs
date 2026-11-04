//! Freshness check for the manifest of the plugin surface served over RPC:
//! `surface-manifest --check` verifies that `crates/shirabe/src/plugin/rpc/surface.tsv` matches
//! what the tool derives from the committed stubs and runtime classes and from the match arms of
//! the Rust dispatchers.
//!
//! The tool needs a PHP interpreter and the stub generator's composer vendor directory. A missing
//! prerequisite fails the test; `just setup` prepares both.

use shirabe_php_shim::path_from_bytes;
use shirabe_symfony_process::PhpExecutableFinder;
use std::path::Path;

#[test]
fn rpc_surface_manifest_is_fresh() {
    let php = PhpExecutableFinder::new()
        .find(false)
        .expect("no PHP interpreter found; the surface manifest tool needs one");
    let tool_dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/plugin-stub-generator");
    assert!(
        tool_dir.join("vendor/autoload.php").is_file(),
        "{} is missing; run `composer install` there or `just setup`",
        tool_dir.join("vendor").display(),
    );

    let output = std::process::Command::new(path_from_bytes(&php))
        .arg(tool_dir.join("surface-manifest"))
        .arg("--check")
        .output()
        .expect("failed to spawn the surface manifest tool");
    assert!(
        output.status.success(),
        "surface-manifest --check failed; regenerate the manifest with \
         scripts/plugin-stub-generator/surface-manifest:\n{}",
        bstr::BStr::new(&output.stderr).display_lossy(),
    );
}
