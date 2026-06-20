//! ref: composer/tests/Composer/Test/Platform/HhvmDetectorTest.php

#[test]
#[ignore = "skipped unless running under HHVM (HHVM_VERSION_ID defined), which never holds here"]
fn test_hhvm_version_when_executing_in_hhvm() {
    todo!()
}

#[test]
#[ignore = "needs an installed hhvm executable plus ExecutableFinder/ProcessExecutor; skipped in PHP when hhvm is absent"]
fn test_hhvm_version_when_executing_in_php() {
    todo!()
}
