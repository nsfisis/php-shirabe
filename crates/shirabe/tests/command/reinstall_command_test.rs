//! ref: composer/tests/Composer/Test/Command/ReinstallCommandTest.php sha1:7e95f88a54d07278b8d0bde60becb57264152ac0

use crate::test_case::{
    RunOptions, create_composer_lock, create_installed_json, get_application_tester, get_package,
    init_temp_composer,
};
use bstr::{BStr, BString};
use shirabe_php_shim::trim;
use shirabe_symfony_console::input::{InputValue, ParameterName};

fn input(pairs: Vec<(&BStr, InputValue)>) -> Vec<(ParameterName, InputValue)> {
    pairs
        .into_iter()
        .map(|(k, v)| (ParameterName::of(k), v))
        .collect()
}

/// ref: ReinstallCommandTest::caseProvider
fn cases() -> Vec<(
    &'static BStr,
    Vec<(&'static BStr, InputValue)>,
    &'static BStr,
)> {
    vec![
        (
            BStr::new("reinstall a package by name"),
            vec![(
                BStr::new("packages"),
                InputValue::Array(vec!["root/req".into(), "root/anotherreq*".into()]),
            )],
            BStr::new(
                "- Removing root/req (1.0.0)
  - Removing root/anotherreq2 (1.0.0)
  - Removing root/anotherreq (1.0.0)
  - Installing root/anotherreq (1.0.0)
  - Installing root/anotherreq2 (1.0.0)
  - Installing root/req (1.0.0)",
            ),
        ),
        (
            BStr::new("reinstall packages by type"),
            vec![(
                BStr::new("--type"),
                InputValue::Array(vec!["metapackage".into()]),
            )],
            BStr::new(
                "- Removing root/req (1.0.0)
  - Removing root/lala (1.0.0)
  - Removing root/anotherreq2 (1.0.0)
  - Removing root/anotherreq (1.0.0)
  - Installing root/anotherreq (1.0.0)
  - Installing root/anotherreq2 (1.0.0)
  - Installing root/lala (1.0.0)
  - Installing root/req (1.0.0)",
            ),
        ),
        (
            BStr::new("reinstall a package that is not installed"),
            vec![(
                BStr::new("packages"),
                InputValue::Array(vec!["root/unknownreq".into()]),
            )],
            BStr::new(
                r#"<warning>Pattern "root/unknownreq" does not match any currently installed packages.</warning>
<warning>Found no packages to reinstall, aborting.</warning>"#,
            ),
        ),
    ]
}

#[test]
#[serial_test::serial]
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
        root_req_package.__set_type(BString::from("metapackage"));
        another_req_package.__set_type(BString::from("metapackage"));
        another_req_package2.__set_type(BString::from("metapackage"));
        another_req_package3.__set_type(BString::from("metapackage"));

        let dev = [
            another_req_package.clone(),
            another_req_package2.clone(),
            another_req_package3.clone(),
        ];
        create_composer_lock(std::slice::from_ref(&root_req_package), &dev);
        create_installed_json(std::slice::from_ref(&root_req_package), &dev, true);

        let mut app_tester = get_application_tester();
        let mut args = vec![
            (BStr::new("command"), InputValue::from("reinstall")),
            (BStr::new("--no-progress"), InputValue::from(true)),
            (BStr::new("--no-plugins"), InputValue::from(true)),
        ];
        args.extend(options);
        app_tester.run(input(args), RunOptions::default()).unwrap();

        assert_eq!(
            expected,
            trim(app_tester.get_display()),
            "case: {label}",
            label = label.display_lossy()
        );
    }
}
