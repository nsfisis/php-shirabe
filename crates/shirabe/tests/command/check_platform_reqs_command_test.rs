//! ref: composer/tests/Composer/Test/Command/CheckPlatformReqsCommandTest.php

#[test]
#[ignore = "needs create_installed_json / create_composer_lock helpers (InstalledFilesystemRepository \
            / Locker::set_lock_data), and require_composer -> Factory reaches ProcessExecutor (git) \
            -> shirabe-php-shim stream_set_blocking (stream.rs todo!(), requires fcntl(2))"]
fn test_platform_reqs_are_satisfied() {
    todo!()
}

#[test]
#[ignore = "check-platform-reqs calls require_composer -> Factory, reaching ProcessExecutor (git) \
            -> shirabe-php-shim stream_set_blocking (stream.rs todo!(), requires fcntl(2)) before \
            the missing-lockfile LogicException can be thrown"]
fn test_exception_thrown_if_no_lockfile_found() {
    todo!()
}

#[test]
#[ignore = "needs create_installed_json / create_composer_lock helpers (InstalledFilesystemRepository \
            / Locker::set_lock_data), and require_composer -> Factory reaches ProcessExecutor (git) \
            -> shirabe-php-shim stream_set_blocking (stream.rs todo!(), requires fcntl(2))"]
fn test_failed_platform_requirement() {
    todo!()
}
