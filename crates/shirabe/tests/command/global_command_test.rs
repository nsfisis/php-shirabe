//! ref: composer/tests/Composer/Test/Command/GlobalCommandTest.php

use shirabe::util::platform::Platform;

fn tear_down() {
    Platform::clear_env("COMPOSER_HOME");
    Platform::clear_env("COMPOSER");
}

struct TearDown;

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down();
    }
}

#[test]
#[ignore = "requires the ApplicationTester/initTempComposer harness, which is not yet ported"]
fn test_global() {
    let _tear_down = TearDown;

    todo!()
}

#[test]
#[ignore = "requires the ApplicationTester/initTempComposer harness, which is not yet ported"]
fn test_cannot_create_home() {
    let _tear_down = TearDown;

    todo!()
}

#[test]
#[ignore = "requires the ApplicationTester/initTempComposer harness, which is not yet ported"]
fn test_global_show() {
    let _tear_down = TearDown;

    todo!()
}

#[test]
#[ignore = "requires the ApplicationTester/initTempComposer harness, which is not yet ported"]
fn test_global_show_without_packages() {
    let _tear_down = TearDown;

    todo!()
}

#[test]
#[ignore = "requires the ApplicationTester/initTempComposer harness, which is not yet ported"]
fn test_global_require() {
    let _tear_down = TearDown;

    todo!()
}

#[test]
#[ignore = "requires the ApplicationTester/initTempComposer harness, which is not yet ported"]
fn test_global_update() {
    let _tear_down = TearDown;

    todo!()
}

#[test]
#[ignore = "requires the ApplicationTester/initTempComposer harness, which is not yet ported"]
fn test_global_changes_directory() {
    let _tear_down = TearDown;

    todo!()
}

#[test]
#[ignore = "requires the ApplicationTester/initTempComposer harness, which is not yet ported"]
fn test_global_missing_command_name() {
    let _tear_down = TearDown;

    todo!()
}
