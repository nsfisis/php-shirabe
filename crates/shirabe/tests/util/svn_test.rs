//! ref: composer/tests/Composer/Test/Util/SvnTest.php

use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::util::svn::Svn;
use shirabe_php_shim::PhpMixed;

fn map(pairs: Vec<(&str, PhpMixed)>) -> IndexMap<String, PhpMixed> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

/// Builds a `['config' => ['http-basic' => [host => ['username' => .., 'password' => ..]]]]`
/// map for `Config::merge`.
fn http_basic_config(host: &str, username: &str, password: &str) -> IndexMap<String, PhpMixed> {
    let creds = map(vec![
        ("username", PhpMixed::String(username.to_string())),
        ("password", PhpMixed::String(password.to_string())),
    ]);
    let http_basic = map(vec![(host, PhpMixed::Array(creds))]);
    let config = map(vec![("http-basic", PhpMixed::Array(http_basic))]);
    map(vec![("config", PhpMixed::Array(config))])
}

/// ref: SvnTest::urlProvider
fn url_provider() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        (
            "http://till:test@svn.example.org/",
            vec!["--username", "till", "--password", "test"],
        ),
        ("http://svn.apache.org/", vec![]),
        (
            "svn://johndoe@example.org",
            vec!["--username", "johndoe", "--password", ""],
        ),
    ]
}

#[test]
fn test_credentials() {
    for (url, expect) in url_provider() {
        let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
        let config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(true, None)));
        let mut svn = Svn::new(url.to_string(), io, config, None);

        let expect: Vec<String> = expect.iter().map(|s| s.to_string()).collect();
        assert_eq!(expect, svn.__get_credential_args());
    }
}

#[test]
fn test_interactive_string() {
    let url = "http://svn.example.org";

    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(true, None)));
    let mut svn = Svn::new(url.to_string(), io, config, None);

    assert_eq!(
        vec![
            "svn".to_string(),
            "ls".to_string(),
            "--non-interactive".to_string(),
            "--".to_string(),
            "http://svn.example.org".to_string(),
        ],
        svn.__get_command(vec!["svn".to_string(), "ls".to_string()], url, None)
    );
}

#[test]
fn test_credentials_from_config() {
    let url = "http://svn.apache.org";

    let mut config = Config::new(true, None);
    config.merge(&http_basic_config("svn.apache.org", "foo", "bar"), "test");

    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let mut svn = Svn::new(
        url.to_string(),
        io,
        std::rc::Rc::new(std::cell::RefCell::new(config)),
        None,
    );

    assert_eq!(
        vec![
            "--username".to_string(),
            "foo".to_string(),
            "--password".to_string(),
            "bar".to_string(),
        ],
        svn.__get_credential_args()
    );
}

#[test]
fn test_credentials_from_config_with_cache_credentials_true() {
    let url = "http://svn.apache.org";

    let mut config = Config::new(true, None);
    config.merge(&http_basic_config("svn.apache.org", "foo", "bar"), "test");

    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let mut svn = Svn::new(
        url.to_string(),
        io,
        std::rc::Rc::new(std::cell::RefCell::new(config)),
        None,
    );
    svn.set_cache_credentials(true);

    assert_eq!(
        vec![
            "--username".to_string(),
            "foo".to_string(),
            "--password".to_string(),
            "bar".to_string(),
        ],
        svn.__get_credential_args()
    );
}

#[test]
fn test_credentials_from_config_with_cache_credentials_false() {
    let url = "http://svn.apache.org";

    let mut config = Config::new(true, None);
    config.merge(&http_basic_config("svn.apache.org", "foo", "bar"), "test");

    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let mut svn = Svn::new(
        url.to_string(),
        io,
        std::rc::Rc::new(std::cell::RefCell::new(config)),
        None,
    );
    svn.set_cache_credentials(false);

    assert_eq!(
        vec![
            "--no-auth-cache".to_string(),
            "--username".to_string(),
            "foo".to_string(),
            "--password".to_string(),
            "bar".to_string(),
        ],
        svn.__get_credential_args()
    );
}
