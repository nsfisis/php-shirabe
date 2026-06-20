//! ref: composer/tests/Composer/Test/FactoryTest.php

use shirabe::factory::Factory;
use shirabe::util::platform::Platform;

#[test]
#[ignore = "mocks an IOInterface with a writeError expectation and a Config returning disable-tls=true; mocking is not available"]
fn test_default_values_are_as_expected() {
    todo!()
}

#[test]
#[ignore = "depends on COMPOSER being unset, but sibling tests set it and the tearDown that clears it is not ported"]
fn test_get_composer_json_path() {
    assert_eq!("./composer.json", Factory::get_composer_file().unwrap());
}

#[test]
fn test_get_composer_json_path_fails_if_dir() {
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
    Platform::put_env("COMPOSER", " foo.json ");
    assert_eq!("foo.json", Factory::get_composer_file().unwrap());
}
