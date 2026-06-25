//! ref: composer/tests/Composer/Test/Command/AboutCommandTest.php

use crate::test_case::{RunOptions, get_application_tester};
use serial_test::serial;
use shirabe::composer;
use shirabe_php_shim::PhpMixed;

#[test]
#[serial]
fn test_about() {
    let composer_version = composer::get_version();
    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![(PhpMixed::from("command"), PhpMixed::from("about"))],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code);

    assert!(app_tester.get_display().contains(&format!(
        "Composer - Dependency Manager for PHP - version {composer_version}"
    )));

    assert!(app_tester.get_display().contains(
        "Composer is a dependency manager tracking local dependencies of your projects and libraries."
    ));
    assert!(
        app_tester
            .get_display()
            .contains("See https://getcomposer.org/ for more information.")
    );
}
