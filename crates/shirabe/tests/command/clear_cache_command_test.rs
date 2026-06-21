//! ref: composer/tests/Composer/Test/Command/ClearCacheCommandTest.php

use shirabe::util::platform::Platform;

fn tear_down() {
    // --no-cache triggers the env to change so make sure the env is cleaned up after these tests run
    Platform::clear_env("COMPOSER_CACHE_DIR");
}

struct TearDown;

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down();
    }
}

#[test]
#[ignore = "requires the ApplicationTester/initTempComposer harness, which is not yet ported"]
fn test_clear_cache_command_success() {
    let _tear_down = TearDown;

    todo!()
}

#[test]
#[ignore = "requires the ApplicationTester/initTempComposer harness, which is not yet ported"]
fn test_clear_cache_command_with_option_garbage_collection() {
    let _tear_down = TearDown;

    todo!()
}

#[test]
#[ignore = "requires the ApplicationTester/initTempComposer harness, which is not yet ported"]
fn test_clear_cache_command_with_option_no_cache() {
    let _tear_down = TearDown;

    todo!()
}
