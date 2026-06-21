//! ref: composer/tests/Composer/Test/ConfigTest.php

use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe_php_shim::PhpMixed;

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
#[ignore = "not yet ported (env-dependent without the setUp/tearDown isolation, or plugin-config merge details)"]
fn test_preferred_install_as_string() {
    todo!()
}

#[test]
#[ignore = "not yet ported (env-dependent without the setUp/tearDown isolation, or plugin-config merge details)"]
fn test_merge_preferred_install() {
    todo!()
}

#[test]
#[ignore = "not yet ported (env-dependent without the setUp/tearDown isolation, or plugin-config merge details)"]
fn test_merge_github_oauth() {
    todo!()
}

#[test]
#[ignore = "not yet ported (env-dependent without the setUp/tearDown isolation, or plugin-config merge details)"]
fn test_var_replacement() {
    todo!()
}

#[test]
#[ignore = "not yet ported (env-dependent without the setUp/tearDown isolation, or plugin-config merge details)"]
fn test_realpath_replacement() {
    todo!()
}

#[test]
#[ignore = "not yet ported (env-dependent without the setUp/tearDown isolation, or plugin-config merge details)"]
fn test_stream_wrapper_dirs() {
    todo!()
}

#[test]
#[ignore = "not yet ported (env-dependent without the setUp/tearDown isolation, or plugin-config merge details)"]
fn test_fetching_relative_paths() {
    todo!()
}

#[test]
#[ignore = "not yet ported (env-dependent without the setUp/tearDown isolation, or plugin-config merge details)"]
fn test_override_github_protocols() {
    todo!()
}

#[test]
#[ignore = "not yet ported (env-dependent without the setUp/tearDown isolation, or plugin-config merge details)"]
fn test_git_disabled_by_default_in_github_protocols() {
    todo!()
}

#[test]
#[ignore = "not yet ported (env-dependent without the setUp/tearDown isolation, or plugin-config merge details)"]
fn test_allowed_urls_pass() {
    todo!()
}

#[test]
#[ignore = "not yet ported (env-dependent without the setUp/tearDown isolation, or plugin-config merge details)"]
fn test_prohibited_urls_throw_exception() {
    todo!()
}

#[test]
#[ignore = "not yet ported (env-dependent without the setUp/tearDown isolation, or plugin-config merge details)"]
fn test_prohibited_urls_warning_verify_peer() {
    todo!()
}

#[test]
#[ignore = "not yet ported (env-dependent without the setUp/tearDown isolation, or plugin-config merge details)"]
fn test_disable_tls_can_be_overridden() {
    todo!()
}

#[test]
#[ignore = "not yet ported (env-dependent without the setUp/tearDown isolation, or plugin-config merge details)"]
fn test_process_timeout() {
    todo!()
}

#[test]
#[ignore = "not yet ported (env-dependent without the setUp/tearDown isolation, or plugin-config merge details)"]
fn test_htaccess_protect() {
    todo!()
}

#[test]
#[ignore = "not yet ported (env-dependent without the setUp/tearDown isolation, or plugin-config merge details)"]
fn test_get_source_of_value() {
    todo!()
}

#[test]
#[ignore = "not yet ported (env-dependent without the setUp/tearDown isolation, or plugin-config merge details)"]
fn test_get_source_of_value_env_variables() {
    todo!()
}

#[test]
#[ignore = "not yet ported (env-dependent without the setUp/tearDown isolation, or plugin-config merge details)"]
fn test_audit() {
    todo!()
}

#[test]
#[ignore = "not yet ported (env-dependent without the setUp/tearDown isolation, or plugin-config merge details)"]
fn test_get_defaults_to_an_empty_array() {
    todo!()
}

#[test]
#[ignore = "not yet ported (env-dependent without the setUp/tearDown isolation, or plugin-config merge details)"]
fn test_merges_plugin_config() {
    todo!()
}

#[test]
#[ignore = "not yet ported (env-dependent without the setUp/tearDown isolation, or plugin-config merge details)"]
fn test_overrides_global_boolean_plugins_config() {
    todo!()
}

#[test]
#[ignore = "not yet ported (env-dependent without the setUp/tearDown isolation, or plugin-config merge details)"]
fn test_allows_all_plugins_from_local_boolean() {
    todo!()
}
