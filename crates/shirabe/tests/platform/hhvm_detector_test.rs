//! ref: composer/tests/Composer/Test/Platform/HhvmDetectorTest.php

use shirabe::platform::hhvm_detector::HhvmDetector;

fn set_up() -> HhvmDetector {
    let hhvm_detector = HhvmDetector::new(None, None);
    hhvm_detector.reset();
    hhvm_detector
}

#[test]
#[ignore = "skipped unless running under HHVM (HHVM_VERSION_ID defined), which never holds here"]
fn test_hhvm_version_when_executing_in_hhvm() {
    let _hhvm_detector = set_up();
    todo!()
}

#[test]
#[ignore = "needs an installed hhvm executable plus ExecutableFinder/ProcessExecutor; skipped in PHP when hhvm is absent"]
fn test_hhvm_version_when_executing_in_php() {
    let _hhvm_detector = set_up();
    todo!()
}
