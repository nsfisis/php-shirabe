//! ref: composer/tests/Composer/Test/Command/ReinstallCommandTest.php

use crate::test_case::{
    RunOptions, create_composer_lock, create_installed_json, get_application_tester, get_package,
    init_temp_composer,
};
use serial_test::serial;
use shirabe_php_shim::PhpMixed;

fn input(pairs: Vec<(&str, PhpMixed)>) -> Vec<(PhpMixed, PhpMixed)> {
    pairs
        .into_iter()
        .map(|(k, v)| (PhpMixed::from(k), v))
        .collect()
}

/// ref: ReinstallCommandTest::caseProvider
fn cases() -> Vec<(&'static str, Vec<(&'static str, PhpMixed)>, &'static str)> {
    vec![
        (
            "reinstall a package by name",
            vec![(
                "packages",
                PhpMixed::List(vec![
                    PhpMixed::from("root/req"),
                    PhpMixed::from("root/anotherreq*"),
                ]),
            )],
            "- Removing root/req (1.0.0)
  - Removing root/anotherreq2 (1.0.0)
  - Removing root/anotherreq (1.0.0)
  - Installing root/anotherreq (1.0.0)
  - Installing root/anotherreq2 (1.0.0)
  - Installing root/req (1.0.0)",
        ),
        (
            "reinstall packages by type",
            vec![(
                "--type",
                PhpMixed::List(vec![PhpMixed::from("metapackage")]),
            )],
            "- Removing root/req (1.0.0)
  - Removing root/lala (1.0.0)
  - Removing root/anotherreq2 (1.0.0)
  - Removing root/anotherreq (1.0.0)
  - Installing root/anotherreq (1.0.0)
  - Installing root/anotherreq2 (1.0.0)
  - Installing root/lala (1.0.0)
  - Installing root/req (1.0.0)",
        ),
        (
            "reinstall a package that is not installed",
            vec![(
                "packages",
                PhpMixed::List(vec![PhpMixed::from("root/unknownreq")]),
            )],
            r#"<warning>Pattern "root/unknownreq" does not match any currently installed packages.</warning>
<warning>Found no packages to reinstall, aborting.</warning>"#,
        ),
    ]
}

#[test]
#[serial]
fn test_reinstall_command() {
    for (label, options, expected) in cases() {
        let composer_json = serde_json::json!({
            "require": { "root/req": "1.*" },
            "require-dev": {
                "root/anotherreq": "2.*",
                "root/anotherreq2": "2.*",
                "root/lala": "2.*",
            },
        });
        let _tear_down = init_temp_composer(Some(&composer_json), None, None, true);

        let root_req_package = get_package("root/req", "1.0.0");
        let another_req_package = get_package("root/anotherreq", "1.0.0");
        let another_req_package2 = get_package("root/anotherreq2", "1.0.0");
        let another_req_package3 = get_package("root/lala", "1.0.0");
        root_req_package.__set_type("metapackage".to_string());
        another_req_package.__set_type("metapackage".to_string());
        another_req_package2.__set_type("metapackage".to_string());
        another_req_package3.__set_type("metapackage".to_string());

        let dev = [
            another_req_package.clone(),
            another_req_package2.clone(),
            another_req_package3.clone(),
        ];
        create_composer_lock(std::slice::from_ref(&root_req_package), &dev);
        create_installed_json(std::slice::from_ref(&root_req_package), &dev, true);

        let mut app_tester = get_application_tester();
        let mut args = vec![
            ("command", PhpMixed::from("reinstall")),
            ("--no-progress", PhpMixed::from(true)),
            ("--no-plugins", PhpMixed::from(true)),
        ];
        args.extend(options);
        app_tester.run(input(args), RunOptions::default()).unwrap();

        assert_eq!(expected, app_tester.get_display().trim(), "case: {label}");
    }
}
