//! Freshness check for the generated proxy stubs and guard classes: `generate-stubs --check`
//! verifies that the committed stub and guard files, and the `STUB_FILES` list in `lib.rs`, match
//! what the generator emits from the current Composer checkout and classifier report.
//!
//! The generator needs a PHP interpreter, its composer vendor directory and the classifier
//! report. A missing prerequisite fails the test; `just setup` prepares all of them.

use shirabe_symfony_process::PhpExecutableFinder;
use std::path::Path;

#[test]
fn generated_stubs_are_fresh() {
    let php = PhpExecutableFinder::new()
        .find(false)
        .expect("no PHP interpreter found; the stub generator needs one");
    let tool_dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/plugin-stub-generator");
    assert!(
        tool_dir.join("vendor/autoload.php").is_file(),
        "{} is missing; run `composer install` there or `just setup`",
        tool_dir.join("vendor").display(),
    );
    let report = tool_dir.join("../plugin-class-classifier/report.json");
    assert!(
        report.is_file(),
        "{} is missing; run scripts/plugin-class-classifier/classify or `just setup`",
        report.display(),
    );

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
