//! Freshness check for the generated proxy stubs: `generate-stubs --check` verifies that the
//! committed stub files and the `STUB_FILES` list in `lib.rs` match what the generator emits
//! from the current Composer checkout and classifier report.
//!
//! The generator needs a PHP interpreter, its composer vendor directory and the classifier
//! report; when any of those is missing the test returns early, following the non-mock test
//! convention of this crate.

use shirabe_external_packages::symfony::process::PhpExecutableFinder;
use std::path::Path;

#[test]
fn generated_stubs_are_fresh() {
    let Some(php) = PhpExecutableFinder::new().find(false) else {
        return;
    };
    let tool_dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/plugin-stub-generator");
    if !tool_dir.join("vendor/autoload.php").is_file() {
        return;
    }
    if !tool_dir
        .join("../plugin-class-classifier/report.json")
        .is_file()
    {
        return;
    }

    let output = std::process::Command::new(php)
        .arg(tool_dir.join("generate-stubs"))
        .arg("--check")
        .output()
        .expect("failed to spawn the stub generator");
    assert!(
        output.status.success(),
        "generate-stubs --check failed; regenerate the stubs with \
         scripts/plugin-stub-generator/generate-stubs:\n{}",
        String::from_utf8_lossy(&output.stderr),
    );
}
