//! ref: composer/tests/Composer/Test/Command/StatusCommandTest.php

use crate::test_case::{
    RunOptions, create_composer_lock, create_installed_json, get_application_tester,
    get_complete_package, init_temp_composer,
};
use serial_test::serial;
use shirabe::package::handle::PackageInterfaceHandle;
use shirabe_php_shim::PhpMixed;

#[test]
#[serial]
fn test_no_local_changes() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({ "require": { "root/req": "1.*" } })),
        None,
        None,
        true,
    );

    let package = get_complete_package("root/req", "1.0.0");
    package.__set_type("metapackage".to_string());

    let packages: Vec<PackageInterfaceHandle> = vec![package.into()];

    create_composer_lock(&packages, &[]);
    create_installed_json(&packages, &[], true);

    let mut app_tester = get_application_tester();
    app_tester
        .run(
            vec![(PhpMixed::from("command"), PhpMixed::from("status"))],
            RunOptions::default(),
        )
        .unwrap();

    assert_eq!("No local changes", app_tester.get_display().trim());

    drop(tear_down);
}

#[ignore = "exercises `install` over the network (downloads composer/class-map-generator from a git \
            source or smarty/smarty from a dist zip), then mutates the installed package and runs \
            `status`; the install path needs real network access"]
#[test]
fn test_locally_modified_packages() {
    todo!()
}
