//! ref: composer/tests/Composer/Test/Command/SelfUpdateCommandTest.php

/// Returns the path to the copied composer.phar used by the test bodies.
fn set_up() -> String {
    // Depends on initTempComposer and the composer-test.phar fixture, neither ported yet.
    todo!()
}

#[test]
#[ignore = "depends on initTempComposer, composer-test.phar fixture, and Symfony Process to spawn the phar; none ported"]
fn test_successful_update() {
    let _phar = set_up();

    todo!()
}

#[test]
#[ignore = "depends on initTempComposer, composer-test.phar fixture, and Symfony Process to spawn the phar; none ported"]
fn test_update_to_specific_version() {
    let _phar = set_up();

    todo!()
}

#[test]
#[ignore = "depends on getApplicationTester (ApplicationTester) which is not ported"]
fn test_update_with_invalid_option_throws_exception() {
    let _phar = set_up();

    todo!()
}

#[test]
#[ignore = "depends on initTempComposer, composer-test.phar fixture, and Symfony Process to spawn the phar; none ported"]
fn test_update_to_different_channel() {
    let _phar = set_up();

    todo!()
}
