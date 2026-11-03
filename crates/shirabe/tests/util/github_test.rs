//! ref: composer/tests/Composer/Test/Util/GitHubTest.php

use crate::config_stub::ConfigStubBuilder;
use crate::http_downloader_mock::{expect_full, get_http_downloader_mock};
use crate::io_mock::{Expectation, IOMock, get_io_mock};
use bstr::BStr;
use shirabe::config::{Config, ConfigSourceInterface};
use shirabe::io::{IOInterface, io_interface};
use shirabe::util::GitHub;
use shirabe::util::http_downloader::{HttpDownloader, HttpDownloaderMockHandler};
use shirabe_php_shim::{PhpMixed, bconcat, php_str};

const PASSWORD: &BStr = php_str(b"password");
const MESSAGE: &BStr = php_str(b"mymessage");
const ORIGIN: &BStr = php_str(b"github.com");

// Mirrors GitHubTest's JsonConfigSource mocks. Methods left without an expectation
// panic if called.
#[expect(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    reason = "the code mockall generates"
)]
mod mock_config_source {
    use super::*;

    mockall::mock! {
        #[derive(Debug)]
        pub ConfigSource {}
        impl ConfigSourceInterface for ConfigSource {
            fn add_repository(&mut self, name: &bstr::BStr, config: PhpMixed, append: bool) -> anyhow::Result<()>;
            fn insert_repository(&mut self, name: &bstr::BStr, config: PhpMixed, reference_name: &bstr::BStr, offset: i64) -> anyhow::Result<()>;
            fn set_repository_url(&mut self, name: &bstr::BStr, url: &bstr::BStr) -> anyhow::Result<()>;
            fn remove_repository(&mut self, name: &bstr::BStr) -> anyhow::Result<()>;
            fn add_config_setting(&mut self, name: &bstr::BStr, value: PhpMixed) -> anyhow::Result<()>;
            fn remove_config_setting(&mut self, name: &bstr::BStr) -> anyhow::Result<()>;
            fn add_property(&mut self, name: &bstr::BStr, value: PhpMixed) -> anyhow::Result<()>;
            fn remove_property(&mut self, name: &bstr::BStr) -> anyhow::Result<()>;
            fn add_link(&mut self, r#type: &bstr::BStr, name: &bstr::BStr, value: &bstr::BStr) -> anyhow::Result<()>;
            fn remove_link(&mut self, r#type: &bstr::BStr, name: &bstr::BStr) -> anyhow::Result<()>;
            fn get_name(&self) -> bstr::BString;
        }
    }
}
use mock_config_source::MockConfigSource;

// getAuthJsonMock: getName atLeastOnce -> "auth.json".
fn get_auth_json_mock() -> Box<MockConfigSource> {
    let mut mock = MockConfigSource::new();
    mock.expect_get_name()
        .times(1..)
        .returning(|| "auth.json".into());
    mock.expect_add_config_setting().returning(|_, _| Ok(()));
    mock.expect_remove_config_setting().returning(|_| Ok(()));
    Box::new(mock)
}

// getConfJsonMock: removeConfigSetting atLeastOnce with('github-oauth.<origin>').
fn get_conf_json_mock(origin: &BStr) -> Box<MockConfigSource> {
    let mut mock = MockConfigSource::new();
    let expected = bconcat!("github-oauth.", origin);
    mock.expect_remove_config_setting()
        .times(1..)
        .withf(move |name| name == expected)
        .returning(|_| Ok(()));
    mock.expect_add_config_setting().returning(|_, _| Ok(()));
    mock.expect_get_name().returning(|| "config.json".into());
    Box::new(mock)
}

fn build_github(
    io_mock: &std::rc::Rc<std::cell::RefCell<IOMock>>,
    config: std::rc::Rc<std::cell::RefCell<Config>>,
    http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
) -> GitHub {
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io_mock.clone();
    GitHub::new(io, config, None, Some(http_downloader)).unwrap()
}

// The PHP Config mock returns null for `get('github-expose-hostname')`, which is
// falsy and skips the `hostname` process call. A real Config defaults that key to
// true, so the stub seeds false to reproduce the mock's behaviour.
fn build_config() -> std::rc::Rc<std::cell::RefCell<Config>> {
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
            bconcat!("https://api.", ORIGIN, "/"),
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
            .authorize_oauth_interactively(bstr::BStr::new(ORIGIN), Some(bstr::BStr::new(MESSAGE)))
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
            bconcat!("https://api.", ORIGIN, "/"),
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

    assert!(
        !github
            .authorize_oauth_interactively(bstr::BStr::new(ORIGIN), None)
            .unwrap()
    );
}
