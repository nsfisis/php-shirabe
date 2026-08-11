//! ref: composer/tests/bootstrap.php

use shirabe::util::platform::Platform;

/// PHPUnit loads `bootstrap.php` once for the entire test run (see `composer/phpunit.xml.dist`'s
/// `bootstrap` attribute), so every PHP test implicitly gets `COMPOSER_TESTS_ARE_RUNNING=1`
/// before it runs. Without it, `Application::do_run` disables interactivity whenever stdin isn't
/// a tty (as it isn't under `cargo test`), so interactive `ApplicationTester` runs silently no-op
/// instead of consuming `set_inputs`.
///
/// TODO(port): this is only wired into `get_application_tester()` (used by the `command` test
/// binary) rather than into every test binary's `main.rs`, unlike PHPUnit's bootstrap which
/// covers the whole suite unconditionally. Rust's libtest has no per-binary setup hook, so a true
/// equivalent needs either the `ctor` crate (new dependency, user decision) or wiring a call into
/// every shared fixture constructor.
pub fn bootstrap() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // PHP: error_reporting(E_ALL) has no counterpart here.

        // PHP: date_default_timezone_set(@date_default_timezone_get());
        shirabe_php_shim::date_default_timezone_set(&shirabe_php_shim::date_default_timezone_get());

        // PHP: require src/bootstrap.php and refresh vendor/composer/InstalledVersions.php.
        // TODO(php-runtime): port remaining bootstrap processes (the src/bootstrap.php include and
        // the InstalledVersions refresh are PHP autoload mechanics with no Rust counterpart yet).

        Platform::put_env("COMPOSER_TESTS_ARE_RUNNING", "1");

        // ensure Windows color support detection does not attempt to use colors
        // as this is dependent on env vars and not actual stream capabilities, see
        // https://github.com/composer/composer/issues/11598
        Platform::put_env("NO_COLOR", "1");

        // symfony/phpunit-bridge sets some default env vars which we do not need polluting the test env
        Platform::clear_env("COMPOSER");
        Platform::clear_env("COMPOSER_VENDOR_DIR");
        Platform::clear_env("COMPOSER_BIN_DIR");
    });
}
