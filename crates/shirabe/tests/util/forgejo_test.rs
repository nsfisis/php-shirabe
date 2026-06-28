//! ref: composer/tests/Composer/Test/Util/ForgejoTest.php

use crate::config_stub::ConfigStubBuilder;
use crate::http_downloader_mock::{expect_full, get_http_downloader_mock};
use crate::io_mock::{Expectation, IOMock, get_io_mock};
use shirabe::config::{Config, ConfigSourceInterface};
use shirabe::io::IOInterface;
use shirabe::io::io_interface;
use shirabe::util::Forgejo;
use shirabe::util::http_downloader::{HttpDownloader, HttpDownloaderMockHandler};
use shirabe_php_shim::PhpMixed;
use std::cell::RefCell;
use std::rc::Rc;

const USERNAME: &str = "username";
const ACCESS_TOKEN: &str = "access-token";
const MESSAGE: &str = "mymessage";
const ORIGIN: &str = "codeberg.org";

// Mirrors ForgejoTest's JsonConfigSource mocks. Methods left without an expectation
// panic if called.
mockall::mock! {
    #[derive(Debug)]
    pub ConfigSource {}
    impl ConfigSourceInterface for ConfigSource {
        fn add_repository(&mut self, name: &str, config: PhpMixed, append: bool) -> anyhow::Result<()>;
        fn insert_repository(&mut self, name: &str, config: PhpMixed, reference_name: &str, offset: i64) -> anyhow::Result<()>;
        fn set_repository_url(&mut self, name: &str, url: &str) -> anyhow::Result<()>;
        fn remove_repository(&mut self, name: &str) -> anyhow::Result<()>;
        fn add_config_setting(&mut self, name: &str, value: PhpMixed) -> anyhow::Result<()>;
        fn remove_config_setting(&mut self, name: &str) -> anyhow::Result<()>;
        fn add_property(&mut self, name: &str, value: PhpMixed) -> anyhow::Result<()>;
        fn remove_property(&mut self, name: &str) -> anyhow::Result<()>;
        fn add_link(&mut self, r#type: &str, name: &str, value: &str) -> anyhow::Result<()>;
        fn remove_link(&mut self, r#type: &str, name: &str) -> anyhow::Result<()>;
        fn get_name(&self) -> String;
    }
}

// getAuthJsonMock: getName atLeastOnce -> "auth.json".
fn get_auth_json_mock() -> Box<MockConfigSource> {
    let mut mock = MockConfigSource::new();
    mock.expect_get_name()
        .times(1..)
        .returning(|| "auth.json".to_string());
    mock.expect_add_config_setting().returning(|_, _| Ok(()));
    mock.expect_remove_config_setting().returning(|_| Ok(()));
    Box::new(mock)
}

// getConfJsonMock: removeConfigSetting atLeastOnce with('forgejo-token.<origin>').
fn get_conf_json_mock(origin: &str) -> Box<MockConfigSource> {
    let mut mock = MockConfigSource::new();
    let expected = format!("forgejo-token.{}", origin);
    mock.expect_remove_config_setting()
        .times(1..)
        .withf(move |name| name == expected)
        .returning(|_| Ok(()));
    mock.expect_add_config_setting().returning(|_, _| Ok(()));
    mock.expect_get_name()
        .returning(|| "config.json".to_string());
    Box::new(mock)
}

fn build_forgejo(
    io_mock: &Rc<RefCell<IOMock>>,
    config: Rc<RefCell<Config>>,
    http_downloader: Rc<RefCell<HttpDownloader>>,
) -> Forgejo {
    let io: Rc<RefCell<dyn IOInterface>> = io_mock.clone();
    Forgejo::new(io, config, http_downloader)
}

#[test]
fn test_username_password_authentication_flow() {
    let (io_mock, _io_guard) = get_io_mock(io_interface::NORMAL).unwrap();
    io_mock
        .borrow_mut()
        .expects(
            vec![
                Expectation::text(MESSAGE),
                Expectation::ask("Username: ", USERNAME),
                Expectation::ask("Token (hidden): ", ACCESS_TOKEN),
            ],
            false,
        )
        .unwrap();

    let (http_downloader, _http_guard) = get_http_downloader_mock(
        vec![expect_full(
            format!("https://{}/api/v1/version", ORIGIN),
            None,
            200,
            "{}",
            vec![],
        )],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let config = ConfigStubBuilder::new().build_shared();
    config
        .borrow_mut()
        .set_auth_config_source(get_auth_json_mock());
    config
        .borrow_mut()
        .set_config_source(get_conf_json_mock(ORIGIN));

    let mut forgejo = build_forgejo(&io_mock, config, http_downloader);

    assert!(
        forgejo
            .authorize_o_auth_interactively(ORIGIN, Some(MESSAGE))
            .unwrap()
            .unwrap()
    );
    // removeConfigSetting('forgejo-token.<origin>') verified on the conf source mock drop.
}

#[test]
fn test_username_password_failure() {
    let (io_mock, _io_guard) = get_io_mock(io_interface::NORMAL).unwrap();
    io_mock
        .borrow_mut()
        .expects(
            vec![
                Expectation::ask("Username: ", USERNAME),
                Expectation::ask("Token (hidden): ", ACCESS_TOKEN),
            ],
            false,
        )
        .unwrap();

    let (http_downloader, _http_guard) = get_http_downloader_mock(
        vec![expect_full(
            format!("https://{}/api/v1/version", ORIGIN),
            None,
            404,
            "",
            vec![],
        )],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let config = ConfigStubBuilder::new().build_shared();
    config
        .borrow_mut()
        .set_auth_config_source(get_auth_json_mock());

    let mut forgejo = build_forgejo(&io_mock, config, http_downloader);

    assert!(
        !forgejo
            .authorize_o_auth_interactively(ORIGIN, None)
            .unwrap()
            .unwrap()
    );
}
