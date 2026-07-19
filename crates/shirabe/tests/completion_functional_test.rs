//! ref: composer/tests/Composer/Test/CompletionFunctionalTest.php

#[test]
#[ignore = "CommandCompletionTester (Symfony Console test helper) is not ported, and the test only works inside the Composer dev checkout (its own composer.json/lock + installed vendor dir, plus live Packagist queries for package-name suggestions)"]
fn test_complete() {
    // TODO(phase-d): two blockers. (1) CommandCompletionTester (Symfony Console test helper
    // driving the `|_complete` command) is not implemented in the shirabe-external-packages
    // console port, so the data-provider-driven completion test has no harness to run against.
    // (2) The PHP test's expected suggestions come from the environment it runs in: the Composer
    // dev checkout's own composer.json/composer.lock and installed vendor packages (e.g.
    // `depends ` -> composer/semver, psr/log; `run-script ` -> compile/test/phpstan) and live
    // Packagist API queries for package-name completion (e.g. `archive symfony/http-`). The port
    // has no equivalent fixture environment, so even with a tester the data sets could not be
    // reproduced without altering expected values.
    todo!()
}
