//! ref: composer/tests/Composer/Test/FactoryTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use serial_test::serial;
use shirabe::factory::Factory;
use shirabe::io::IOInterface;
use shirabe::io::io_interface;
use shirabe::util::platform::Platform;
use shirabe_php_shim::PhpMixed;

#[path = "common/config_stub.rs"]
mod config_stub;
#[path = "common/io_mock.rs"]
#[allow(dead_code)] // io_mock exposes more helpers than this binary uses
mod io_mock;
use config_stub::ConfigStubBuilder;
use io_mock::{Expectation, get_io_mock};

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
#[serial]
fn test_default_values_are_as_expected() {
    let _tear_down = TearDown;

    let (io_mock, _io_guard) = get_io_mock(io_interface::DEBUG).unwrap();
    io_mock
        .borrow_mut()
        .expects(
            vec![Expectation::text(
                "<warning>You are running Composer with SSL/TLS protection disabled.</warning>",
            )],
            false,
        )
        .unwrap();

    let config = ConfigStubBuilder::new()
        .with("disable-tls", PhpMixed::Bool(true))
        .build_shared();

    let io: Rc<RefCell<dyn IOInterface>> = io_mock.clone();
    Factory::create_http_downloader(io, &config, IndexMap::new()).unwrap();
}

#[test]
#[serial]
fn test_get_composer_json_path() {
    let _tear_down = TearDown;

    assert_eq!("./composer.json", Factory::get_composer_file().unwrap());
}

#[test]
#[serial]
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
#[serial]
fn test_get_composer_json_path_from_env() {
    let _tear_down = TearDown;

    Platform::put_env("COMPOSER", " foo.json ");
    assert_eq!("foo.json", Factory::get_composer_file().unwrap());
}
