//! ref: composer/tests/Composer/Test/Util/GitLabTest.php sha1:c321d8ac0bc27af8063a3377b63846568ae0d8de

use crate::config_stub::ConfigStubBuilder;
use crate::http_downloader_mock::{expect_full, get_http_downloader_mock};
use crate::io_mock::{Expectation, IOMock, get_io_mock};
use bstr::BStr;
use shirabe::config::{Config, ConfigSourceInterface};
use shirabe::io::{IOInterface, io_interface};
use shirabe::util::GitLab;
use shirabe::util::http_downloader::HttpDownloaderMockHandler;
use shirabe_php_shim::{PhpMixed, bconcat, error_message, php_str};

const USERNAME: &BStr = php_str(b"username");
const PASSWORD: &BStr = php_str(b"password");
const MESSAGE: &BStr = php_str(b"mymessage");
const ORIGIN: &BStr = php_str(b"gitlab.com");
const TOKEN: &BStr = php_str(b"gitlabtoken");
const REFRESHTOKEN: &BStr = php_str(b"gitlabrefreshtoken");

// Mirrors GitLabTest::getAuthJsonMock: a JsonConfigSource whose getName() returns
// "auth.json" (atLeastOnce) and whose addConfigSetting is a no-op (the PHP mock
// never stubs it). Methods left without an expectation panic if called.
#[expect(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    reason = "the code mockall generates"
)]
mod mock_auth_json {
    use super::*;

    mockall::mock! {
        #[derive(Debug)]
        pub AuthJson {}
        impl ConfigSourceInterface for AuthJson {
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
use mock_auth_json::MockAuthJson;

fn get_auth_json_mock() -> Box<MockAuthJson> {
    let mut mock = MockAuthJson::new();
    mock.expect_get_name()
        .times(1..)
        .returning(|| "auth.json".into());
    mock.expect_add_config_setting().returning(|_, _| Ok(()));
    mock.expect_remove_config_setting().returning(|_| Ok(()));
    Box::new(mock)
}

fn set_up(
    io_mock: &std::rc::Rc<std::cell::RefCell<IOMock>>,
    config: &std::rc::Rc<std::cell::RefCell<Config>>,
) {
    config
        .borrow_mut()
        .set_auth_config_source(get_auth_json_mock());
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

    let body = bconcat!(
        "{\"access_token\": \"",
        TOKEN,
        "\", \"refresh_token\": \"",
        REFRESHTOKEN,
        "\", \"token_type\": \"bearer\", \"expires_in\": 7200, \"created_at\": 0}"
    );
    let (http_downloader, _http_guard) = get_http_downloader_mock(
        vec![expect_full(
            bconcat!("http://", ORIGIN, "/oauth/token"),
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

    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io_mock;
    let mut gitlab = GitLab::new(io, config, None, Some(http_downloader)).unwrap();

    assert!(
        gitlab
            .authorize_oauth_interactively(
                bstr::BStr::new("http"),
                bstr::BStr::new(ORIGIN),
                Some(bstr::BStr::new(MESSAGE))
            )
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

    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io_mock;
    let mut gitlab = GitLab::new(io, config, None, Some(http_downloader)).unwrap();

    let err = gitlab
        .authorize_oauth_interactively(bstr::BStr::new("https"), bstr::BStr::new(ORIGIN), None)
        .unwrap_err();
    assert_eq!(
        error_message(&err),
        "Invalid GitLab credentials 5 times in a row, aborting."
    );
}
