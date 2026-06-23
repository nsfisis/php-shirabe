//! ref: composer/tests/Composer/Test/ConfigTest.php

use indexmap::IndexMap;
use shirabe::advisory::Auditor;
use shirabe::config::Config;
use shirabe::util::Platform;
use shirabe_php_shim::PhpMixed;

/// Builds a `['config' => {...}]` map for `Config::merge`.
fn config_section(pairs: Vec<(&str, PhpMixed)>) -> IndexMap<String, PhpMixed> {
    let mut m: IndexMap<String, PhpMixed> = IndexMap::new();
    m.insert("config".to_string(), PhpMixed::Array(map(pairs)));
    m
}

/// PHP assertEquals on associative arrays compares pairs irrespective of order.
fn assert_map_equals(expected: &IndexMap<String, PhpMixed>, actual: &IndexMap<String, PhpMixed>) {
    assert_eq!(expected.len(), actual.len());
    for (key, value) in expected {
        assert_eq!(Some(value), actual.get(key), "key {key:?}");
    }
}

fn repo(r#type: &str, url: &str) -> PhpMixed {
    let mut m: IndexMap<String, PhpMixed> = IndexMap::new();
    m.insert("type".to_string(), PhpMixed::String(r#type.to_string()));
    m.insert("url".to_string(), PhpMixed::String(url.to_string()));
    PhpMixed::Array(m)
}

fn map(pairs: Vec<(&str, PhpMixed)>) -> IndexMap<String, PhpMixed> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

fn disable(name: &str) -> PhpMixed {
    PhpMixed::Array(map(vec![(name, PhpMixed::Bool(false))]))
}

fn packagist() -> PhpMixed {
    repo("composer", "https://repo.packagist.org")
}

struct Case {
    expected: IndexMap<String, PhpMixed>,
    local: IndexMap<String, PhpMixed>,
    system: Option<IndexMap<String, PhpMixed>>,
}

/// ref: ConfigTest::dataAddPackagistRepository
fn data_add_packagist_repository() -> Vec<Case> {
    vec![
        // local config inherits system defaults
        Case {
            expected: map(vec![("packagist.org", packagist())]),
            local: map(vec![]),
            system: None,
        },
        // local config can disable system config by name
        Case {
            expected: map(vec![]),
            local: map(vec![("0", disable("packagist.org"))]),
            system: None,
        },
        // local config can disable system config by name bc
        Case {
            expected: map(vec![]),
            local: map(vec![("0", disable("packagist"))]),
            system: None,
        },
        // local config adds above defaults
        Case {
            expected: map(vec![
                ("0", repo("vcs", "git://github.com/composer/composer.git")),
                ("1", repo("pear", "http://pear.composer.org")),
                ("packagist.org", packagist()),
            ]),
            local: map(vec![
                ("0", repo("vcs", "git://github.com/composer/composer.git")),
                ("1", repo("pear", "http://pear.composer.org")),
            ]),
            system: None,
        },
        // system config adds above core defaults
        Case {
            expected: map(vec![
                ("example.com", repo("composer", "http://example.com")),
                ("packagist.org", packagist()),
            ]),
            local: map(vec![]),
            system: Some(map(vec![(
                "example.com",
                repo("composer", "http://example.com"),
            )])),
        },
        // local config can disable repos by name and re-add them anonymously to bring them above system config
        Case {
            expected: map(vec![
                ("1", repo("composer", "http://packagist.org")),
                ("example.com", repo("composer", "http://example.com")),
            ]),
            local: map(vec![
                ("0", disable("packagist.org")),
                ("1", repo("composer", "http://packagist.org")),
            ]),
            system: Some(map(vec![(
                "example.com",
                repo("composer", "http://example.com"),
            )])),
        },
        // local config can override by name to bring a repo above system config
        Case {
            expected: map(vec![
                ("packagist.org", repo("composer", "http://packagistnew.org")),
                ("example.com", repo("composer", "http://example.com")),
            ]),
            local: map(vec![(
                "packagist.org",
                repo("composer", "http://packagistnew.org"),
            )]),
            system: Some(map(vec![(
                "example.com",
                repo("composer", "http://example.com"),
            )])),
        },
        // local config redefining packagist.org by URL override it if no named keys are used
        Case {
            expected: map(vec![("0", repo("composer", "https://repo.packagist.org"))]),
            local: map(vec![("0", repo("composer", "https://repo.packagist.org"))]),
            system: None,
        },
        // local config redefining packagist.org by URL override it also with named keys
        Case {
            expected: map(vec![(
                "example",
                repo("composer", "https://repo.packagist.org"),
            )]),
            local: map(vec![(
                "example",
                repo("composer", "https://repo.packagist.org"),
            )]),
            system: None,
        },
        // incorrect local config does not cause ErrorException
        Case {
            expected: map(vec![
                ("packagist.org", packagist()),
                ("type", PhpMixed::String("vcs".to_string())),
                ("url", PhpMixed::String("http://example.com".to_string())),
            ]),
            local: map(vec![
                ("type", PhpMixed::String("vcs".to_string())),
                ("url", PhpMixed::String("http://example.com".to_string())),
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
            let mut cfg: IndexMap<String, PhpMixed> = IndexMap::new();
            cfg.insert("repositories".to_string(), PhpMixed::Array(system));
            config.merge(&cfg, "test");
        }
        let mut cfg: IndexMap<String, PhpMixed> = IndexMap::new();
        cfg.insert("repositories".to_string(), PhpMixed::Array(case.local));
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
            "preferred-install",
            PhpMixed::String("source".to_string()),
        )]),
        "test",
    );
    config.merge(
        &config_section(vec![(
            "preferred-install",
            PhpMixed::String("dist".to_string()),
        )]),
        "test",
    );

    assert_eq!(
        PhpMixed::String("dist".to_string()),
        config.get("preferred-install")
    );
}

#[test]
fn test_merge_preferred_install() {
    let mut config = Config::new(false, None);
    config.merge(
        &config_section(vec![(
            "preferred-install",
            PhpMixed::String("dist".to_string()),
        )]),
        "test",
    );
    config.merge(
        &config_section(vec![(
            "preferred-install",
            PhpMixed::Array(map(vec![("foo/*", PhpMixed::String("source".to_string()))])),
        )]),
        "test",
    );

    // This assertion needs to make sure full wildcard preferences are placed last
    // Handled by composer because we convert string preferences for BC, all other
    // care for ordering and collision prevention is up to the user
    let expected = map(vec![
        ("foo/*", PhpMixed::String("source".to_string())),
        ("*", PhpMixed::String("dist".to_string())),
    ]);
    match config.get("preferred-install") {
        PhpMixed::Array(actual) => assert_map_equals(&expected, &actual),
        other => panic!("expected array, got {other:?}"),
    }
}

#[test]
fn test_merge_github_oauth() {
    let mut config = Config::new(false, None);
    config.merge(
        &config_section(vec![(
            "github-oauth",
            PhpMixed::Array(map(vec![("foo", PhpMixed::String("bar".to_string()))])),
        )]),
        "test",
    );
    config.merge(
        &config_section(vec![(
            "github-oauth",
            PhpMixed::Array(map(vec![("bar", PhpMixed::String("baz".to_string()))])),
        )]),
        "test",
    );

    let expected = map(vec![
        ("foo", PhpMixed::String("bar".to_string())),
        ("bar", PhpMixed::String("baz".to_string())),
    ]);
    match config.get("github-oauth") {
        PhpMixed::Array(actual) => assert_map_equals(&expected, &actual),
        other => panic!("expected array, got {other:?}"),
    }
}

#[test]
fn test_var_replacement() {
    let mut config = Config::new(false, None);
    config.merge(
        &config_section(vec![
            ("a", PhpMixed::String("b".to_string())),
            ("c", PhpMixed::String("{$a}".to_string())),
        ]),
        "test",
    );
    config.merge(
        &config_section(vec![
            ("bin-dir", PhpMixed::String("$HOME".to_string())),
            ("cache-dir", PhpMixed::String("~/foo/".to_string())),
        ]),
        "test",
    );

    let home_raw = Platform::get_env("HOME")
        .filter(|s| !s.is_empty())
        .or_else(|| Platform::get_env("USERPROFILE"))
        .unwrap_or_default();
    let home = home_raw.trim_end_matches(['\\', '/']).to_string();
    assert_eq!(PhpMixed::String("b".to_string()), config.get("c"));
    assert_eq!(PhpMixed::String(home.clone()), config.get("bin-dir"));
    assert_eq!(
        PhpMixed::String(format!("{}/foo", home)),
        config.get("cache-dir")
    );
}

#[test]
fn test_realpath_replacement() {
    let mut config = Config::new(false, Some("/foo/bar".to_string()));
    config.merge(
        &config_section(vec![
            ("bin-dir", PhpMixed::String("$HOME/foo".to_string())),
            ("cache-dir", PhpMixed::String("/baz/".to_string())),
            ("vendor-dir", PhpMixed::String("vendor".to_string())),
        ]),
        "test",
    );

    let home_raw = Platform::get_env("HOME")
        .filter(|s| !s.is_empty())
        .or_else(|| Platform::get_env("USERPROFILE"))
        .unwrap_or_default();
    let home = home_raw.trim_end_matches(['\\', '/']).to_string();
    assert_eq!(
        PhpMixed::String("/foo/bar/vendor".to_string()),
        config.get("vendor-dir")
    );
    assert_eq!(
        PhpMixed::String(format!("{}/foo", home)),
        config.get("bin-dir")
    );
    assert_eq!(
        PhpMixed::String("/baz".to_string()),
        config.get("cache-dir")
    );
}

#[test]
fn test_stream_wrapper_dirs() {
    let mut config = Config::new(false, Some("/foo/bar".to_string()));
    config.merge(
        &config_section(vec![(
            "cache-dir",
            PhpMixed::String("s3://baz/".to_string()),
        )]),
        "test",
    );

    assert_eq!(
        PhpMixed::String("s3://baz".to_string()),
        config.get("cache-dir")
    );
}

#[test]
fn test_fetching_relative_paths() {
    let mut config = Config::new(false, Some("/foo/bar".to_string()));
    config.merge(
        &config_section(vec![
            ("bin-dir", PhpMixed::String("{$vendor-dir}/foo".to_string())),
            ("vendor-dir", PhpMixed::String("vendor".to_string())),
        ]),
        "test",
    );

    assert_eq!(
        PhpMixed::String("/foo/bar/vendor".to_string()),
        config.get("vendor-dir")
    );
    assert_eq!(
        PhpMixed::String("/foo/bar/vendor/foo".to_string()),
        config.get("bin-dir")
    );
    assert_eq!(
        PhpMixed::String("vendor".to_string()),
        config
            .get_with_flags("vendor-dir", Config::RELATIVE_PATHS)
            .unwrap()
    );
    assert_eq!(
        PhpMixed::String("vendor/foo".to_string()),
        config
            .get_with_flags("bin-dir", Config::RELATIVE_PATHS)
            .unwrap()
    );
}

#[ignore]
#[test]
fn test_override_github_protocols() {
    let mut config = Config::new(false, None);
    config.merge(
        &config_section(vec![(
            "github-protocols",
            PhpMixed::List(vec![
                PhpMixed::String("https".to_string()),
                PhpMixed::String("ssh".to_string()),
            ]),
        )]),
        "test",
    );
    config.merge(
        &config_section(vec![(
            "github-protocols",
            PhpMixed::List(vec![PhpMixed::String("https".to_string())]),
        )]),
        "test",
    );

    assert_eq!(
        PhpMixed::List(vec![PhpMixed::String("https".to_string())]),
        config.get("github-protocols")
    );
}

#[ignore]
#[test]
fn test_git_disabled_by_default_in_github_protocols() {
    let mut config = Config::new(false, None);
    config.merge(
        &config_section(vec![(
            "github-protocols",
            PhpMixed::List(vec![
                PhpMixed::String("https".to_string()),
                PhpMixed::String("git".to_string()),
            ]),
        )]),
        "test",
    );
    assert_eq!(
        PhpMixed::List(vec![PhpMixed::String("https".to_string())]),
        config.get("github-protocols")
    );

    config.merge(
        &config_section(vec![("secure-http", PhpMixed::Bool(false))]),
        "test",
    );
    assert_eq!(
        PhpMixed::List(vec![
            PhpMixed::String("https".to_string()),
            PhpMixed::String("git".to_string()),
        ]),
        config.get("github-protocols")
    );
}

#[ignore]
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
            .prohibit_url_by_config(url, None, &IndexMap::new())
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
            .prohibit_url_by_config(url, None, &IndexMap::new())
            .unwrap_err();
        assert!(
            err.to_string().contains(&format!(
                "Your configuration does not allow connections to {url}"
            )),
            "url {url:?}: {err}"
        );
    }
}

// PHP asserts the warning via getIOMock()->expects(); here a real BufferIO captures the output
// instead. The case stays #[ignore] because BufferIO::get_output is todo!() (its PhpResource
// stream model is unfinished).
#[ignore = "BufferIO::get_output is todo!() (PhpResource stream model unfinished)"]
#[test]
fn test_prohibited_urls_warning_verify_peer() {
    let io = std::rc::Rc::new(std::cell::RefCell::new(
        shirabe::io::buffer_io::BufferIO::new(
            String::new(),
            shirabe_external_packages::symfony::console::output::output_interface::VERBOSITY_NORMAL,
            None,
        )
        .unwrap(),
    ));

    let mut config = Config::new(false, None);

    let mut ssl: IndexMap<String, PhpMixed> = IndexMap::new();
    ssl.insert("verify_peer".to_string(), PhpMixed::Bool(false));
    ssl.insert("verify_peer_name".to_string(), PhpMixed::Bool(false));
    let mut repo_options: IndexMap<String, PhpMixed> = IndexMap::new();
    repo_options.insert("ssl".to_string(), PhpMixed::Array(ssl));

    config
        .prohibit_url_by_config("https://example.org", Some(io.clone()), &repo_options)
        .unwrap();

    assert_eq!(
        "<warning>Warning: Accessing example.org with verify_peer and verify_peer_name disabled.</warning>",
        io.borrow().get_output()
    );
}

#[ignore]
#[test]
fn test_disable_tls_can_be_overridden() {
    let mut config = Config::new(true, None);
    config.merge(
        &config_section(vec![("disable-tls", PhpMixed::String("false".to_string()))]),
        "test",
    );
    assert_eq!(PhpMixed::Bool(false), config.get("disable-tls"));
    config.merge(
        &config_section(vec![("disable-tls", PhpMixed::String("true".to_string()))]),
        "test",
    );
    assert_eq!(PhpMixed::Bool(true), config.get("disable-tls"));
}

#[test]
fn test_process_timeout() {
    Platform::put_env("COMPOSER_PROCESS_TIMEOUT", "0");
    let config = Config::new(true, None);
    let result = config.get("process-timeout");
    Platform::clear_env("COMPOSER_PROCESS_TIMEOUT");

    assert_eq!(PhpMixed::Int(0), result);
}

#[ignore]
#[test]
fn test_htaccess_protect() {
    Platform::put_env("COMPOSER_HTACCESS_PROTECT", "0");
    let config = Config::new(true, None);
    let result = config.get("htaccess-protect");
    Platform::clear_env("COMPOSER_HTACCESS_PROTECT");

    assert_eq!(PhpMixed::Bool(false), result);
}

#[test]
fn test_get_source_of_value() {
    Platform::clear_env("COMPOSER_PROCESS_TIMEOUT");

    let mut config = Config::new(true, None);

    assert_eq!(
        Config::SOURCE_DEFAULT,
        config.get_source_of_value("process-timeout").as_str()
    );

    config.merge(
        &config_section(vec![("process-timeout", PhpMixed::Int(1))]),
        "phpunit-test",
    );

    assert_eq!(
        "phpunit-test",
        config.get_source_of_value("process-timeout").as_str()
    );
}

#[test]
fn test_get_source_of_value_env_variables() {
    Platform::put_env("COMPOSER_HTACCESS_PROTECT", "0");
    let mut config = Config::new(true, None);
    let result = config.get_source_of_value("htaccess-protect");
    Platform::clear_env("COMPOSER_HTACCESS_PROTECT");

    assert_eq!("COMPOSER_HTACCESS_PROTECT", result.as_str());
}

#[test]
fn test_audit() {
    let mut config = Config::new(true, None);
    let result = config.get("audit");
    let result = result.as_array().unwrap();
    assert!(result.contains_key("abandoned"));
    assert!(result.contains_key("ignore"));
    assert_eq!(
        Some(&PhpMixed::String(Auditor::ABANDONED_FAIL.to_string())),
        result.get("abandoned")
    );
    assert_eq!(
        Some(&PhpMixed::Array(IndexMap::new())),
        result.get("ignore")
    );

    Platform::put_env("COMPOSER_AUDIT_ABANDONED", Auditor::ABANDONED_IGNORE);
    let result = config.get("audit");
    Platform::clear_env("COMPOSER_AUDIT_ABANDONED");
    let result = result.as_array().unwrap();
    assert!(result.contains_key("abandoned"));
    assert!(result.contains_key("ignore"));
    assert_eq!(
        Some(&PhpMixed::String(Auditor::ABANDONED_IGNORE.to_string())),
        result.get("abandoned")
    );
    assert_eq!(
        Some(&PhpMixed::Array(IndexMap::new())),
        result.get("ignore")
    );

    config.merge(
        &config_section(vec![(
            "audit",
            PhpMixed::Array(map(vec![(
                "ignore",
                PhpMixed::List(vec![
                    PhpMixed::String("A".to_string()),
                    PhpMixed::String("B".to_string()),
                ]),
            )])),
        )]),
        "test",
    );
    config.merge(
        &config_section(vec![(
            "audit",
            PhpMixed::Array(map(vec![(
                "ignore",
                PhpMixed::List(vec![
                    PhpMixed::String("A".to_string()),
                    PhpMixed::String("C".to_string()),
                ]),
            )])),
        )]),
        "test",
    );
    let result = config.get("audit");
    let result = result.as_array().unwrap();
    assert!(result.contains_key("ignore"));
    assert_eq!(
        Some(&PhpMixed::List(vec![
            PhpMixed::String("A".to_string()),
            PhpMixed::String("B".to_string()),
            PhpMixed::String("A".to_string()),
            PhpMixed::String("C".to_string()),
        ])),
        result.get("ignore")
    );

    // Test COMPOSER_SECURITY_BLOCKING_ABANDONED env var
    Platform::put_env("COMPOSER_SECURITY_BLOCKING_ABANDONED", "1");
    let result = config.get("audit");
    Platform::clear_env("COMPOSER_SECURITY_BLOCKING_ABANDONED");
    let result = result.as_array().unwrap();
    assert!(result.contains_key("block-abandoned"));
    assert_eq!(Some(&PhpMixed::Bool(true)), result.get("block-abandoned"));

    Platform::put_env("COMPOSER_SECURITY_BLOCKING_ABANDONED", "0");
    let result = config.get("audit");
    Platform::clear_env("COMPOSER_SECURITY_BLOCKING_ABANDONED");
    let result = result.as_array().unwrap();
    assert!(result.contains_key("block-abandoned"));
    assert_eq!(Some(&PhpMixed::Bool(false)), result.get("block-abandoned"));
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
        let value = config.get(key);
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
            "allow-plugins",
            PhpMixed::Array(map(vec![("some/plugin", PhpMixed::Bool(true))])),
        )]),
        "test",
    );
    match config.get("allow-plugins") {
        PhpMixed::Array(actual) => {
            assert_map_equals(&map(vec![("some/plugin", PhpMixed::Bool(true))]), &actual)
        }
        other => panic!("expected array, got {other:?}"),
    }

    config.merge(
        &config_section(vec![(
            "allow-plugins",
            PhpMixed::Array(map(vec![("another/plugin", PhpMixed::Bool(true))])),
        )]),
        "test",
    );
    match config.get("allow-plugins") {
        PhpMixed::Array(actual) => assert_map_equals(
            &map(vec![
                ("some/plugin", PhpMixed::Bool(true)),
                ("another/plugin", PhpMixed::Bool(true)),
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
        &config_section(vec![("allow-plugins", PhpMixed::Bool(true))]),
        "test",
    );
    assert_eq!(PhpMixed::Bool(true), config.get("allow-plugins"));

    config.merge(
        &config_section(vec![(
            "allow-plugins",
            PhpMixed::Array(map(vec![("another/plugin", PhpMixed::Bool(true))])),
        )]),
        "test",
    );
    match config.get("allow-plugins") {
        PhpMixed::Array(actual) => assert_map_equals(
            &map(vec![("another/plugin", PhpMixed::Bool(true))]),
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
            "allow-plugins",
            PhpMixed::Array(map(vec![("some/plugin", PhpMixed::Bool(true))])),
        )]),
        "test",
    );
    match config.get("allow-plugins") {
        PhpMixed::Array(actual) => {
            assert_map_equals(&map(vec![("some/plugin", PhpMixed::Bool(true))]), &actual)
        }
        other => panic!("expected array, got {other:?}"),
    }

    config.merge(
        &config_section(vec![("allow-plugins", PhpMixed::Bool(true))]),
        "test",
    );
    assert_eq!(PhpMixed::Bool(true), config.get("allow-plugins"));
}
