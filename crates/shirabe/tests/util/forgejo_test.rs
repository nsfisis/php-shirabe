//! ref: composer/tests/Composer/Test/Util/ForgejoTest.php

use std::cell::RefCell;
use std::rc::Rc;

use shirabe::config::{Config, ConfigSourceInterface};
use shirabe::io::IOInterface;
use shirabe::io::io_interface;
use shirabe::util::Forgejo;
use shirabe::util::http_downloader::{HttpDownloader, HttpDownloaderMockHandler};
use shirabe_php_shim::PhpMixed;

use crate::config_stub::ConfigStubBuilder;
use crate::http_downloader_mock::{expect_full, get_http_downloader_mock};
use crate::io_mock::{Expectation, IOMock, get_io_mock};

const USERNAME: &str = "username";
const ACCESS_TOKEN: &str = "access-token";
const MESSAGE: &str = "mymessage";
const ORIGIN: &str = "codeberg.org";

// Records the config setting names a source has had removed, plus a fixed getName,
// mirroring ForgejoTest's JsonConfigSource mocks (getName -> "auth.json", and the
// config source stubbing removeConfigSetting('forgejo-token.<origin>')).
#[derive(Debug)]
struct ConfigSourceMock {
    name: String,
    removed: Rc<RefCell<Vec<String>>>,
}

impl ConfigSourceMock {
    fn new(name: &str) -> (Box<Self>, Rc<RefCell<Vec<String>>>) {
        let removed = Rc::new(RefCell::new(Vec::new()));
        (
            Box::new(Self {
                name: name.to_string(),
                removed: removed.clone(),
            }),
            removed,
        )
    }
}

impl ConfigSourceInterface for ConfigSourceMock {
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
    fn remove_config_setting(&mut self, name: &str) -> anyhow::Result<()> {
        self.removed.borrow_mut().push(name.to_string());
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
        self.name.clone()
    }
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
    let (auth_source, _) = ConfigSourceMock::new("auth.json");
    let (conf_source, conf_removed) = ConfigSourceMock::new("config.json");
    config.borrow_mut().set_auth_config_source(auth_source);
    config.borrow_mut().set_config_source(conf_source);

    let mut forgejo = build_forgejo(&io_mock, config, http_downloader);

    assert!(
        forgejo
            .authorize_o_auth_interactively(ORIGIN, Some(MESSAGE))
            .unwrap()
            .unwrap()
    );
    assert_eq!(
        *conf_removed.borrow(),
        vec![format!("forgejo-token.{}", ORIGIN)]
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
    let (auth_source, _) = ConfigSourceMock::new("auth.json");
    config.borrow_mut().set_auth_config_source(auth_source);

    let mut forgejo = build_forgejo(&io_mock, config, http_downloader);

    assert!(
        !forgejo
            .authorize_o_auth_interactively(ORIGIN, None)
            .unwrap()
            .unwrap()
    );
}
