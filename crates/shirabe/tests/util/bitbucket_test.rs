//! ref: composer/tests/Composer/Test/Util/BitbucketTest.php

use crate::config_stub::ConfigStubBuilder;
use crate::http_downloader_mock::{expect_full, get_http_downloader_mock};
use crate::io_mock::{Expectation, IOMock, get_io_mock};
use crate::process_executor_mock::get_process_executor_mock;
use indexmap::IndexMap;
use shirabe::config::{Config, ConfigSourceInterface};
use shirabe::io::IOInterface;
use shirabe::io::io_interface;
use shirabe::util::Bitbucket;
use shirabe::util::http_downloader::{
    HttpDownloader, HttpDownloaderMockExpectation, HttpDownloaderMockHandler,
};
use shirabe::util::process_executor::MockHandler;
use shirabe_php_shim::{PhpMixed, time};

const USERNAME: &str = "username";
const PASSWORD: &str = "password";
const CONSUMER_KEY: &str = "consumer_key";
const CONSUMER_SECRET: &str = "consumer_secret";
const MESSAGE: &str = "mymessage";
const ORIGIN: &str = "bitbucket.org";
const TOKEN: &str = "bitbuckettoken";

// Mirrors the PHPUnit mock of ConfigSourceInterface used throughout BitbucketTest.
// Methods left without an expectation panic if called.
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

// A bare auth config source placeholder: getAuthConfigSource() is consulted (e.g. while
// printing instructions), but no add/removeConfigSetting expectations are asserted.
fn placeholder_auth_config_source() -> Box<MockConfigSource> {
    let mut mock = MockConfigSource::new();
    mock.expect_get_name()
        .returning(|| "auth-config-source".to_string());
    mock.expect_add_config_setting().returning(|_, _| Ok(()));
    mock.expect_remove_config_setting().returning(|_| Ok(()));
    Box::new(mock)
}

// Mirrors BitbucketTest::setUp: a DEBUG-verbosity IOMock, a mocked HttpDownloader, a
// real Config, the captured `time()`, and the Bitbucket under test. The mock guards
// run their assert_complete on drop at the end of the test scope.
struct Fixture {
    io: std::rc::Rc<std::cell::RefCell<IOMock>>,
    config: std::rc::Rc<std::cell::RefCell<Config>>,
    http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
    time: i64,
    bitbucket: Bitbucket,
    _io_guard: crate::io_mock::IOMockGuard,
    _http_guard: crate::http_downloader_mock::HttpDownloaderMockGuard,
}

fn set_up_with_config_and_http(
    config: std::rc::Rc<std::cell::RefCell<Config>>,
    http_expectations: Vec<HttpDownloaderMockExpectation>,
) -> Fixture {
    let (io_mock, io_guard) = get_io_mock(io_interface::DEBUG).unwrap();
    let (http_downloader, http_guard) = get_http_downloader_mock(
        http_expectations,
        true,
        HttpDownloaderMockHandler::default(),
    );

    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io_mock.clone();
    let time = time();
    let bitbucket = Bitbucket::new(
        io,
        config.clone(),
        None,
        Some(http_downloader.clone()),
        Some(time),
    )
    .unwrap();

    Fixture {
        io: io_mock,
        config,
        http_downloader,
        time,
        bitbucket,
        _io_guard: io_guard,
        _http_guard: http_guard,
    }
}

// The OAuth2 token request as built by Bitbucket::request_access_token.
fn access_token_request_options() -> IndexMap<String, PhpMixed> {
    let mut http = IndexMap::new();
    http.insert("method".to_string(), PhpMixed::String("POST".to_string()));
    http.insert(
        "content".to_string(),
        PhpMixed::String("grant_type=client_credentials".to_string()),
    );
    let mut options: IndexMap<String, PhpMixed> = IndexMap::new();
    options.insert("retry-auth-failure".to_string(), PhpMixed::Bool(false));
    options.insert("http".to_string(), PhpMixed::Array(http));
    options
}

fn access_token_body() -> String {
    format!(
        "{{\"access_token\": \"{}\", \"scopes\": \"repository\", \"expires_in\": 3600, \"refresh_token\": \"refreshtoken\", \"token_type\": \"bearer\"}}",
        TOKEN
    )
}

// Installs config/auth config source mocks with the access-token storing expectations,
// mirroring BitbucketTest::setExpectationsForStoringAccessToken. Verification happens
// when the mocks are dropped together with the Config.
fn set_expectations_for_storing_access_token(
    config: &std::rc::Rc<std::cell::RefCell<Config>>,
    time: i64,
    remove_basic_auth: bool,
) {
    let mut config_source = MockConfigSource::new();
    config_source
        .expect_get_name()
        .returning(|| "config-source".to_string());
    let removed_key = format!("bitbucket-oauth.{}", ORIGIN);
    config_source
        .expect_remove_config_setting()
        .times(1)
        .withf(move |name| name == removed_key)
        .returning(|_| Ok(()));
    config
        .borrow_mut()
        .set_config_source(Box::new(config_source));

    let mut auth_config_source = MockConfigSource::new();
    auth_config_source
        .expect_get_name()
        .returning(|| "auth-config-source".to_string());
    let added_key = format!("bitbucket-oauth.{}", ORIGIN);
    let expected_value = expected_stored_token(time);
    auth_config_source
        .expect_add_config_setting()
        .times(1)
        .withf(move |name, value| name == added_key && *value == expected_value)
        .returning(|_, _| Ok(()));
    if remove_basic_auth {
        let basic_key = format!("http-basic.{}", ORIGIN);
        auth_config_source
            .expect_remove_config_setting()
            .times(1)
            .withf(move |name| name == basic_key)
            .returning(|_| Ok(()));
    }
    config
        .borrow_mut()
        .set_auth_config_source(Box::new(auth_config_source));
}

fn expected_stored_token(time: i64) -> PhpMixed {
    let mut consumer = IndexMap::new();
    consumer.insert(
        "consumer-key".to_string(),
        PhpMixed::String(CONSUMER_KEY.to_string()),
    );
    consumer.insert(
        "consumer-secret".to_string(),
        PhpMixed::String(CONSUMER_SECRET.to_string()),
    );
    consumer.insert(
        "access-token".to_string(),
        PhpMixed::String(TOKEN.to_string()),
    );
    consumer.insert(
        "access-token-expiration".to_string(),
        PhpMixed::Int(time + 3600),
    );
    PhpMixed::Array(consumer)
}

#[test]
fn test_request_access_token_with_valid_oauth_consumer() {
    let config = ConfigStubBuilder::new().build_shared();
    let mut f = set_up_with_config_and_http(
        config,
        vec![expect_full(
            Bitbucket::OAUTH2_ACCESS_TOKEN_URL,
            Some(access_token_request_options()),
            200,
            access_token_body(),
            vec![],
        )],
    );

    f.io.borrow_mut()
        .expects(
            vec![Expectation::auth(
                ORIGIN,
                CONSUMER_KEY,
                Some(CONSUMER_SECRET.to_string()),
            )],
            false,
        )
        .unwrap();

    set_expectations_for_storing_access_token(&f.config, f.time, false);

    assert_eq!(
        f.bitbucket
            .request_token(ORIGIN, CONSUMER_KEY, CONSUMER_SECRET)
            .unwrap(),
        TOKEN
    );
}

#[test]
fn test_request_access_token_with_valid_oauth_consumer_and_valid_stored_access_token() {
    let time = time();
    let mut stored = IndexMap::new();
    stored.insert(
        "access-token".to_string(),
        PhpMixed::String(TOKEN.to_string()),
    );
    stored.insert(
        "access-token-expiration".to_string(),
        PhpMixed::Int(time + 1800),
    );
    stored.insert(
        "consumer-key".to_string(),
        PhpMixed::String(CONSUMER_KEY.to_string()),
    );
    stored.insert(
        "consumer-secret".to_string(),
        PhpMixed::String(CONSUMER_SECRET.to_string()),
    );
    let mut oauth = IndexMap::new();
    oauth.insert(ORIGIN.to_string(), PhpMixed::Array(stored));

    let config = ConfigStubBuilder::new()
        .with("bitbucket-oauth", PhpMixed::Array(oauth))
        .build_shared();
    let mut f = set_up_with_config_and_http(config, vec![]);
    f.time = time;

    assert_eq!(
        f.bitbucket
            .request_token(ORIGIN, CONSUMER_KEY, CONSUMER_SECRET)
            .unwrap(),
        TOKEN
    );

    // testGetTokenWithAccessToken (@depends): the same Bitbucket now returns the token.
    assert_eq!(f.bitbucket.get_token(), TOKEN);
}

#[test]
fn test_request_access_token_with_valid_oauth_consumer_and_expired_access_token() {
    let time = time();
    let mut stored = IndexMap::new();
    stored.insert(
        "access-token".to_string(),
        PhpMixed::String("randomExpiredToken".to_string()),
    );
    stored.insert(
        "access-token-expiration".to_string(),
        PhpMixed::Int(time - 400),
    );
    stored.insert(
        "consumer-key".to_string(),
        PhpMixed::String(CONSUMER_KEY.to_string()),
    );
    stored.insert(
        "consumer-secret".to_string(),
        PhpMixed::String(CONSUMER_SECRET.to_string()),
    );
    let mut oauth = IndexMap::new();
    oauth.insert(ORIGIN.to_string(), PhpMixed::Array(stored));

    let config = ConfigStubBuilder::new()
        .with("bitbucket-oauth", PhpMixed::Array(oauth))
        .build_shared();
    let mut f = set_up_with_config_and_http(
        config,
        vec![expect_full(
            Bitbucket::OAUTH2_ACCESS_TOKEN_URL,
            Some(access_token_request_options()),
            200,
            access_token_body(),
            vec![],
        )],
    );
    f.time = time;

    f.io.borrow_mut()
        .expects(
            vec![Expectation::auth(
                ORIGIN,
                CONSUMER_KEY,
                Some(CONSUMER_SECRET.to_string()),
            )],
            false,
        )
        .unwrap();

    set_expectations_for_storing_access_token(&f.config, f.time, false);

    assert_eq!(
        f.bitbucket
            .request_token(ORIGIN, CONSUMER_KEY, CONSUMER_SECRET)
            .unwrap(),
        TOKEN
    );
}

#[test]
fn test_request_access_token_with_username_and_password() {
    let config = ConfigStubBuilder::new().build_shared();
    // A 400 status makes the mocked HttpDownloader raise a TransportException(400),
    // matching PHP's willThrowException for the BAD REQUEST case.
    let mut f = set_up_with_config_and_http(
        config,
        vec![expect_full(
            Bitbucket::OAUTH2_ACCESS_TOKEN_URL,
            Some(access_token_request_options()),
            400,
            "",
            vec![],
        )],
    );

    f.io
        .borrow_mut()
        .expects(
            vec![
                Expectation::auth(ORIGIN, USERNAME, Some(PASSWORD.to_string())),
                Expectation::text("Invalid OAuth consumer provided."),
                Expectation::text("This can have three reasons:"),
                Expectation::text(
                    "1. You are authenticating with a bitbucket username/password combination",
                ),
                Expectation::text(
                    "2. You are using an OAuth consumer, but didn't configure a (dummy) callback url",
                ),
                Expectation::text(
                    "3. You are using an OAuth consumer, but didn't configure it as private consumer",
                ),
            ],
            true,
        )
        .unwrap();

    assert_eq!(
        f.bitbucket
            .request_token(ORIGIN, USERNAME, PASSWORD)
            .unwrap(),
        ""
    );
}

#[test]
fn test_request_access_token_with_username_and_password_with_unauthorized_response() {
    let config = ConfigStubBuilder::new().build_shared();
    let mut f = set_up_with_config_and_http(
        config,
        vec![expect_full(
            Bitbucket::OAUTH2_ACCESS_TOKEN_URL,
            Some(access_token_request_options()),
            401,
            "",
            vec![],
        )],
    );

    f.io
        .borrow_mut()
        .expects(
            vec![
                Expectation::auth(ORIGIN, USERNAME, Some(PASSWORD.to_string())),
                Expectation::text("Invalid OAuth consumer provided."),
                Expectation::text(
                    "You can also add it manually later by using \"composer config --global --auth bitbucket-oauth.bitbucket.org <consumer-key> <consumer-secret>\"",
                ),
            ],
            true,
        )
        .unwrap();

    assert_eq!(
        f.bitbucket
            .request_token(ORIGIN, USERNAME, PASSWORD)
            .unwrap(),
        ""
    );
}

#[test]
fn test_request_access_token_with_username_and_password_with_not_found_response() {
    let config = ConfigStubBuilder::new().build_shared();
    let mut f = set_up_with_config_and_http(
        config,
        vec![expect_full(
            Bitbucket::OAUTH2_ACCESS_TOKEN_URL,
            Some(access_token_request_options()),
            404,
            "",
            vec![],
        )],
    );

    f.io.borrow_mut()
        .expects(
            vec![Expectation::auth(
                ORIGIN,
                USERNAME,
                Some(PASSWORD.to_string()),
            )],
            false,
        )
        .unwrap();

    let result = f.bitbucket.request_token(ORIGIN, USERNAME, PASSWORD);
    assert!(
        result.is_err(),
        "expected a TransportException to propagate"
    );
}

#[test]
fn test_username_password_authentication_flow() {
    let url = format!("https://{}/site/oauth2/access_token", ORIGIN);
    let body = format!(
        "{{\"access_token\": \"{}\", \"scopes\": \"repository\", \"expires_in\": 3600, \"refresh_token\": \"refresh_token\", \"token_type\": \"bearer\"}}",
        TOKEN
    );
    // PHP matches the URL with `$this->anything()` for options, so match any options.
    let config = ConfigStubBuilder::new().build_shared();
    let mut f =
        set_up_with_config_and_http(config, vec![expect_full(url, None, 200, body, vec![])]);

    f.io.borrow_mut()
        .expects(
            vec![
                Expectation::text(MESSAGE),
                Expectation::ask("Consumer Key (hidden): ", CONSUMER_KEY),
                Expectation::ask("Consumer Secret (hidden): ", CONSUMER_SECRET),
            ],
            false,
        )
        .unwrap();

    set_expectations_for_storing_access_token(&f.config, f.time, true);

    assert!(
        f.bitbucket
            .authorize_oauth_interactively(ORIGIN, Some(MESSAGE))
            .unwrap()
    );
}

#[test]
fn test_authorize_oauth_interactively_with_empty_username() {
    let config = ConfigStubBuilder::new().build_shared();
    let mut f = set_up_with_config_and_http(config, vec![]);

    // getAuthConfigSource() is consulted while printing the instructions.
    f.config
        .borrow_mut()
        .set_auth_config_source(placeholder_auth_config_source());

    f.io.borrow_mut()
        .expects(vec![Expectation::ask("Consumer Key (hidden): ", "")], false)
        .unwrap();

    assert!(
        !f.bitbucket
            .authorize_oauth_interactively(ORIGIN, Some(MESSAGE))
            .unwrap()
    );
}

#[test]
fn test_authorize_oauth_interactively_with_empty_password() {
    let config = ConfigStubBuilder::new().build_shared();
    let mut f = set_up_with_config_and_http(config, vec![]);

    f.config
        .borrow_mut()
        .set_auth_config_source(placeholder_auth_config_source());

    f.io.borrow_mut()
        .expects(
            vec![
                Expectation::text(MESSAGE),
                Expectation::ask("Consumer Key (hidden): ", CONSUMER_KEY),
                Expectation::ask("Consumer Secret (hidden): ", ""),
            ],
            false,
        )
        .unwrap();

    assert!(
        !f.bitbucket
            .authorize_oauth_interactively(ORIGIN, Some(MESSAGE))
            .unwrap()
    );
}

#[test]
fn test_authorize_oauth_interactively_with_request_access_token_failure() {
    let url = format!("https://{}/site/oauth2/access_token", ORIGIN);
    let config = ConfigStubBuilder::new().build_shared();
    // A 400 status makes the mocked HttpDownloader raise a TransportException(400).
    let mut f = set_up_with_config_and_http(config, vec![expect_full(url, None, 400, "", vec![])]);

    f.config
        .borrow_mut()
        .set_auth_config_source(placeholder_auth_config_source());

    f.io.borrow_mut()
        .expects(
            vec![
                Expectation::text(MESSAGE),
                Expectation::ask("Consumer Key (hidden): ", CONSUMER_KEY),
                Expectation::ask("Consumer Secret (hidden): ", CONSUMER_SECRET),
            ],
            false,
        )
        .unwrap();

    assert!(
        !f.bitbucket
            .authorize_oauth_interactively(ORIGIN, Some(MESSAGE))
            .unwrap()
    );
}

#[test]
fn test_get_token_without_access_token() {
    let config = ConfigStubBuilder::new().build_shared();
    let f = set_up_with_config_and_http(config, vec![]);
    assert_eq!(f.bitbucket.get_token(), "");
}

#[test]
fn test_authorize_oauth_with_wrong_origin_url() {
    let config = ConfigStubBuilder::new().build_shared();
    let mut f = set_up_with_config_and_http(config, vec![]);
    assert!(!f.bitbucket.authorize_oauth(&format!("non-{}", ORIGIN)));
}

#[test]
fn test_authorize_oauth_without_available_git_config_token() {
    let config = ConfigStubBuilder::new().build_shared();
    let (io_mock, _io_guard) = get_io_mock(io_interface::DEBUG).unwrap();
    let (http_downloader, _http_guard) =
        get_http_downloader_mock(vec![], true, HttpDownloaderMockHandler::default());
    let (process, _process_guard) = get_process_executor_mock(
        vec![],
        false,
        MockHandler {
            r#return: -1,
            ..Default::default()
        },
    );

    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io_mock.clone();
    let time = time();
    let mut bitbucket =
        Bitbucket::new(io, config, Some(process), Some(http_downloader), Some(time)).unwrap();

    assert!(!bitbucket.authorize_oauth(ORIGIN));
}

#[test]
fn test_authorize_oauth_with_available_git_config_token() {
    let config = ConfigStubBuilder::new().build_shared();
    let (io_mock, _io_guard) = get_io_mock(io_interface::DEBUG).unwrap();
    let (http_downloader, _http_guard) =
        get_http_downloader_mock(vec![], true, HttpDownloaderMockHandler::default());
    let (process, _process_guard) =
        get_process_executor_mock(vec![], false, MockHandler::default());

    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io_mock.clone();
    let time = time();
    let mut bitbucket =
        Bitbucket::new(io, config, Some(process), Some(http_downloader), Some(time)).unwrap();

    assert!(bitbucket.authorize_oauth(ORIGIN));
}
