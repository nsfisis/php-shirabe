//! ref: composer/tests/Composer/Test/Command/HomeCommandTest.php

#[test]
#[ignore = "needs create_installed_json helper (InstalledFilesystemRepository) plus \
            setHomepage on CompletePackage, and HomeCommand::initialize_repos calls try_composer \
            -> Factory, reaching ProcessExecutor (git) -> shirabe-php-shim stream_set_blocking \
            (stream.rs todo!(), requires fcntl(2))"]
fn test_home_command_with_show_flag() {
    todo!()
}
