//! ref: composer/tests/Composer/Test/Util/GitLabTest.php

use std::cell::RefCell;
use std::rc::Rc;

use shirabe::config::{Config, ConfigSourceInterface};
use shirabe::io::IOInterface;
use shirabe::io::io_interface;
use shirabe::util::GitLab;
use shirabe::util::http_downloader::{HttpDownloader, HttpDownloaderMockHandler};
use shirabe_php_shim::PhpMixed;

use crate::config_stub::ConfigStubBuilder;
use crate::http_downloader_mock::{expect_full, get_http_downloader_mock};
use crate::io_mock::{Expectation, IOMock, get_io_mock};

const USERNAME: &str = "username";
const PASSWORD: &str = "password";
const MESSAGE: &str = "mymessage";
const ORIGIN: &str = "gitlab.com";
const TOKEN: &str = "gitlabtoken";
const REFRESHTOKEN: &str = "gitlabrefreshtoken";

// Mirrors GitLabTest::getAuthJsonMock: a JsonConfigSource whose getName() returns
// "auth.json" and whose addConfigSetting is a no-op (the PHP mock never stubs it).
#[derive(Debug)]
struct AuthJsonMock;

impl ConfigSourceInterface for AuthJsonMock {
    fn add_repository(
        &mut self,
        _name: &str,
        _config: PhpMixed,
        _append: bool,
    ) -> anyhow::Result<()> {
        unreachable!()
    }
    fn insert_repository(
        &mut self,
        _name: &str,
        _config: PhpMixed,
        _reference_name: &str,
        _offset: i64,
    ) -> anyhow::Result<()> {
        unreachable!()
    }
    fn set_repository_url(&mut self, _name: &str, _url: &str) -> anyhow::Result<()> {
        unreachable!()
    }
    fn remove_repository(&mut self, _name: &str) -> anyhow::Result<()> {
        unreachable!()
    }
    fn add_config_setting(&mut self, _name: &str, _value: PhpMixed) -> anyhow::Result<()> {
        Ok(())
    }
    fn remove_config_setting(&mut self, _name: &str) -> anyhow::Result<()> {
        Ok(())
    }
    fn add_property(&mut self, _name: &str, _value: PhpMixed) -> anyhow::Result<()> {
        unreachable!()
    }
    fn remove_property(&mut self, _name: &str) -> anyhow::Result<()> {
        unreachable!()
    }
    fn add_link(&mut self, _type: &str, _name: &str, _value: &str) -> anyhow::Result<()> {
        unreachable!()
    }
    fn remove_link(&mut self, _type: &str, _name: &str) -> anyhow::Result<()> {
        unreachable!()
    }
    fn get_name(&self) -> String {
        "auth.json".to_string()
    }
}

fn set_up(io_mock: &Rc<RefCell<IOMock>>, config: &Rc<RefCell<Config>>) {
    config
        .borrow_mut()
        .set_auth_config_source(Box::new(AuthJsonMock));
    let _ = io_mock;
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
                Expectation::ask("Password: ", PASSWORD),
            ],
            false,
        )
        .unwrap();

    let body = format!(
        "{{\"access_token\": \"{}\", \"refresh_token\": \"{}\", \"token_type\": \"bearer\", \"expires_in\": 7200, \"created_at\": 0}}",
        TOKEN, REFRESHTOKEN
    );
    let (http_downloader, _http_guard) = get_http_downloader_mock(
        vec![expect_full(
            format!("http://{}/oauth/token", ORIGIN),
            None,
            200,
            body,
            vec![],
        )],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let config = ConfigStubBuilder::new().build_shared();
    set_up(&io_mock, &config);

    let io: Rc<RefCell<dyn IOInterface>> = io_mock.clone();
    let mut gitlab = GitLab::new(io, config, None, Some(http_downloader)).unwrap();

    assert!(
        gitlab
            .authorize_oauth_interactively("http", ORIGIN, Some(MESSAGE))
            .unwrap()
    );
}

#[test]
fn test_username_password_failure() {
    let (io_mock, _io_guard) = get_io_mock(io_interface::NORMAL).unwrap();
    io_mock
        .borrow_mut()
        .expects(
            vec![
                Expectation::ask("Username: ", USERNAME),
                Expectation::ask("Password: ", PASSWORD),
                Expectation::ask("Username: ", USERNAME),
                Expectation::ask("Password: ", PASSWORD),
                Expectation::ask("Username: ", USERNAME),
                Expectation::ask("Password: ", PASSWORD),
                Expectation::ask("Username: ", USERNAME),
                Expectation::ask("Password: ", PASSWORD),
                Expectation::ask("Username: ", USERNAME),
                Expectation::ask("Password: ", PASSWORD),
            ],
            false,
        )
        .unwrap();

    let (http_downloader, _http_guard) = get_http_downloader_mock(
        vec![
            expect_full("https://gitlab.com/oauth/token", None, 401, "{}", vec![]),
            expect_full("https://gitlab.com/oauth/token", None, 401, "{}", vec![]),
            expect_full("https://gitlab.com/oauth/token", None, 401, "{}", vec![]),
            expect_full("https://gitlab.com/oauth/token", None, 401, "{}", vec![]),
            expect_full("https://gitlab.com/oauth/token", None, 401, "{}", vec![]),
        ],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let config = ConfigStubBuilder::new().build_shared();
    set_up(&io_mock, &config);

    let io: Rc<RefCell<dyn IOInterface>> = io_mock.clone();
    let mut gitlab = GitLab::new(io, config, None, Some(http_downloader)).unwrap();

    let err = gitlab
        .authorize_oauth_interactively("https", ORIGIN, None)
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Invalid GitLab credentials 5 times in a row, aborting."
    );
}
