//! ref: composer/tests/Composer/Test/ConfigTest.php sha1:3e56d46bbf990dfe2ea9afa25fa3e5b00557dabe

use bstr::{B, BStr, BString, ByteSlice};
use indexmap::IndexMap;
use serial_test::serial;
use shirabe::{
    advisory::Auditor,
    config::Config,
    io::{IOInterface, io_interface},
    util::Platform,
};
use shirabe_php_shim::{PhpMixed, bformat, error_message, rtrim_chars};

#[path = "common/io_mock.rs"]
#[allow(dead_code)] // io_mock exposes more helpers than this binary uses
mod io_mock;
use io_mock::{Expectation, get_io_mock};

/// Builds a `['config' => {...}]` map for `Config::merge`.
fn config_section(pairs: Vec<(&BStr, PhpMixed)>) -> IndexMap<BString, PhpMixed> {
    let mut m: IndexMap<BString, PhpMixed> = IndexMap::new();
    m.insert("config".into(), PhpMixed::Array(map(pairs)));
    m
}

/// PHP assertEquals on associative arrays compares pairs irrespective of order.
fn assert_map_equals(expected: &IndexMap<BString, PhpMixed>, actual: &IndexMap<BString, PhpMixed>) {
    assert_eq!(expected.len(), actual.len());
    for (key, value) in expected {
        assert_eq!(Some(value), actual.get(key), "key {key:?}");
    }
}

fn repo(r#type: &BStr, url: &BStr) -> PhpMixed {
    let mut m: IndexMap<BString, PhpMixed> = IndexMap::new();
    m.insert("type".into(), PhpMixed::string(bformat!("{}", r#type)));
    m.insert("url".into(), PhpMixed::string(bformat!("{url}")));
    PhpMixed::Array(m)
}

fn map(pairs: Vec<(&BStr, PhpMixed)>) -> IndexMap<BString, PhpMixed> {
    pairs
        .into_iter()
        .map(|(k, v)| (bformat!("{k}"), v))
        .collect()
}

fn disable(name: &BStr) -> PhpMixed {
    PhpMixed::Array(map(vec![(name, PhpMixed::Bool(false))]))
}

fn packagist() -> PhpMixed {
    repo(
        BStr::new("composer"),
        BStr::new("https://repo.packagist.org"),
    )
}

struct Case {
    expected: IndexMap<BString, PhpMixed>,
    local: IndexMap<BString, PhpMixed>,
    system: Option<IndexMap<BString, PhpMixed>>,
}

/// ref: ConfigTest::dataAddPackagistRepository
fn data_add_packagist_repository() -> Vec<Case> {
    vec![
        // local config inherits system defaults
        Case {
            expected: map(vec![(BStr::new("packagist.org"), packagist())]),
            local: map(vec![]),
            system: None,
        },
        // local config can disable system config by name
        Case {
            expected: map(vec![]),
            local: map(vec![(BStr::new("0"), disable(BStr::new("packagist.org")))]),
            system: None,
        },
        // local config can disable system config by name bc
        Case {
            expected: map(vec![]),
            local: map(vec![(BStr::new("0"), disable(BStr::new("packagist")))]),
            system: None,
        },
        // local config adds above defaults
        Case {
            expected: map(vec![
                (
                    BStr::new("0"),
                    repo(
                        BStr::new("vcs"),
                        BStr::new("git://github.com/composer/composer.git"),
                    ),
                ),
                (
                    BStr::new("1"),
                    repo(BStr::new("pear"), BStr::new("http://pear.composer.org")),
                ),
                (BStr::new("packagist.org"), packagist()),
            ]),
            local: map(vec![
                (
                    BStr::new("0"),
                    repo(
                        BStr::new("vcs"),
                        BStr::new("git://github.com/composer/composer.git"),
                    ),
                ),
                (
                    BStr::new("1"),
                    repo(BStr::new("pear"), BStr::new("http://pear.composer.org")),
                ),
            ]),
            system: None,
        },
        // system config adds above core defaults
        Case {
            expected: map(vec![
                (
                    BStr::new("example.com"),
                    repo(BStr::new("composer"), BStr::new("http://example.com")),
                ),
                (BStr::new("packagist.org"), packagist()),
            ]),
            local: map(vec![]),
            system: Some(map(vec![(
                BStr::new("example.com"),
                repo(BStr::new("composer"), BStr::new("http://example.com")),
            )])),
        },
        // local config can disable repos by name and re-add them anonymously to bring them above system config
        Case {
            expected: map(vec![
                (
                    BStr::new("1"),
                    repo(BStr::new("composer"), BStr::new("http://packagist.org")),
                ),
                (
                    BStr::new("example.com"),
                    repo(BStr::new("composer"), BStr::new("http://example.com")),
                ),
            ]),
            local: map(vec![
                (BStr::new("0"), disable(BStr::new("packagist.org"))),
                (
                    BStr::new("1"),
                    repo(BStr::new("composer"), BStr::new("http://packagist.org")),
                ),
            ]),
            system: Some(map(vec![(
                BStr::new("example.com"),
                repo(BStr::new("composer"), BStr::new("http://example.com")),
            )])),
        },
        // local config can override by name to bring a repo above system config
        Case {
            expected: map(vec![
                (
                    BStr::new("packagist.org"),
                    repo(BStr::new("composer"), BStr::new("http://packagistnew.org")),
                ),
                (
                    BStr::new("example.com"),
                    repo(BStr::new("composer"), BStr::new("http://example.com")),
                ),
            ]),
            local: map(vec![(
                BStr::new("packagist.org"),
                repo(BStr::new("composer"), BStr::new("http://packagistnew.org")),
            )]),
            system: Some(map(vec![(
                BStr::new("example.com"),
                repo(BStr::new("composer"), BStr::new("http://example.com")),
            )])),
        },
        // local config redefining packagist.org by URL override it if no named keys are used
        Case {
            expected: map(vec![(
                BStr::new("0"),
                repo(
                    BStr::new("composer"),
                    BStr::new("https://repo.packagist.org"),
                ),
            )]),
            local: map(vec![(
                BStr::new("0"),
                repo(
                    BStr::new("composer"),
                    BStr::new("https://repo.packagist.org"),
                ),
            )]),
            system: None,
        },
        // local config redefining packagist.org by URL override it also with named keys
        Case {
            expected: map(vec![(
                BStr::new("example"),
                repo(
                    BStr::new("composer"),
                    BStr::new("https://repo.packagist.org"),
                ),
            )]),
            local: map(vec![(
                BStr::new("example"),
                repo(
                    BStr::new("composer"),
                    BStr::new("https://repo.packagist.org"),
                ),
            )]),
            system: None,
        },
        // incorrect local config does not cause ErrorException
        Case {
            expected: map(vec![
                (BStr::new("packagist.org"), packagist()),
                (BStr::new("type"), PhpMixed::string(BString::from("vcs"))),
                (
                    BStr::new("url"),
                    PhpMixed::string(BString::from("http://example.com")),
                ),
            ]),
            local: map(vec![
                (BStr::new("type"), PhpMixed::string(BString::from("vcs"))),
                (
                    BStr::new("url"),
                    PhpMixed::string(BString::from("http://example.com")),
                ),
            ]),
            system: None,
        },
    ]
}

#[test]
fn test_add_packagist_repository() {
    for case in data_add_packagist_repository() {
        let mut config = Config::new(false, None);
        if let Some(system) = case.system {
            let mut cfg: IndexMap<BString, PhpMixed> = IndexMap::new();
            cfg.insert("repositories".into(), PhpMixed::Array(system));
            config.merge(&cfg, "test");
        }
        let mut cfg: IndexMap<BString, PhpMixed> = IndexMap::new();
        cfg.insert("repositories".into(), PhpMixed::Array(case.local));
        config.merge(&cfg, "test");

        let actual = config.get_repositories();

        // PHP assertEquals on arrays compares pairs irrespective of order.
        assert_eq!(case.expected.len(), actual.len());
        for (key, value) in &case.expected {
            assert_eq!(Some(value), actual.get(key), "repository key {key:?}");
        }
    }
}

// The remaining ConfigTest cases either read process env via Platform (process-timeout,
// htaccess-protect, var/realpath replacement, oauth, audit, ...) without the env isolation
// their setUp/tearDown provides, or exercise plugin-config merge details. They are not
// ported yet.
#[test]
fn test_preferred_install_as_string() {
    let mut config = Config::new(false, None);
    config.merge(
        &config_section(vec![(
            BStr::new("preferred-install"),
            PhpMixed::string(BString::from("source")),
        )]),
        "test",
    );
    config.merge(
        &config_section(vec![(
            BStr::new("preferred-install"),
            PhpMixed::string(BString::from("dist")),
        )]),
        "test",
    );

    assert_eq!(
        PhpMixed::string(BString::from("dist")),
        config.get("preferred-install").unwrap()
    );
}

#[test]
fn test_merge_preferred_install() {
    let mut config = Config::new(false, None);
    config.merge(
        &config_section(vec![(
            BStr::new("preferred-install"),
            PhpMixed::string(BString::from("dist")),
        )]),
        "test",
    );
    config.merge(
        &config_section(vec![(
            BStr::new("preferred-install"),
            PhpMixed::Array(map(vec![(
                BStr::new("foo/*"),
                PhpMixed::string(BString::from("source")),
            )])),
        )]),
        "test",
    );

    // This assertion needs to make sure full wildcard preferences are placed last
    // Handled by composer because we convert string preferences for BC, all other
    // care for ordering and collision prevention is up to the user
    let expected = map(vec![
        (
            BStr::new("foo/*"),
            PhpMixed::string(BString::from("source")),
        ),
        (BStr::new("*"), PhpMixed::string(BString::from("dist"))),
    ]);
    match config.get("preferred-install").unwrap() {
        PhpMixed::Array(actual) => assert_map_equals(&expected, &actual),
        other => panic!("expected array, got {other:?}"),
    }
}

#[test]
fn test_merge_github_oauth() {
    let mut config = Config::new(false, None);
    config.merge(
        &config_section(vec![(
            BStr::new("github-oauth"),
            PhpMixed::Array(map(vec![(
                BStr::new("foo"),
                PhpMixed::string(BString::from("bar")),
            )])),
        )]),
        "test",
    );
    config.merge(
        &config_section(vec![(
            BStr::new("github-oauth"),
            PhpMixed::Array(map(vec![(
                BStr::new("bar"),
                PhpMixed::string(BString::from("baz")),
            )])),
        )]),
        "test",
    );

    let expected = map(vec![
        (BStr::new("foo"), PhpMixed::string(BString::from("bar"))),
        (BStr::new("bar"), PhpMixed::string(BString::from("baz"))),
    ]);
    match config.get("github-oauth").unwrap() {
        PhpMixed::Array(actual) => assert_map_equals(&expected, &actual),
        other => panic!("expected array, got {other:?}"),
    }
}

#[test]
fn test_var_replacement() {
    let mut config = Config::new(false, None);
    config.merge(
        &config_section(vec![
            (BStr::new("a"), PhpMixed::string(BString::from("b"))),
            (BStr::new("c"), PhpMixed::string(BString::from("{$a}"))),
        ]),
        "test",
    );
    config.merge(
        &config_section(vec![
            (
                BStr::new("bin-dir"),
                PhpMixed::string(BString::from("$HOME")),
            ),
            (
                BStr::new("cache-dir"),
                PhpMixed::string(BString::from("~/foo/")),
            ),
        ]),
        "test",
    );

    let home_raw = Platform::get_env("HOME")
        .filter(|s| !s.is_empty())
        .or_else(|| Platform::get_env("USERPROFILE"))
        .unwrap_or_default();
    let home = rtrim_chars(&home_raw, "\\/");
    assert_eq!(
        PhpMixed::string(BString::from("b")),
        config.get("c").unwrap()
    );
    assert_eq!(
        PhpMixed::string(home.clone()),
        config.get("bin-dir").unwrap()
    );
    assert_eq!(
        PhpMixed::string(bformat!("{home}/foo")),
        config.get("cache-dir").unwrap()
    );
}

#[test]
fn test_realpath_replacement() {
    let mut config = Config::new(false, Some("/foo/bar".into()));
    config.merge(
        &config_section(vec![
            (
                BStr::new("bin-dir"),
                PhpMixed::string(BString::from("$HOME/foo")),
            ),
            (
                BStr::new("cache-dir"),
                PhpMixed::string(BString::from("/baz/")),
            ),
            (
                BStr::new("vendor-dir"),
                PhpMixed::string(BString::from("vendor")),
            ),
        ]),
        "test",
    );

    let home_raw = Platform::get_env("HOME")
        .filter(|s| !s.is_empty())
        .or_else(|| Platform::get_env("USERPROFILE"))
        .unwrap_or_default();
    let home = rtrim_chars(&home_raw, "\\/");
    assert_eq!(
        PhpMixed::string(BString::from("/foo/bar/vendor")),
        config.get("vendor-dir").unwrap()
    );
    assert_eq!(
        PhpMixed::string(bformat!("{home}/foo")),
        config.get("bin-dir").unwrap()
    );
    assert_eq!(
        PhpMixed::string(BString::from("/baz")),
        config.get("cache-dir").unwrap()
    );
}

#[test]
fn test_stream_wrapper_dirs() {
    let mut config = Config::new(false, Some("/foo/bar".into()));
    config.merge(
        &config_section(vec![(
            BStr::new("cache-dir"),
            PhpMixed::string(BString::from("s3://baz/")),
        )]),
        "test",
    );

    assert_eq!(
        PhpMixed::string(BString::from("s3://baz")),
        config.get("cache-dir").unwrap()
    );
}

#[test]
fn test_fetching_relative_paths() {
    let mut config = Config::new(false, Some("/foo/bar".into()));
    config.merge(
        &config_section(vec![
            (
                BStr::new("bin-dir"),
                PhpMixed::string(BString::from("{$vendor-dir}/foo")),
            ),
            (
                BStr::new("vendor-dir"),
                PhpMixed::string(BString::from("vendor")),
            ),
        ]),
        "test",
    );

    assert_eq!(
        PhpMixed::string(BString::from("/foo/bar/vendor")),
        config.get("vendor-dir").unwrap()
    );
    assert_eq!(
        PhpMixed::string(BString::from("/foo/bar/vendor/foo")),
        config.get("bin-dir").unwrap()
    );
    assert_eq!(
        PhpMixed::string(BString::from("vendor")),
        config
            .get_with_flags("vendor-dir", Config::RELATIVE_PATHS)
            .unwrap()
    );
    assert_eq!(
        PhpMixed::string(BString::from("vendor/foo")),
        config
            .get_with_flags("bin-dir", Config::RELATIVE_PATHS)
            .unwrap()
    );
}

#[test]
fn test_override_github_protocols() {
    let mut config = Config::new(false, None);
    config.merge(
        &config_section(vec![(
            BStr::new("github-protocols"),
            PhpMixed::List(vec![
                PhpMixed::string(BString::from("https")),
                PhpMixed::string(BString::from("ssh")),
            ]),
        )]),
        "test",
    );
    config.merge(
        &config_section(vec![(
            BStr::new("github-protocols"),
            PhpMixed::List(vec![PhpMixed::string(BString::from("https"))]),
        )]),
        "test",
    );

    assert_eq!(
        PhpMixed::List(vec![PhpMixed::string(BString::from("https"))]),
        config.get("github-protocols").unwrap()
    );
}

#[test]
fn test_git_disabled_by_default_in_github_protocols() {
    let mut config = Config::new(false, None);
    config.merge(
        &config_section(vec![(
            BStr::new("github-protocols"),
            PhpMixed::List(vec![
                PhpMixed::string(BString::from("https")),
                PhpMixed::string(BString::from("git")),
            ]),
        )]),
        "test",
    );
    assert_eq!(
        PhpMixed::List(vec![PhpMixed::string(BString::from("https"))]),
        config.get("github-protocols").unwrap()
    );

    config.merge(
        &config_section(vec![(BStr::new("secure-http"), PhpMixed::Bool(false))]),
        "test",
    );
    assert_eq!(
        PhpMixed::List(vec![
            PhpMixed::string(BString::from("https")),
            PhpMixed::string(BString::from("git")),
        ]),
        config.get("github-protocols").unwrap()
    );
}

#[test]
fn test_allowed_urls_pass() {
    let urls = vec![
        "https://packagist.org",
        "git@github.com:composer/composer.git",
        "hg://user:pass@my.satis/satis",
        "\\\\myserver\\myplace.git",
        "file://myserver.localhost/mygit.git",
        "file://example.org/mygit.git",
        "git:Department/Repo.git",
        "ssh://[user@]host.xz[:port]/path/to/repo.git/",
    ];
    for url in urls {
        let mut config = Config::new(false, None);
        config
            .prohibit_url_by_config(url.into(), None, &IndexMap::new())
            .unwrap();
    }
}

#[test]
fn test_prohibited_urls_throw_exception() {
    let urls = vec![
        "http://packagist.org",
        "http://10.1.0.1/satis",
        "http://127.0.0.1/satis",
        "http://\u{1F49B}@example.org",
        "svn://localhost/trunk",
        "svn://will.not.resolve/trunk",
        "svn://192.168.0.1/trunk",
        "svn://1.2.3.4/trunk",
        "git://5.6.7.8/git.git",
    ];
    for url in urls {
        let mut config = Config::new(false, None);
        let err = config
            .prohibit_url_by_config(url.into(), None, &IndexMap::new())
            .unwrap_err();
        assert!(
            error_message(&err).contains_str(bformat!(
                "Your configuration does not allow connections to {url}"
            )),
            "url {url:?}: {err}"
        );
    }
}

#[test]
fn test_prohibited_urls_warning_verify_peer() {
    let (io_mock, _io_guard) = get_io_mock(io_interface::DEBUG).unwrap();
    io_mock
        .borrow_mut()
        .expects(
            vec![Expectation::text(
                "<warning>Warning: Accessing example.org with verify_peer and verify_peer_name disabled.</warning>",
            )],
            true,
        )
        .unwrap();

    let mut config = Config::new(false, None);
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io_mock;
    let mut repo_options: IndexMap<BString, PhpMixed> = IndexMap::new();
    repo_options.insert(
        "ssl".into(),
        PhpMixed::Array(map(vec![
            (BStr::new("verify_peer"), PhpMixed::Bool(false)),
            (BStr::new("verify_peer_name"), PhpMixed::Bool(false)),
        ])),
    );
    config
        .prohibit_url_by_config("https://example.org".into(), Some(io), &repo_options)
        .unwrap();
}

#[test]
fn test_disable_tls_can_be_overridden() {
    let mut config = Config::new(true, None);
    config.merge(
        &config_section(vec![(
            BStr::new("disable-tls"),
            PhpMixed::string(BString::from("false")),
        )]),
        "test",
    );
    assert_eq!(PhpMixed::Bool(false), config.get("disable-tls").unwrap());
    config.merge(
        &config_section(vec![(
            BStr::new("disable-tls"),
            PhpMixed::string(BString::from("true")),
        )]),
        "test",
    );
    assert_eq!(PhpMixed::Bool(true), config.get("disable-tls").unwrap());
}

#[test]
#[serial]
fn test_process_timeout() {
    Platform::put_env("COMPOSER_PROCESS_TIMEOUT", "0").unwrap();
    let config = Config::new(true, None);
    let result = config.get("process-timeout").unwrap();
    Platform::clear_env("COMPOSER_PROCESS_TIMEOUT").unwrap();

    assert_eq!(PhpMixed::Int(0), result);
}

#[test]
#[serial]
fn test_htaccess_protect() {
    Platform::put_env("COMPOSER_HTACCESS_PROTECT", "0").unwrap();
    let config = Config::new(true, None);
    let result = config.get("htaccess-protect").unwrap();
    Platform::clear_env("COMPOSER_HTACCESS_PROTECT").unwrap();

    assert_eq!(PhpMixed::Bool(false), result);
}

#[test]
#[serial]
fn test_get_source_of_value() {
    Platform::clear_env("COMPOSER_PROCESS_TIMEOUT").unwrap();

    let mut config = Config::new(true, None);

    assert_eq!(
        Config::SOURCE_DEFAULT,
        config.get_source_of_value("process-timeout").unwrap()
    );

    config.merge(
        &config_section(vec![(BStr::new("process-timeout"), PhpMixed::Int(1))]),
        "phpunit-test",
    );

    assert_eq!(
        "phpunit-test",
        config.get_source_of_value("process-timeout").unwrap()
    );
}

#[test]
#[serial]
fn test_get_source_of_value_env_variables() {
    Platform::put_env("COMPOSER_HTACCESS_PROTECT", "0").unwrap();
    let config = Config::new(true, None);
    let result = config.get_source_of_value("htaccess-protect").unwrap();
    Platform::clear_env("COMPOSER_HTACCESS_PROTECT").unwrap();

    assert_eq!("COMPOSER_HTACCESS_PROTECT", result);
}

#[test]
#[serial]
fn test_audit() {
    let mut config = Config::new(true, None);
    let result = config.get("audit").unwrap();
    let result = result.as_array().unwrap();
    assert!(result.contains_key(B("abandoned")));
    assert!(result.contains_key(B("ignore")));
    assert_eq!(
        Some(&PhpMixed::string(Auditor::ABANDONED_FAIL.to_owned())),
        result.get(B("abandoned"))
    );
    assert_eq!(
        Some(&PhpMixed::Array(IndexMap::new())),
        result.get(B("ignore"))
    );

    Platform::put_env("COMPOSER_AUDIT_ABANDONED", Auditor::ABANDONED_IGNORE).unwrap();
    let result = config.get("audit").unwrap();
    Platform::clear_env("COMPOSER_AUDIT_ABANDONED").unwrap();
    let result = result.as_array().unwrap();
    assert!(result.contains_key(B("abandoned")));
    assert!(result.contains_key(B("ignore")));
    assert_eq!(
        Some(&PhpMixed::string(Auditor::ABANDONED_IGNORE.to_owned())),
        result.get(B("abandoned"))
    );
    assert_eq!(
        Some(&PhpMixed::Array(IndexMap::new())),
        result.get(B("ignore"))
    );

    config.merge(
        &config_section(vec![(
            BStr::new("audit"),
            PhpMixed::Array(map(vec![(
                BStr::new("ignore"),
                PhpMixed::List(vec![
                    PhpMixed::string(BString::from("A")),
                    PhpMixed::string(BString::from("B")),
                ]),
            )])),
        )]),
        "test",
    );
    config.merge(
        &config_section(vec![(
            BStr::new("audit"),
            PhpMixed::Array(map(vec![(
                BStr::new("ignore"),
                PhpMixed::List(vec![
                    PhpMixed::string(BString::from("A")),
                    PhpMixed::string(BString::from("C")),
                ]),
            )])),
        )]),
        "test",
    );
    let result = config.get("audit").unwrap();
    let result = result.as_array().unwrap();
    assert!(result.contains_key(B("ignore")));
    assert_eq!(
        Some(&PhpMixed::List(vec![
            PhpMixed::string(BString::from("A")),
            PhpMixed::string(BString::from("B")),
            PhpMixed::string(BString::from("A")),
            PhpMixed::string(BString::from("C")),
        ])),
        result.get(B("ignore"))
    );

    // Test COMPOSER_SECURITY_BLOCKING_ABANDONED env var
    Platform::put_env("COMPOSER_SECURITY_BLOCKING_ABANDONED", "1").unwrap();
    let result = config.get("audit").unwrap();
    Platform::clear_env("COMPOSER_SECURITY_BLOCKING_ABANDONED").unwrap();
    let result = result.as_array().unwrap();
    assert!(result.contains_key(B("block-abandoned")));
    assert_eq!(
        Some(&PhpMixed::Bool(true)),
        result.get(B("block-abandoned"))
    );

    Platform::put_env("COMPOSER_SECURITY_BLOCKING_ABANDONED", "0").unwrap();
    let result = config.get("audit").unwrap();
    Platform::clear_env("COMPOSER_SECURITY_BLOCKING_ABANDONED").unwrap();
    let result = result.as_array().unwrap();
    assert!(result.contains_key(B("block-abandoned")));
    assert_eq!(
        Some(&PhpMixed::Bool(false)),
        result.get(B("block-abandoned"))
    );
}

#[test]
fn test_get_defaults_to_an_empty_array() {
    let config = Config::new(true, None);
    let keys = [
        "bitbucket-oauth",
        "github-oauth",
        "gitlab-oauth",
        "gitlab-token",
        "forgejo-token",
        "http-basic",
        "bearer",
    ];
    for key in keys {
        let value = config.get(key).unwrap();
        match value {
            PhpMixed::Array(m) => assert_eq!(0, m.len(), "key {key:?}"),
            other => panic!("key {key:?}: expected array, got {other:?}"),
        }
    }
}

#[test]
fn test_merges_plugin_config() {
    let mut config = Config::new(false, None);
    config.merge(
        &config_section(vec![(
            BStr::new("allow-plugins"),
            PhpMixed::Array(map(vec![(BStr::new("some/plugin"), PhpMixed::Bool(true))])),
        )]),
        "test",
    );
    match config.get("allow-plugins").unwrap() {
        PhpMixed::Array(actual) => assert_map_equals(
            &map(vec![(BStr::new("some/plugin"), PhpMixed::Bool(true))]),
            &actual,
        ),
        other => panic!("expected array, got {other:?}"),
    }

    config.merge(
        &config_section(vec![(
            BStr::new("allow-plugins"),
            PhpMixed::Array(map(vec![(
                BStr::new("another/plugin"),
                PhpMixed::Bool(true),
            )])),
        )]),
        "test",
    );
    match config.get("allow-plugins").unwrap() {
        PhpMixed::Array(actual) => assert_map_equals(
            &map(vec![
                (BStr::new("some/plugin"), PhpMixed::Bool(true)),
                (BStr::new("another/plugin"), PhpMixed::Bool(true)),
            ]),
            &actual,
        ),
        other => panic!("expected array, got {other:?}"),
    }
}

#[test]
fn test_overrides_global_boolean_plugins_config() {
    let mut config = Config::new(false, None);
    config.merge(
        &config_section(vec![(BStr::new("allow-plugins"), PhpMixed::Bool(true))]),
        "test",
    );
    assert_eq!(PhpMixed::Bool(true), config.get("allow-plugins").unwrap());

    config.merge(
        &config_section(vec![(
            BStr::new("allow-plugins"),
            PhpMixed::Array(map(vec![(
                BStr::new("another/plugin"),
                PhpMixed::Bool(true),
            )])),
        )]),
        "test",
    );
    match config.get("allow-plugins").unwrap() {
        PhpMixed::Array(actual) => assert_map_equals(
            &map(vec![(BStr::new("another/plugin"), PhpMixed::Bool(true))]),
            &actual,
        ),
        other => panic!("expected array, got {other:?}"),
    }
}

#[test]
fn test_allows_all_plugins_from_local_boolean() {
    let mut config = Config::new(false, None);
    config.merge(
        &config_section(vec![(
            BStr::new("allow-plugins"),
            PhpMixed::Array(map(vec![(BStr::new("some/plugin"), PhpMixed::Bool(true))])),
        )]),
        "test",
    );
    match config.get("allow-plugins").unwrap() {
        PhpMixed::Array(actual) => assert_map_equals(
            &map(vec![(BStr::new("some/plugin"), PhpMixed::Bool(true))]),
            &actual,
        ),
        other => panic!("expected array, got {other:?}"),
    }

    config.merge(
        &config_section(vec![(BStr::new("allow-plugins"), PhpMixed::Bool(true))]),
        "test",
    );
    assert_eq!(PhpMixed::Bool(true), config.get("allow-plugins").unwrap());
}
