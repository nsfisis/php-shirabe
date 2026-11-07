//! ref: composer/tests/Composer/Test/Util/SvnTest.php sha1:2845204ef100384d371b94832b3e2520c28e927e

use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::{IOInterface, NullIO};
use shirabe::util::Svn;
use shirabe_php_shim::{PhpMixed, ToPhpString};

fn map(pairs: Vec<(&BStr, PhpMixed)>) -> IndexMap<BString, PhpMixed> {
    pairs
        .into_iter()
        .map(|(k, v)| (k.to_php_string(), v))
        .collect()
}

/// Builds a `['config' => ['http-basic' => [host => ['username' => .., 'password' => ..]]]]`
/// map for `Config::merge`.
fn http_basic_config(host: &BStr, username: &BStr, password: &BStr) -> IndexMap<BString, PhpMixed> {
    let creds = map(vec![
        (
            BStr::new("username"),
            PhpMixed::string(username.to_php_string()),
        ),
        (
            BStr::new("password"),
            PhpMixed::string(password.to_php_string()),
        ),
    ]);
    let http_basic = map(vec![(host, PhpMixed::Array(creds))]);
    let config = map(vec![(BStr::new("http-basic"), PhpMixed::Array(http_basic))]);
    map(vec![(BStr::new("config"), PhpMixed::Array(config))])
}

/// ref: SvnTest::urlProvider
fn url_provider() -> Vec<(&'static BStr, Vec<&'static BStr>)> {
    vec![
        (
            BStr::new("http://till:test@svn.example.org/"),
            vec![
                BStr::new("--username"),
                BStr::new("till"),
                BStr::new("--password"),
                BStr::new("test"),
            ],
        ),
        (BStr::new("http://svn.apache.org/"), vec![]),
        (
            BStr::new("svn://johndoe@example.org"),
            vec![
                BStr::new("--username"),
                BStr::new("johndoe"),
                BStr::new("--password"),
                BStr::new(""),
            ],
        ),
    ]
}

#[test]
fn test_credentials() {
    for (url, expect) in url_provider() {
        let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
        let config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(true, None)));
        let mut svn = Svn::new(url.into(), io, config, None);

        let expect: Vec<BString> = expect.iter().map(|s| s.to_php_string()).collect();
        assert_eq!(expect, svn.__get_credential_args().unwrap());
    }
}

#[test]
fn test_interactive_string() {
    let url = "http://svn.example.org";

    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(true, None)));
    let mut svn = Svn::new(url.into(), io, config, None);

    assert_eq!(
        vec![
            BString::from("svn"),
            BString::from("ls"),
            BString::from("--non-interactive"),
            BString::from("--"),
            BString::from("http://svn.example.org"),
        ],
        svn.__get_command(vec!["svn".into(), "ls".into()], url.into(), None)
            .unwrap()
    );
}

#[test]
fn test_credentials_from_config() {
    let url = "http://svn.apache.org";

    let mut config = Config::new(true, None);
    config.merge(
        &http_basic_config(
            BStr::new("svn.apache.org"),
            BStr::new("foo"),
            BStr::new("bar"),
        ),
        "test",
    );

    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let mut svn = Svn::new(
        url.into(),
        io,
        std::rc::Rc::new(std::cell::RefCell::new(config)),
        None,
    );

    assert_eq!(
        vec![
            BString::from("--username"),
            BString::from("foo"),
            BString::from("--password"),
            BString::from("bar"),
        ],
        svn.__get_credential_args().unwrap()
    );
}

#[test]
fn test_credentials_from_config_with_cache_credentials_true() {
    let url = "http://svn.apache.org";

    let mut config = Config::new(true, None);
    config.merge(
        &http_basic_config(
            BStr::new("svn.apache.org"),
            BStr::new("foo"),
            BStr::new("bar"),
        ),
        "test",
    );

    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let mut svn = Svn::new(
        url.into(),
        io,
        std::rc::Rc::new(std::cell::RefCell::new(config)),
        None,
    );
    svn.set_cache_credentials(true);

    assert_eq!(
        vec![
            BString::from("--username"),
            BString::from("foo"),
            BString::from("--password"),
            BString::from("bar"),
        ],
        svn.__get_credential_args().unwrap()
    );
}

#[test]
fn test_credentials_from_config_with_cache_credentials_false() {
    let url = "http://svn.apache.org";

    let mut config = Config::new(true, None);
    config.merge(
        &http_basic_config(
            BStr::new("svn.apache.org"),
            BStr::new("foo"),
            BStr::new("bar"),
        ),
        "test",
    );

    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let mut svn = Svn::new(
        url.into(),
        io,
        std::rc::Rc::new(std::cell::RefCell::new(config)),
        None,
    );
    svn.set_cache_credentials(false);

    assert_eq!(
        vec![
            BString::from("--no-auth-cache"),
            BString::from("--username"),
            BString::from("foo"),
            BString::from("--password"),
            BString::from("bar"),
        ],
        svn.__get_credential_args().unwrap()
    );
}
