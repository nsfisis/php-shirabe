//! ref: composer/tests/Composer/Test/Util/AuthHelperTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::config::ConfigSourceInterface;
use shirabe::io::IOInterface;
use shirabe::io::io_interface;
use shirabe::util::{AuthHelper, Bitbucket, StoreAuth};
use shirabe_php_shim::{PhpMixed, base64_encode, json_encode};

use crate::config_stub::ConfigStubBuilder;
use crate::io_mock::{Expectation, IOMock, get_io_mock};

// Mirrors AuthHelperTest::setUp: a DEBUG-verbosity IOMock plus a real Config, both
// shared with the AuthHelper under test. The IOMockGuard runs assert_complete on drop.
struct Fixture {
    io: Rc<RefCell<IOMock>>,
    config: Rc<RefCell<shirabe::config::Config>>,
    auth_helper: AuthHelper,
    _guard: crate::io_mock::IOMockGuard,
}

fn set_up_with_config(config: Rc<RefCell<shirabe::config::Config>>) -> Fixture {
    let (mock, guard) = get_io_mock(io_interface::DEBUG).unwrap();
    let io: Rc<RefCell<dyn IOInterface>> = mock.clone();
    let auth_helper = AuthHelper::new(io, config.clone());
    Fixture {
        io: mock,
        config,
        auth_helper,
        _guard: guard,
    }
}

fn set_up() -> Fixture {
    set_up_with_config(ConfigStubBuilder::new().build_shared())
}

// Mirrors AuthHelperTest::expectsAuthentication: pre-seed the IO so hasAuthentication
// and getAuthentication return the given credentials for `origin`.
fn expects_authentication(io: &Rc<RefCell<IOMock>>, origin: &str, username: &str, password: &str) {
    use shirabe::io::IOInterfaceMutable;
    io.borrow_mut().set_authentication(
        origin.to_string(),
        username.to_string(),
        Some(password.to_string()),
    );
}

fn header_strings(options: &IndexMap<String, PhpMixed>) -> Vec<String> {
    options
        .get("http")
        .and_then(|v| v.as_array())
        .and_then(|http| http.get("header"))
        .and_then(|v| v.as_list())
        .map(|list| {
            list.iter()
                .map(|v| v.as_string().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn base_headers() -> Vec<PhpMixed> {
    vec![
        PhpMixed::String("Accept-Encoding: gzip".to_string()),
        PhpMixed::String("Connection: close".to_string()),
    ]
}

fn http_options_with_headers() -> IndexMap<String, PhpMixed> {
    let mut http = IndexMap::new();
    http.insert("header".to_string(), PhpMixed::List(base_headers()));
    let mut options = IndexMap::new();
    options.insert("http".to_string(), PhpMixed::Array(http));
    options
}

#[test]
fn test_add_authentication_header_without_auth_credentials() {
    let mut f = set_up();
    let options = http_options_with_headers();
    let origin = "http://example.org";
    let url = "file://example";

    let options = f
        .auth_helper
        .add_authentication_options(options, origin, url)
        .unwrap();

    assert_eq!(
        header_strings(&options),
        vec!["Accept-Encoding: gzip", "Connection: close"]
    );
}

#[test]
fn test_add_authentication_header_with_bearer_password() {
    let mut f = set_up();
    let options = http_options_with_headers();
    let origin = "http://example.org";
    let url = "file://example";
    expects_authentication(&f.io, origin, "my_username", "bearer");

    let options = f
        .auth_helper
        .add_authentication_options(options, origin, url)
        .unwrap();

    assert_eq!(
        header_strings(&options),
        vec![
            "Accept-Encoding: gzip",
            "Connection: close",
            "Authorization: Bearer my_username",
        ]
    );
}

#[test]
fn test_add_authentication_header_with_github_token() {
    let mut f = set_up();
    f.io.borrow_mut()
        .expects(
            vec![Expectation::text("Using GitHub token authentication")],
            false,
        )
        .unwrap();
    let options = http_options_with_headers();
    let origin = "github.com";
    let url = "https://api.github.com/";
    expects_authentication(&f.io, origin, "my_username", "x-oauth-basic");

    let options = f
        .auth_helper
        .add_authentication_options(options, origin, url)
        .unwrap();

    assert_eq!(
        header_strings(&options),
        vec![
            "Accept-Encoding: gzip",
            "Connection: close",
            "Authorization: token my_username",
        ]
    );
}

#[test]
fn test_add_authentication_header_with_gitlab_oath_token() {
    let config = ConfigStubBuilder::new()
        .with(
            "gitlab-domains",
            PhpMixed::List(vec![PhpMixed::String("gitlab.com".to_string())]),
        )
        .build_shared();
    let mut f = set_up_with_config(config);
    f.io.borrow_mut()
        .expects(
            vec![Expectation::text("Using GitLab OAuth token authentication")],
            false,
        )
        .unwrap();
    let options = http_options_with_headers();
    let origin = "gitlab.com";
    let url = "https://api.gitlab.com/";
    expects_authentication(&f.io, origin, "my_username", "oauth2");

    let options = f
        .auth_helper
        .add_authentication_options(options, origin, url)
        .unwrap();

    assert_eq!(
        header_strings(&options),
        vec![
            "Accept-Encoding: gzip",
            "Connection: close",
            "Authorization: Bearer my_username",
        ]
    );
}

#[test]
fn test_add_authentication_options_for_client_certificate() {
    let mut f = set_up();
    let options = IndexMap::new();
    let origin = "example.org";
    let url = "file://example";

    let mut certificate_configuration = IndexMap::new();
    certificate_configuration.insert(
        "local_cert".to_string(),
        PhpMixed::String("certificate value".to_string()),
    );
    certificate_configuration.insert(
        "local_pk".to_string(),
        PhpMixed::String("key value".to_string()),
    );
    certificate_configuration.insert(
        "passphrase".to_string(),
        PhpMixed::String("passphrase value".to_string()),
    );
    let password = json_encode(&PhpMixed::Array(certificate_configuration.clone())).unwrap();
    expects_authentication(&f.io, origin, "client-certificate", &password);

    let options = f
        .auth_helper
        .add_authentication_options(options, origin, url)
        .unwrap();

    assert_eq!(
        options.get("ssl"),
        Some(&PhpMixed::Array(certificate_configuration))
    );
}

fn add_authentication_header_with_gitlab_private_token(password: &str) {
    let config = ConfigStubBuilder::new()
        .with(
            "gitlab-domains",
            PhpMixed::List(vec![PhpMixed::String("gitlab.com".to_string())]),
        )
        .build_shared();
    let mut f = set_up_with_config(config);
    f.io.borrow_mut()
        .expects(
            vec![Expectation::text(
                "Using GitLab private token authentication",
            )],
            false,
        )
        .unwrap();
    let options = http_options_with_headers();
    let origin = "gitlab.com";
    let url = "https://api.gitlab.com/";
    expects_authentication(&f.io, origin, "my_username", password);

    let options = f
        .auth_helper
        .add_authentication_options(options, origin, url)
        .unwrap();

    assert_eq!(
        header_strings(&options),
        vec![
            "Accept-Encoding: gzip",
            "Connection: close",
            "PRIVATE-TOKEN: my_username",
        ]
    );
}

#[test]
fn test_add_authentication_header_with_gitlab_private_token() {
    add_authentication_header_with_gitlab_private_token("private-token");
    add_authentication_header_with_gitlab_private_token("gitlab-ci-token");
}

#[test]
fn test_add_authentication_header_with_bitbucket_oath_token() {
    let mut f = set_up();
    f.io.borrow_mut()
        .expects(
            vec![Expectation::text(
                "Using Bitbucket OAuth token authentication",
            )],
            false,
        )
        .unwrap();
    let options = http_options_with_headers();
    let origin = "bitbucket.org";
    let url = "https://bitbucket.org/site/oauth2/authorize";
    expects_authentication(&f.io, origin, "x-token-auth", "my_password");

    let options = f
        .auth_helper
        .add_authentication_options(options, origin, url)
        .unwrap();

    assert_eq!(
        header_strings(&options),
        vec![
            "Accept-Encoding: gzip",
            "Connection: close",
            "Authorization: Bearer my_password",
        ]
    );
}

fn add_authentication_header_with_bitbucket_public_url(url: &str) {
    let mut f = set_up();
    let options = http_options_with_headers();
    let origin = "bitbucket.org";
    expects_authentication(&f.io, origin, "x-token-auth", "my_password");

    let options = f
        .auth_helper
        .add_authentication_options(options, origin, url)
        .unwrap();

    assert_eq!(
        header_strings(&options),
        vec!["Accept-Encoding: gzip", "Connection: close"]
    );
}

#[test]
fn test_add_authentication_header_with_bitbucket_public_url() {
    add_authentication_header_with_bitbucket_public_url(
        "https://bitbucket.org/user/repo/downloads/whatever",
    );
    add_authentication_header_with_bitbucket_public_url(
        "https://bbuseruploads.s3.amazonaws.com/9421ee72-638e-43a9-82ea-39cfaae2bfaa/downloads/b87c59d9-54f3-4922-b711-d89059ec3bcf",
    );
}

fn add_authentication_header_with_basic_http_authentication(
    url: &str,
    origin: &str,
    username: &str,
    password: &str,
) {
    let mut f = set_up();
    f.io.borrow_mut()
        .expects(
            vec![Expectation::text(format!(
                "Using HTTP basic authentication with username \"{}\"",
                username
            ))],
            false,
        )
        .unwrap();
    let options = http_options_with_headers();
    expects_authentication(&f.io, origin, username, password);

    let expected = format!(
        "Authorization: Basic {}",
        base64_encode(&format!("{}:{}", username, password))
    );

    let options = f
        .auth_helper
        .add_authentication_options(options, origin, url)
        .unwrap();

    assert_eq!(
        header_strings(&options),
        vec![
            "Accept-Encoding: gzip".to_string(),
            "Connection: close".to_string(),
            expected,
        ]
    );
}

#[test]
fn test_add_authentication_header_with_basic_http_authentication() {
    add_authentication_header_with_basic_http_authentication(
        Bitbucket::OAUTH2_ACCESS_TOKEN_URL,
        "bitbucket.org",
        "x-token-auth",
        "my_password",
    );
    add_authentication_header_with_basic_http_authentication(
        "https://some-api.url.com",
        "some-api.url.com",
        "my_username",
        "my_password",
    );
    add_authentication_header_with_basic_http_authentication(
        "https://gitlab.com",
        "gitlab.com",
        "my_username",
        "my_password",
    );
}

#[test]
fn test_is_public_bit_bucket_download_with_bitbucket_public_url() {
    let f = set_up();
    assert!(
        f.auth_helper
            .is_public_bit_bucket_download("https://bitbucket.org/user/repo/downloads/whatever")
    );
    assert!(f.auth_helper.is_public_bit_bucket_download(
        "https://bbuseruploads.s3.amazonaws.com/9421ee72-638e-43a9-82ea-39cfaae2bfaa/downloads/b87c59d9-54f3-4922-b711-d89059ec3bcf",
    ));
}

#[test]
fn test_is_public_bit_bucket_download_with_non_bitbucket_public_url() {
    let f = set_up();
    assert!(
        !f.auth_helper
            .is_public_bit_bucket_download("https://bitbucket.org/site/oauth2/authorize")
    );
}

// Records addConfigSetting calls and serves a configurable getName, mirroring the
// PHPUnit mock of ConfigSourceInterface used by the storeAuth tests.
#[derive(Debug)]
struct ConfigSourceMock {
    name: String,
    added: Rc<RefCell<Vec<(String, PhpMixed)>>>,
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
    fn add_config_setting(&mut self, name: &str, value: PhpMixed) -> anyhow::Result<()> {
        self.added.borrow_mut().push((name.to_string(), value));
        Ok(())
    }
    fn remove_config_setting(&mut self, _name: &str) -> anyhow::Result<()> {
        unreachable!()
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

fn expected_auth_setting(username: &str, password: &str) -> PhpMixed {
    let mut auth = IndexMap::new();
    auth.insert(
        "username".to_string(),
        PhpMixed::String(username.to_string()),
    );
    auth.insert(
        "password".to_string(),
        PhpMixed::String(password.to_string()),
    );
    PhpMixed::Array(auth)
}

#[test]
fn test_store_auth_automatically() {
    let f = set_up();
    let origin = "github.com";
    expects_authentication(&f.io, origin, "my_username", "my_password");

    let added = Rc::new(RefCell::new(Vec::new()));
    f.config
        .borrow_mut()
        .set_auth_config_source(Box::new(ConfigSourceMock {
            name: "https://api.gitlab.com/source".to_string(),
            added: added.clone(),
        }));

    f.auth_helper
        .store_auth(origin, StoreAuth::Bool(true))
        .unwrap();

    assert_eq!(
        *added.borrow(),
        vec![(
            "http-basic.github.com".to_string(),
            expected_auth_setting("my_username", "my_password"),
        )]
    );
}

#[test]
fn test_store_auth_with_prompt_yes_answer() {
    let f = set_up();
    let origin = "github.com";
    expects_authentication(&f.io, origin, "my_username", "my_password");
    let config_source_name = "https://api.gitlab.com/source";

    f.io.borrow_mut()
        .expects(
            vec![Expectation::ask(
                format!(
                    "Do you want to store credentials for {} in {} ? [Yn] ",
                    origin, config_source_name
                ),
                "y",
            )],
            false,
        )
        .unwrap();

    let added = Rc::new(RefCell::new(Vec::new()));
    f.config
        .borrow_mut()
        .set_auth_config_source(Box::new(ConfigSourceMock {
            name: config_source_name.to_string(),
            added: added.clone(),
        }));

    f.auth_helper.store_auth(origin, StoreAuth::Prompt).unwrap();

    assert_eq!(
        *added.borrow(),
        vec![(
            "http-basic.github.com".to_string(),
            expected_auth_setting("my_username", "my_password"),
        )]
    );
}

#[test]
fn test_store_auth_with_prompt_no_answer() {
    let f = set_up();
    let origin = "github.com";
    let config_source_name = "https://api.gitlab.com/source";

    f.io.borrow_mut()
        .expects(
            vec![Expectation::ask(
                format!(
                    "Do you want to store credentials for {} in {} ? [Yn] ",
                    origin, config_source_name
                ),
                "n",
            )],
            false,
        )
        .unwrap();

    let added = Rc::new(RefCell::new(Vec::new()));
    f.config
        .borrow_mut()
        .set_auth_config_source(Box::new(ConfigSourceMock {
            name: config_source_name.to_string(),
            added: added.clone(),
        }));

    f.auth_helper.store_auth(origin, StoreAuth::Prompt).unwrap();

    assert!(added.borrow().is_empty());
}

#[test]
#[ignore = "PHP catches a RuntimeException from the validator; the Rust QuestionHelper exhausts \
input and panics via expect() rather than returning Err, so it cannot be represented as a \
recoverable Result"]
fn test_store_auth_with_prompt_invalid_answer() {
    todo!()
}

#[test]
#[ignore = "needs the extra prompt_auth_if_needed params (headers/retry_count/response_body) and \
GitLab::authorize_oauth which shells out to real `git config`; depends on host git state and is a \
design-level port concern"]
fn test_prompt_auth_if_needed_git_lab_no_auth_change() {
    todo!()
}

#[test]
#[ignore = "drives Bitbucket::request_token over the network and relies on willReturnCallback \
sequencing of getAuthentication; design-level port concern"]
fn test_prompt_auth_if_needed_multiple_bitbucket_downloads() {
    todo!()
}

#[test]
#[ignore = "exercises the deprecated addAuthenticationHeader wrapper (not ported) which relies on \
trigger_error/E_USER_DEPRECATED; the PHP error-handler subsystem is not modeled"]
fn test_add_authentication_header_with_custom_headers() {
    todo!()
}

#[test]
#[ignore = "exercises the deprecated addAuthenticationHeader wrapper (not ported) which relies on \
trigger_error/E_USER_DEPRECATED; the PHP error-handler subsystem is not modeled"]
fn test_add_authentication_header_is_working() {
    todo!()
}

#[test]
#[ignore = "exercises the deprecated addAuthenticationHeader wrapper (not ported) which relies on \
trigger_error/E_USER_DEPRECATED converted to a RuntimeException via set_error_handler; not modeled"]
fn test_add_authentication_header_deprecation() {
    todo!()
}
