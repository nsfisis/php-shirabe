//! ref: composer/tests/Composer/Test/AllFunctionalTest.php

// These build the composer.phar and run the .test integration fixtures by invoking the
// composer binary as a subprocess; the phar build and functional-test harness are not
// ported.

// setUp only does cwd management (chdir into Fixtures/functional), which is intentionally
// not ported, so it has no portable body.

// The chdir back to oldcwd is cwd management (not ported); the removeDirectory of testDir
// targets a path produced by the unported functional-test run.
fn tear_down() {
    todo!()
}

struct TearDown;

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down();
    }
}

#[test]
#[ignore = "depends on unported bin/compile phar build (./bin/compile) and running composer.phar as a subprocess"]
fn test_build_phar() {
    let _tear_down = TearDown;
    todo!()
}

#[test]
#[ignore = "depends on unported functional-test harness (parseTestFile/cleanOutput) running the built composer.phar as a subprocess"]
fn test_integration() {
    let _tear_down = TearDown;
    todo!()
}
