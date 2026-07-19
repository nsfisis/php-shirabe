//! ref: composer/tests/bootstrap.php

use shirabe::util::platform::Platform;

/// PHPUnit loads `bootstrap.php` once for the entire test run (see `composer/phpunit.xml.dist`'s
/// `bootstrap` attribute), so every PHP test implicitly gets `COMPOSER_TESTS_ARE_RUNNING=1`
/// before it runs. Without it, `Application::do_run` disables interactivity whenever stdin isn't
/// a tty (as it isn't under `cargo test`), so interactive `ApplicationTester` runs silently no-op
/// instead of consuming `set_inputs`.
///
/// TODO(phase-d): this is only wired into `get_application_tester()` (used by the `command` test
/// binary) rather than into every test binary's `main.rs`, unlike PHPUnit's bootstrap which
/// covers the whole suite unconditionally. Call this from a shared entry point across all test
/// binaries once one exists.
pub fn bootstrap() {
    Platform::put_env("COMPOSER_TESTS_ARE_RUNNING", "1");

    // TODO(phase-d): port remaining bootstrap processes.
}
