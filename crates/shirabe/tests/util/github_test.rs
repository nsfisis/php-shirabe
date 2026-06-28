//! ref: composer/tests/Composer/Test/Util/GitHubTest.php

use std::cell::RefCell;
use std::rc::Rc;

use shirabe::config::{Config, ConfigSourceInterface};
use shirabe::io::IOInterface;
use shirabe::io::io_interface;
use shirabe::util::GitHub;
use shirabe::util::http_downloader::{HttpDownloader, HttpDownloaderMockHandler};
use shirabe_php_shim::PhpMixed;

use crate::config_stub::ConfigStubBuilder;
use crate::http_downloader_mock::{expect_full, get_http_downloader_mock};
use crate::io_mock::{Expectation, IOMock, get_io_mock};

const PASSWORD: &str = "password";
const MESSAGE: &str = "mymessage";
const ORIGIN: &str = "github.com";

// Mirrors GitHubTest's JsonConfigSource mocks. Methods left without an expectation
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

// getConfJsonMock: removeConfigSetting atLeastOnce with('github-oauth.<origin>').
fn get_conf_json_mock(origin: &str) -> Box<MockConfigSource> {
    let mut mock = MockConfigSource::new();
    let expected = format!("github-oauth.{}", origin);
    mock.expect_remove_config_setting()
        .times(1..)
        .withf(move |name| name == expected)
        .returning(|_| Ok(()));
    mock.expect_add_config_setting().returning(|_, _| Ok(()));
    mock.expect_get_name()
        .returning(|| "config.json".to_string());
    Box::new(mock)
}

fn build_github(
    io_mock: &Rc<RefCell<IOMock>>,
    config: Rc<RefCell<Config>>,
    http_downloader: Rc<RefCell<HttpDownloader>>,
) -> GitHub {
    let io: Rc<RefCell<dyn IOInterface>> = io_mock.clone();
    GitHub::new(io, config, None, Some(http_downloader)).unwrap()
}

// The PHP Config mock returns null for `get('github-expose-hostname')`, which is
// falsy and skips the `hostname` process call. A real Config defaults that key to
// true, so the stub seeds false to reproduce the mock's behaviour.
fn build_config() -> Rc<RefCell<Config>> {
    ConfigStubBuilder::new()
        .with("github-expose-hostname", PhpMixed::Bool(false))
        .build_shared()
}

#[test]
fn test_username_password_authentication_flow() {
    let (io_mock, _io_guard) = get_io_mock(io_interface::NORMAL).unwrap();
    io_mock
        .borrow_mut()
        .expects(
            vec![
                Expectation::text(MESSAGE),
                Expectation::ask("Token (hidden): ", PASSWORD),
            ],
            false,
        )
        .unwrap();

    let (http_downloader, _http_guard) = get_http_downloader_mock(
        vec![expect_full(
            format!("https://api.{}/", ORIGIN),
            None,
            200,
            "{}",
            vec![],
        )],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let config = build_config();
    config
        .borrow_mut()
        .set_auth_config_source(get_auth_json_mock());
    config
        .borrow_mut()
        .set_config_source(get_conf_json_mock(ORIGIN));

    let mut github = build_github(&io_mock, config, http_downloader);

    assert!(
        github
            .authorize_oauth_interactively(ORIGIN, Some(MESSAGE))
            .unwrap()
    );
    // removeConfigSetting('github-oauth.<origin>') verified on the conf source mock drop.
}

#[test]
fn test_username_password_failure() {
    let (io_mock, _io_guard) = get_io_mock(io_interface::NORMAL).unwrap();
    io_mock
        .borrow_mut()
        .expects(vec![Expectation::ask("Token (hidden): ", PASSWORD)], false)
        .unwrap();

    let (http_downloader, _http_guard) = get_http_downloader_mock(
        vec![expect_full(
            format!("https://api.{}/", ORIGIN),
            None,
            401,
            "",
            vec![],
        )],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let config = build_config();
    config
        .borrow_mut()
        .set_auth_config_source(get_auth_json_mock());

    let mut github = build_github(&io_mock, config, http_downloader);

    assert!(!github.authorize_oauth_interactively(ORIGIN, None).unwrap());
}
