//! ref: composer/tests/Composer/Test/Command/AboutCommandTest.php

use crate::test_case::{RunOptions, get_application_tester};
use serial_test::serial;
use shirabe::composer;
use shirabe_php_shim::PhpMixed;

#[test]
#[serial]
fn test_about() {
    let shirabe_version = composer::SHIRABE_VERSION;
    let composer_version = composer::VERSION;
    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![(PhpMixed::from("command"), PhpMixed::from("about"))],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code);

    assert!(app_tester.get_display().contains(&format!(
        "Shirabe - Dependency Manager for PHP - version {shirabe_version} (based on Composer {composer_version})"
    )));

    assert!(app_tester.get_display().contains(
        "Shirabe is a dependency manager tracking local dependencies of your projects and libraries."
    ));
    assert!(
        app_tester
            .get_display()
            .contains("See https://getcomposer.org/ for more information.")
    );
}
