//! ref: composer/tests/Composer/Test/Repository/ComposerRepositoryTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::RepositoryInterface;
use shirabe::repository::SEARCH_FULLTEXT;
use shirabe::repository::composer_repository::{ComposerRepository, ProviderListingEntry};
use shirabe::util::http_downloader::HttpDownloaderMockHandler;
use shirabe_php_shim::PhpMixed;
use shirabe_semver::constraint::{AnyConstraint, SimpleConstraint};
use tempfile::TempDir;

use crate::http_downloader_mock::{HttpDownloaderMockGuard, expect_full, get_http_downloader_mock};

// Mirrors PHP's `['url' => .., 'body' => ..]` mock entry (status defaults to 200,
// options match any executed options).
fn http_body(
    url: &str,
    body: impl Into<String>,
) -> shirabe::util::http_downloader::HttpDownloaderMockExpectation {
    expect_full(url, None, 200, body, vec![String::new()])
}

// Equivalent to FactoryMock::createConfig(): a real Config with a writable, unique
// home directory. The TempDir is returned so it outlives the test.
fn create_config() -> (Config, TempDir) {
    let home = TempDir::new().unwrap();
    let mut config = Config::new(true, None);
    let mut top: IndexMap<String, PhpMixed> = IndexMap::new();
    let mut config_section: IndexMap<String, PhpMixed> = IndexMap::new();
    config_section.insert(
        "home".to_string(),
        PhpMixed::String(home.path().to_string_lossy().into_owned()),
    );
    top.insert("config".to_string(), PhpMixed::Array(config_section));
    let mut repositories: IndexMap<String, PhpMixed> = IndexMap::new();
    repositories.insert("packagist".to_string(), PhpMixed::Bool(false));
    top.insert("repositories".to_string(), PhpMixed::Array(repositories));
    config.merge(&top, Config::SOURCE_UNKNOWN);
    (config, home)
}

fn create_config_read_only() -> (Config, TempDir) {
    let (mut config, home) = create_config();
    let mut top: IndexMap<String, PhpMixed> = IndexMap::new();
    let mut config_section: IndexMap<String, PhpMixed> = IndexMap::new();
    config_section.insert("cache-read-only".to_string(), PhpMixed::Bool(true));
    top.insert("config".to_string(), PhpMixed::Array(config_section));
    config.merge(&top, Config::SOURCE_UNKNOWN);
    (config, home)
}

fn null_io() -> Rc<RefCell<dyn IOInterface>> {
    Rc::new(RefCell::new(NullIO::new()))
}

fn repo_config(url: &str) -> IndexMap<String, PhpMixed> {
    let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
    repo_config.insert("url".to_string(), PhpMixed::String(url.to_string()));
    repo_config
}

fn json_encode(value: &PhpMixed) -> String {
    shirabe::json::json_file::JsonFile::encode(value)
}

fn str_kv(pairs: &[(&str, PhpMixed)]) -> PhpMixed {
    let mut m: IndexMap<String, PhpMixed> = IndexMap::new();
    for (k, v) in pairs {
        m.insert(k.to_string(), v.clone());
    }
    PhpMixed::Array(m)
}

// loadDataProvider cases: (expected name/version pairs, repoPackages body).
fn load_data_provider() -> Vec<(Vec<(&'static str, &'static str)>, PhpMixed)> {
    vec![
        // Old repository format
        (
            vec![("foo/bar", "1.0.0")],
            str_kv(&[(
                "foo/bar",
                str_kv(&[
                    ("name", PhpMixed::String("foo/bar".to_string())),
                    (
                        "versions",
                        str_kv(&[(
                            "1.0.0",
                            str_kv(&[
                                ("name", PhpMixed::String("foo/bar".to_string())),
                                ("version", PhpMixed::String("1.0.0".to_string())),
                            ]),
                        )]),
                    ),
                ]),
            )]),
        ),
        // New repository format
        (
            vec![("bar/foo", "3.14"), ("bar/foo", "3.145")],
            str_kv(&[(
                "packages",
                str_kv(&[(
                    "bar/foo",
                    str_kv(&[
                        (
                            "3.14",
                            str_kv(&[
                                ("name", PhpMixed::String("bar/foo".to_string())),
                                ("version", PhpMixed::String("3.14".to_string())),
                            ]),
                        ),
                        (
                            "3.145",
                            str_kv(&[
                                ("name", PhpMixed::String("bar/foo".to_string())),
                                ("version", PhpMixed::String("3.145".to_string())),
                            ]),
                        ),
                    ]),
                )]),
            )]),
        ),
        // New repository format but without versions as keys should also be supported
        (
            vec![("bar/foo", "3.14"), ("bar/foo", "3.145")],
            str_kv(&[(
                "packages",
                str_kv(&[(
                    "bar/foo",
                    PhpMixed::List(vec![
                        str_kv(&[
                            ("name", PhpMixed::String("bar/foo".to_string())),
                            ("version", PhpMixed::String("3.14".to_string())),
                        ]),
                        str_kv(&[
                            ("name", PhpMixed::String("bar/foo".to_string())),
                            ("version", PhpMixed::String("3.145".to_string())),
                        ]),
                    ]),
                )]),
            )]),
        ),
    ]
}

#[test]
fn test_load_data() {
    for (expected, repo_packages) in load_data_provider() {
        let (config, _home) = create_config();

        let (http_downloader, _http_guard): (_, HttpDownloaderMockGuard) = get_http_downloader_mock(
            vec![http_body(
                "http://example.org/packages.json",
                json_encode(&repo_packages),
            )],
            true,
            HttpDownloaderMockHandler::default(),
        );

        let mut repository = ComposerRepository::new(
            repo_config("http://example.org"),
            null_io(),
            &config,
            http_downloader,
            None,
        )
        .unwrap();

        let packages = repository.get_packages().unwrap();

        assert_eq!(expected.len(), packages.len());
        for (index, (name, version)) in expected.iter().enumerate() {
            assert_eq!(
                format!("{} {}", name, version),
                format!(
                    "{} {}",
                    packages[index].get_name(),
                    packages[index].get_pretty_version()
                )
            );
        }
    }
}

// Ported and exercising the real whatProvides path, but blocked by an unimplemented
// production method: building the dev-* alias packages reaches AliasPackage::get_source_type
// (todo!() in package/alias_package.rs).
#[ignore = "production todo!(): AliasPackage::get_source_type unimplemented (reached when building dev-* branch aliases)"]
#[test]
fn test_what_provides() {
    let (config, _home) = create_config();

    // The fetchFile response that PHP stubs via a method mock; here we serve it over
    // HTTP and let the real fetchFile path parse it. fetchFile verifies the body's
    // sha256 against the providerListing hash, so the listing hash is the actual hash
    // of this body.
    let body = json_encode(&str_kv(&[(
        "packages",
        PhpMixed::List(vec![
            PhpMixed::List(vec![str_kv(&[
                ("uid", PhpMixed::Int(1)),
                ("name", PhpMixed::String("a".to_string())),
                ("version", PhpMixed::String("dev-master".to_string())),
                (
                    "extra",
                    str_kv(&[(
                        "branch-alias",
                        str_kv(&[("dev-master", PhpMixed::String("1.0.x-dev".to_string()))]),
                    )]),
                ),
            ])]),
            PhpMixed::List(vec![str_kv(&[
                ("uid", PhpMixed::Int(2)),
                ("name", PhpMixed::String("a".to_string())),
                ("version", PhpMixed::String("dev-develop".to_string())),
                (
                    "extra",
                    str_kv(&[(
                        "branch-alias",
                        str_kv(&[("dev-develop", PhpMixed::String("1.1.x-dev".to_string()))]),
                    )]),
                ),
            ])]),
            PhpMixed::List(vec![str_kv(&[
                ("uid", PhpMixed::Int(3)),
                ("name", PhpMixed::String("a".to_string())),
                ("version", PhpMixed::String("0.6".to_string())),
            ])]),
        ]),
    )]));

    let sha256 = shirabe_php_shim::hash("sha256", &body);

    let (http_downloader, _http_guard): (_, HttpDownloaderMockGuard) = get_http_downloader_mock(
        vec![http_body("https://dummy.test.link/to/a/file", body)],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let repo = ComposerRepository::new(
        repo_config("https://dummy.test.link"),
        null_io(),
        &config,
        http_downloader,
        None,
    )
    .unwrap();

    let rc = Rc::new(RefCell::new(repo));
    let rc_dyn: Rc<RefCell<dyn RepositoryInterface>> = rc.clone();
    rc.borrow().set_self_handle(Rc::downgrade(&rc_dyn));

    let mut provider_listing: IndexMap<String, ProviderListingEntry> = IndexMap::new();
    provider_listing.insert("a".to_string(), ProviderListingEntry { sha256 });
    rc.borrow_mut().__set_provider_listing(provider_listing);
    rc.borrow_mut()
        .__set_providers_url("https://dummy.test.link/to/%package%/file");

    let packages = rc.borrow_mut().__what_provides("a").unwrap();

    assert_eq!(5, packages.len());
    assert_eq!(
        vec!["1", "1-alias", "2", "2-alias", "3"],
        packages.keys().cloned().collect::<Vec<_>>()
    );
    let alias = packages["2-alias"].as_alias().unwrap();
    let aliased: shirabe::package::handle::PackageInterfaceHandle = alias.get_alias_of().into();
    assert!(packages["2"].ptr_eq(&aliased));
}

#[test]
fn test_search_with_type() {
    let (config, _home) = create_config_read_only();

    let result = str_kv(&[(
        "results",
        PhpMixed::List(vec![str_kv(&[
            ("name", PhpMixed::String("foo".to_string())),
            ("description", PhpMixed::Null),
        ])]),
    )]);

    let (http_downloader, _http_guard): (_, HttpDownloaderMockGuard) = get_http_downloader_mock(
        vec![
            http_body(
                "http://example.org/packages.json",
                json_encode(&str_kv(&[(
                    "search",
                    PhpMixed::String("/search.json?q=%query%&type=%type%".to_string()),
                )])),
            ),
            http_body(
                "http://example.org/search.json?q=foo&type=composer-plugin",
                json_encode(&result),
            ),
            http_body(
                "http://example.org/search.json?q=foo&type=library",
                json_encode(&PhpMixed::List(vec![])),
            ),
        ],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let mut repository = ComposerRepository::new(
        repo_config("http://example.org"),
        null_io(),
        &config,
        http_downloader,
        None,
    )
    .unwrap();

    let plugin_results = repository
        .search(
            "foo".to_string(),
            SEARCH_FULLTEXT,
            Some("composer-plugin".to_string()),
        )
        .unwrap();
    assert_eq!(1, plugin_results.len());
    assert_eq!(
        Some("foo"),
        plugin_results[0].get("name").and_then(|v| v.as_string())
    );
    assert!(matches!(
        plugin_results[0].get("description"),
        Some(PhpMixed::Null)
    ));

    let library_results = repository
        .search(
            "foo".to_string(),
            SEARCH_FULLTEXT,
            Some("library".to_string()),
        )
        .unwrap();
    assert!(library_results.is_empty());
}

#[test]
fn test_search_with_special_chars() {
    let (config, _home) = create_config_read_only();

    let (http_downloader, _http_guard): (_, HttpDownloaderMockGuard) = get_http_downloader_mock(
        vec![
            http_body(
                "http://example.org/packages.json",
                json_encode(&str_kv(&[(
                    "search",
                    PhpMixed::String("/search.json?q=%query%&type=%type%".to_string()),
                )])),
            ),
            http_body(
                "http://example.org/search.json?q=foo+bar&type=",
                json_encode(&PhpMixed::List(vec![])),
            ),
        ],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let mut repository = ComposerRepository::new(
        repo_config("http://example.org"),
        null_io(),
        &config,
        http_downloader,
        None,
    )
    .unwrap();

    let results = repository
        .search("foo bar".to_string(), SEARCH_FULLTEXT, None)
        .unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_search_with_abandoned_packages() {
    let (config, _home) = create_config_read_only();

    let result = str_kv(&[(
        "results",
        PhpMixed::List(vec![
            str_kv(&[
                ("name", PhpMixed::String("foo1".to_string())),
                ("description", PhpMixed::Null),
                ("abandoned", PhpMixed::Bool(true)),
            ]),
            str_kv(&[
                ("name", PhpMixed::String("foo2".to_string())),
                ("description", PhpMixed::Null),
                ("abandoned", PhpMixed::String("bar".to_string())),
            ]),
        ]),
    )]);

    let (http_downloader, _http_guard): (_, HttpDownloaderMockGuard) = get_http_downloader_mock(
        vec![
            http_body(
                "http://example.org/packages.json",
                json_encode(&str_kv(&[(
                    "search",
                    PhpMixed::String("/search.json?q=%query%".to_string()),
                )])),
            ),
            http_body("http://example.org/search.json?q=foo", json_encode(&result)),
        ],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let mut repository = ComposerRepository::new(
        repo_config("http://example.org"),
        null_io(),
        &config,
        http_downloader,
        None,
    )
    .unwrap();

    let results = repository
        .search("foo".to_string(), SEARCH_FULLTEXT, None)
        .unwrap();

    assert_eq!(2, results.len());
    assert_eq!(
        Some("foo1"),
        results[0].get("name").and_then(|v| v.as_string())
    );
    assert!(matches!(
        results[0].get("description"),
        Some(PhpMixed::Null)
    ));
    assert!(matches!(
        results[0].get("abandoned"),
        Some(PhpMixed::Bool(true))
    ));
    assert_eq!(
        Some("foo2"),
        results[1].get("name").and_then(|v| v.as_string())
    );
    assert!(matches!(
        results[1].get("description"),
        Some(PhpMixed::Null)
    ));
    assert_eq!(
        Some("bar"),
        results[1].get("abandoned").and_then(|v| v.as_string())
    );
}

fn canonicalize_url_test_cases() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        (
            "https://example.org/path/to/file",
            "/path/to/file",
            "https://example.org",
        ),
        (
            "https://example.org/canonic_url",
            "https://example.org/canonic_url",
            "https://should-not-see-me.test",
        ),
        (
            "file:///path/to/repository/file",
            "/path/to/repository/file",
            "file:///path/to/repository",
        ),
        // Repository URL returned unchanged if it is not a URL (BC test).
        ("invalid_repo_url", "/path/to/file", "invalid_repo_url"),
        // URLs may contain sequences resembling preg_replace() pattern references
        // without messing up the result (regression test).
        (
            "https://example.org/path/to/unusual_$0_filename",
            "/path/to/unusual_$0_filename",
            "https://example.org",
        ),
    ]
}

#[test]
fn test_canonicalize_url() {
    for (expected, url, repository_url) in canonicalize_url_test_cases() {
        let (config, _home) = create_config();

        let (http_downloader, _http_guard): (_, HttpDownloaderMockGuard) =
            get_http_downloader_mock(vec![], false, HttpDownloaderMockHandler::default());

        let mut repository = ComposerRepository::new(
            repo_config(repository_url),
            null_io(),
            &config,
            http_downloader,
            None,
        )
        .unwrap();

        // __construct ensures the repository URL has a protocol, so reset it here in
        // order to test all cases.
        repository.__set_url(repository_url);

        assert_eq!(expected, repository.__canonicalize_url(url).unwrap());
    }
}

#[test]
fn test_get_provider_names_will_return_partial_package_names() {
    let (config, _home) = create_config();

    let (http_downloader, _http_guard): (_, HttpDownloaderMockGuard) = get_http_downloader_mock(
        vec![http_body(
            "http://example.org/packages.json",
            json_encode(&str_kv(&[
                (
                    "providers-lazy-url",
                    PhpMixed::String("/foo/p/%package%.json".to_string()),
                ),
                (
                    "packages",
                    str_kv(&[(
                        "foo/bar",
                        str_kv(&[
                            (
                                "dev-branch",
                                str_kv(&[("name", PhpMixed::String("foo/bar".to_string()))]),
                            ),
                            (
                                "v1.0.0",
                                str_kv(&[("name", PhpMixed::String("foo/bar".to_string()))]),
                            ),
                        ]),
                    )]),
                ),
            ])),
        )],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let mut repository = ComposerRepository::new(
        repo_config("http://example.org/packages.json"),
        null_io(),
        &config,
        http_downloader,
        None,
    )
    .unwrap();

    assert_eq!(vec!["foo/bar"], repository.get_package_names(None).unwrap());
}

#[test]
fn test_get_security_advisories_assert_repository_http_options_are_used() {
    let (config, _home) = create_config();

    let (http_downloader, _http_guard): (_, HttpDownloaderMockGuard) = get_http_downloader_mock(
        vec![
            http_body(
                "https://example.org/packages.json",
                json_encode(&str_kv(&[
                    (
                        "packages",
                        str_kv(&[(
                            "foo/bar",
                            str_kv(&[
                                (
                                    "dev-branch",
                                    str_kv(&[("name", PhpMixed::String("foo/bar".to_string()))]),
                                ),
                                (
                                    "v1.0.0",
                                    str_kv(&[("name", PhpMixed::String("foo/bar".to_string()))]),
                                ),
                            ]),
                        )]),
                    ),
                    (
                        "metadata-url",
                        PhpMixed::String("https://example.org/p2/%package%.json".to_string()),
                    ),
                    (
                        "security-advisories",
                        str_kv(&[(
                            "api-url",
                            PhpMixed::String("https://example.org/security-advisories".to_string()),
                        )]),
                    ),
                ])),
            ),
            http_body(
                "https://example.org/security-advisories",
                json_encode(&str_kv(&[("advisories", PhpMixed::List(vec![]))])),
            ),
        ],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
    repo_config.insert(
        "url".to_string(),
        PhpMixed::String("https://example.org/packages.json".to_string()),
    );
    repo_config.insert(
        "options".to_string(),
        str_kv(&[("http", str_kv(&[("verify_peer", PhpMixed::Bool(false))]))]),
    );

    let mut repository =
        ComposerRepository::new(repo_config, null_io(), &config, http_downloader, None).unwrap();

    let constraint: AnyConstraint =
        SimpleConstraint::new("=".to_string(), "1.0.0.0".to_string(), None).into();
    let mut map: IndexMap<String, AnyConstraint> = IndexMap::new();
    map.insert("foo/bar".to_string(), constraint);

    let result = repository.get_security_advisories(map, false).unwrap();
    assert!(result.names_found.is_empty());
    assert!(result.advisories.is_empty());
}

fn generate_security_advisory(
    package_name: &str,
    cve: Option<&str>,
    affected_versions: &str,
) -> (String, PhpMixed) {
    let advisory_id = shirabe_php_shim::uniqid("PKSA-", false);
    let advisory = str_kv(&[
        ("advisoryId", PhpMixed::String(advisory_id.clone())),
        ("packageName", PhpMixed::String(package_name.to_string())),
        ("remoteId", PhpMixed::String("test".to_string())),
        ("title", PhpMixed::String("Security Advisory".to_string())),
        ("link", PhpMixed::Null),
        (
            "cve",
            match cve {
                Some(c) => PhpMixed::String(c.to_string()),
                None => PhpMixed::Null,
            },
        ),
        (
            "affectedVersions",
            PhpMixed::String(affected_versions.to_string()),
        ),
        ("source", PhpMixed::String("Tests".to_string())),
        (
            "reportedAt",
            PhpMixed::String("2024-04-31 12:37:47".to_string()),
        ),
        (
            "composerRepository",
            PhpMixed::String("Package Repository".to_string()),
        ),
        ("severity", PhpMixed::String("high".to_string())),
        (
            "sources",
            PhpMixed::List(vec![str_kv(&[
                ("name", PhpMixed::String("Security Advisory".to_string())),
                ("remoteId", PhpMixed::String("test".to_string())),
            ])]),
        ),
    ]);
    (advisory_id, advisory)
}

// Ported and exercising the real getSecurityAdvisories path, but blocked by an unimplemented
// production shim: constructing a full SecurityAdvisory parses `reportedAt` via
// shirabe_php_shim::date_create (todo!(): needs the strtotime grammar parser).
#[ignore = "production todo!(): shirabe_php_shim::date_create unimplemented (reached when parsing advisory reportedAt into a full SecurityAdvisory)"]
#[test]
fn test_get_security_advisories_assert_repository_advisories_is_zero_indexed_array_with_consecutive_keys()
 {
    let (config, _home) = create_config();

    let package_name = "foo/bar";
    let (advisory1_id, advisory1) =
        generate_security_advisory(package_name, Some("CVE-1999-1000"), ">=1.0.0,<1.1.0");
    let (_advisory2_id, advisory2) =
        generate_security_advisory(package_name, Some("CVE-1999-1000"), ">=2.0.0");
    let (advisory3_id, advisory3) =
        generate_security_advisory(package_name, Some("CVE-1999-1000"), ">=1.0.0,<1.1.0");

    let expected_advisory_ids = [advisory1_id, advisory3_id];

    let (http_downloader, _http_guard): (_, HttpDownloaderMockGuard) = get_http_downloader_mock(
        vec![
            http_body(
                "https://example.org/packages.json",
                json_encode(&str_kv(&[
                    (
                        "packages",
                        str_kv(&[(
                            package_name,
                            str_kv(&[
                                (
                                    "dev-branch",
                                    str_kv(&[("name", PhpMixed::String(package_name.to_string()))]),
                                ),
                                (
                                    "v1.0.0",
                                    str_kv(&[("name", PhpMixed::String(package_name.to_string()))]),
                                ),
                            ]),
                        )]),
                    ),
                    (
                        "metadata-url",
                        PhpMixed::String("https://example.org/p2/%package%.json".to_string()),
                    ),
                    (
                        "security-advisories",
                        str_kv(&[(
                            "api-url",
                            PhpMixed::String("https://example.org/security-advisories".to_string()),
                        )]),
                    ),
                ])),
            ),
            http_body(
                "https://example.org/security-advisories",
                json_encode(&str_kv(&[(
                    "advisories",
                    str_kv(&[(
                        package_name,
                        PhpMixed::List(vec![advisory1, advisory2, advisory3]),
                    )]),
                )])),
            ),
        ],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
    repo_config.insert(
        "url".to_string(),
        PhpMixed::String("https://example.org/packages.json".to_string()),
    );
    repo_config.insert(
        "options".to_string(),
        str_kv(&[("http", str_kv(&[("verify_peer", PhpMixed::Bool(false))]))]),
    );

    let mut repository =
        ComposerRepository::new(repo_config, null_io(), &config, http_downloader, None).unwrap();

    let constraint: AnyConstraint =
        SimpleConstraint::new("=".to_string(), "1.0.0.0".to_string(), None).into();
    let mut map: IndexMap<String, AnyConstraint> = IndexMap::new();
    map.insert(package_name.to_string(), constraint);

    let result = repository.get_security_advisories(map, false).unwrap();

    let actual = result.advisories.get(package_name).unwrap();
    assert_eq!(expected_advisory_ids.len(), actual.len());
    for (i, expected_id) in expected_advisory_ids.iter().enumerate() {
        assert_eq!(expected_id, actual[i].advisory_id());
    }
}
