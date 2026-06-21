//! ref: composer/tests/Composer/Test/InstallerTest.php

// These are large end-to-end installer integration cases driven by fixtures and a mocked
// Composer/IO/repositories; the full install pipeline (and constraint parsing through a
// look-around regex) is not ported.

use shirabe::util::platform::Platform;

// The chdir back to prevCwd (cwd management) and removeDirectory of tempComposerHome (a
// path produced by the unported install pipeline) are not ported; only the env clears are.
fn tear_down() {
    Platform::clear_env("COMPOSER_POOL_OPTIMIZER");
    Platform::clear_env("COMPOSER_FUND");
}

struct TearDown;

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down();
    }
}

#[test]
#[ignore = "not yet ported (end-to-end Installer integration over fixtures; constraint parsing uses a look-around regex)"]
fn test_installer() {
    let _tear_down = TearDown;
    todo!()
}

#[test]
#[ignore = "not yet ported (end-to-end Installer integration over fixtures; constraint parsing uses a look-around regex)"]
fn test_slow_integration() {
    let _tear_down = TearDown;
    todo!()
}

#[test]
#[ignore = "not yet ported (end-to-end Installer integration over fixtures; constraint parsing uses a look-around regex)"]
fn test_integration_with_pool_optimizer() {
    let _tear_down = TearDown;
    todo!()
}

#[test]
#[ignore = "not yet ported (end-to-end Installer integration over fixtures; constraint parsing uses a look-around regex)"]
fn test_integration_with_raw_pool() {
    let _tear_down = TearDown;
    todo!()
}
