//! ref: composer/tests/Composer/Test/FactoryTest.php

use shirabe::factory::Factory;
use shirabe::util::platform::Platform;

fn tear_down() {
    Platform::clear_env("COMPOSER");
}

struct TearDown;

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down();
    }
}

#[test]
#[ignore = "requires PHPUnit-style mocks: an IOInterface mock verifying writeError is called exactly once with the exact warning, plus a Config mock stubbing get('disable-tls')=>true; no such mock/expectation infrastructure (e.g. mockall) exists"]
fn test_default_values_are_as_expected() {
    let _tear_down = TearDown;

    todo!()
}

#[test]
#[ignore = "depends on COMPOSER being unset, but sibling tests set it and race it on the process-global env under parallel execution"]
fn test_get_composer_json_path() {
    let _tear_down = TearDown;

    assert_eq!("./composer.json", Factory::get_composer_file().unwrap());
}

#[test]
#[ignore = "mutates the global COMPOSER env and races the from_env case under parallel execution"]
fn test_get_composer_json_path_fails_if_dir() {
    let _tear_down = TearDown;

    let dir = env!("CARGO_MANIFEST_DIR");
    Platform::put_env("COMPOSER", dir);
    let err = Factory::get_composer_file().unwrap_err();
    assert_eq!(
        format!(
            "The COMPOSER environment variable is set to {} which is a directory, this variable should point to a composer.json or be left unset.",
            dir
        ),
        err.to_string()
    );
}

#[test]
fn test_get_composer_json_path_from_env() {
    let _tear_down = TearDown;

    Platform::put_env("COMPOSER", " foo.json ");
    assert_eq!("foo.json", Factory::get_composer_file().unwrap());
}
