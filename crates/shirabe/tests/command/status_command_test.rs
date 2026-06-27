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

#[ignore = "not a partial-mock test: it runs `install` to download composer/class-map-generator (git \
            source) or smarty/smarty (dist zip), mutates the installed package, then runs `status`. \
            Porting without network would require fabricating a real git checkout under vendor/, a \
            `.git` metadata repo, and wiring download-manager routing so GitDownloader::get_local_changes \
            runs `git status` against it — a full VCS integration fixture, not a SUT seam"]
#[test]
fn test_locally_modified_packages() {
    todo!()
}
