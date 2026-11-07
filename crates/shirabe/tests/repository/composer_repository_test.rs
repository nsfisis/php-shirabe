//! ref: composer/tests/Composer/Test/Repository/ComposerRepositoryTest.php sha1:66247b2293fb3020c292dcde7b6341f74371e50c

use crate::http_downloader_mock::{HttpDownloaderMockGuard, expect_full, get_http_downloader_mock};
use crate::test_case::get_unique_tmp_directory;
use bstr::{B, BStr, BString};
use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::{IOInterface, NullIO};
use shirabe::repository::{
    ComposerRepository, ProviderListingEntry, RepositoryInterface, SEARCH_FULLTEXT,
};
use shirabe::util::HttpDownloaderMockHandler;
use shirabe_php_shim::{PhpMixed, ToPhpString, bformat, hash, path_to_bytes, uniqid};
use shirabe_semver::constraint::{AnyConstraint, SimpleConstraint};
use tempfile::TempDir;

// Mirrors PHP's `['url' => .., 'body' => ..]` mock entry (status defaults to 200,
// options match any executed options).
fn http_body(
    url: &BStr,
    body: impl Into<BString>,
) -> shirabe::util::http_downloader::HttpDownloaderMockExpectation {
    expect_full(url, None, 200, body, vec![BString::default()])
}

// Equivalent to FactoryMock::createConfig(): a real Config with a writable, unique
// home directory.
fn create_config() -> (Config, TempDir) {
    let home = get_unique_tmp_directory();
    let mut config = Config::new(true, None);
    let mut top: IndexMap<BString, PhpMixed> = IndexMap::new();
    let mut config_section: IndexMap<BString, PhpMixed> = IndexMap::new();
    config_section.insert("home".into(), PhpMixed::string(path_to_bytes(&home.path())));
    top.insert("config".into(), PhpMixed::Array(config_section));
    let mut repositories: IndexMap<BString, PhpMixed> = IndexMap::new();
    repositories.insert("packagist".into(), PhpMixed::Bool(false));
    top.insert("repositories".into(), PhpMixed::Array(repositories));
    config.merge(&top, Config::SOURCE_UNKNOWN);
    (config, home)
}

fn create_config_read_only() -> (Config, TempDir) {
    let (mut config, home) = create_config();
    let mut top: IndexMap<BString, PhpMixed> = IndexMap::new();
    let mut config_section: IndexMap<BString, PhpMixed> = IndexMap::new();
    config_section.insert("cache-read-only".into(), PhpMixed::Bool(true));
    top.insert("config".into(), PhpMixed::Array(config_section));
    config.merge(&top, Config::SOURCE_UNKNOWN);
    (config, home)
}

fn null_io() -> std::rc::Rc<std::cell::RefCell<dyn IOInterface>> {
    std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()))
}

fn repo_config(url: &BStr) -> IndexMap<BString, PhpMixed> {
    let mut repo_config: IndexMap<BString, PhpMixed> = IndexMap::new();
    repo_config.insert("url".into(), PhpMixed::string(url.to_php_string()));
    repo_config
}

fn json_encode(value: &PhpMixed) -> BString {
    shirabe::json::json_file::JsonFile::encode(value)
        .unwrap()
        .to_owned()
}

fn str_kv(pairs: &[(&BStr, PhpMixed)]) -> PhpMixed {
    let mut m: IndexMap<BString, PhpMixed> = IndexMap::new();
    for (k, v) in pairs {
        m.insert(k.to_php_string(), v.clone());
    }
    PhpMixed::Array(m)
}

// loadDataProvider cases: (expected name/version pairs, repoPackages body).
fn load_data_provider() -> Vec<(Vec<(&'static BStr, &'static BStr)>, PhpMixed)> {
    vec![
        // Old repository format
        (
            vec![(BStr::new("foo/bar"), BStr::new("1.0.0"))],
            str_kv(&[(
                BStr::new("foo/bar"),
                str_kv(&[
                    (
                        BStr::new("name"),
                        PhpMixed::string(BString::from("foo/bar")),
                    ),
                    (
                        BStr::new("versions"),
                        str_kv(&[(
                            BStr::new("1.0.0"),
                            str_kv(&[
                                (
                                    BStr::new("name"),
                                    PhpMixed::string(BString::from("foo/bar")),
                                ),
                                (
                                    BStr::new("version"),
                                    PhpMixed::string(BString::from("1.0.0")),
                                ),
                            ]),
                        )]),
                    ),
                ]),
            )]),
        ),
        // New repository format
        (
            vec![
                (BStr::new("bar/foo"), BStr::new("3.14")),
                (BStr::new("bar/foo"), BStr::new("3.145")),
            ],
            str_kv(&[(
                BStr::new("packages"),
                str_kv(&[(
                    BStr::new("bar/foo"),
                    str_kv(&[
                        (
                            BStr::new("3.14"),
                            str_kv(&[
                                (
                                    BStr::new("name"),
                                    PhpMixed::string(BString::from("bar/foo")),
                                ),
                                (
                                    BStr::new("version"),
                                    PhpMixed::string(BString::from("3.14")),
                                ),
                            ]),
                        ),
                        (
                            BStr::new("3.145"),
                            str_kv(&[
                                (
                                    BStr::new("name"),
                                    PhpMixed::string(BString::from("bar/foo")),
                                ),
                                (
                                    BStr::new("version"),
                                    PhpMixed::string(BString::from("3.145")),
                                ),
                            ]),
                        ),
                    ]),
                )]),
            )]),
        ),
        // New repository format but without versions as keys should also be supported
        (
            vec![
                (BStr::new("bar/foo"), BStr::new("3.14")),
                (BStr::new("bar/foo"), BStr::new("3.145")),
            ],
            str_kv(&[(
                BStr::new("packages"),
                str_kv(&[(
                    BStr::new("bar/foo"),
                    PhpMixed::List(vec![
                        str_kv(&[
                            (
                                BStr::new("name"),
                                PhpMixed::string(BString::from("bar/foo")),
                            ),
                            (
                                BStr::new("version"),
                                PhpMixed::string(BString::from("3.14")),
                            ),
                        ]),
                        str_kv(&[
                            (
                                BStr::new("name"),
                                PhpMixed::string(BString::from("bar/foo")),
                            ),
                            (
                                BStr::new("version"),
                                PhpMixed::string(BString::from("3.145")),
                            ),
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
                BStr::new("http://example.org/packages.json"),
                json_encode(&repo_packages),
            )],
            true,
            HttpDownloaderMockHandler::default(),
        );

        let mut repository = ComposerRepository::new(
            repo_config(BStr::new("http://example.org")),
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
                bformat!("{name} {version}"),
                bformat!(
                    "{} {}",
                    packages[index].get_name(),
                    packages[index].get_pretty_version()
                )
            );
        }
    }
}

#[test]
fn test_what_provides() {
    let (config, _home) = create_config();

    // The fetchFile response that PHP stubs via a method mock; here we serve it over
    // HTTP and let the real fetchFile path parse it. fetchFile verifies the body's
    // sha256 against the providerListing hash, so the listing hash is the actual hash
    // of this body.
    let body = json_encode(&str_kv(&[(
        BStr::new("packages"),
        PhpMixed::List(vec![
            PhpMixed::List(vec![str_kv(&[
                (BStr::new("uid"), PhpMixed::Int(1)),
                (BStr::new("name"), PhpMixed::string(BString::from("a"))),
                (
                    BStr::new("version"),
                    PhpMixed::string(BString::from("dev-master")),
                ),
                (
                    BStr::new("extra"),
                    str_kv(&[(
                        BStr::new("branch-alias"),
                        str_kv(&[(
                            BStr::new("dev-master"),
                            PhpMixed::string(BString::from("1.0.x-dev")),
                        )]),
                    )]),
                ),
            ])]),
            PhpMixed::List(vec![str_kv(&[
                (BStr::new("uid"), PhpMixed::Int(2)),
                (BStr::new("name"), PhpMixed::string(BString::from("a"))),
                (
                    BStr::new("version"),
                    PhpMixed::string(BString::from("dev-develop")),
                ),
                (
                    BStr::new("extra"),
                    str_kv(&[(
                        BStr::new("branch-alias"),
                        str_kv(&[(
                            BStr::new("dev-develop"),
                            PhpMixed::string(BString::from("1.1.x-dev")),
                        )]),
                    )]),
                ),
            ])]),
            PhpMixed::List(vec![str_kv(&[
                (BStr::new("uid"), PhpMixed::Int(3)),
                (BStr::new("name"), PhpMixed::string(BString::from("a"))),
                (BStr::new("version"), PhpMixed::string(BString::from("0.6"))),
            ])]),
        ]),
    )]));

    let sha256 = hash("sha256", &body);

    let (http_downloader, _http_guard): (_, HttpDownloaderMockGuard) = get_http_downloader_mock(
        vec![http_body(
            BStr::new("https://dummy.test.link/to/a/file"),
            body,
        )],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let repo = ComposerRepository::new(
        repo_config(BStr::new("https://dummy.test.link")),
        null_io(),
        &config,
        http_downloader,
        None,
    )
    .unwrap();

    let rc = std::rc::Rc::new(std::cell::RefCell::new(repo));
    let rc_dyn: std::rc::Rc<std::cell::RefCell<dyn RepositoryInterface>> = rc.clone();
    rc.borrow().set_self_handle(std::rc::Rc::downgrade(&rc_dyn));

    let mut provider_listing: IndexMap<BString, ProviderListingEntry> = IndexMap::new();
    provider_listing.insert("a".into(), ProviderListingEntry { sha256 });
    rc.borrow_mut().__set_provider_listing(provider_listing);
    rc.borrow_mut()
        .__set_providers_url("https://dummy.test.link/to/%package%/file");

    let packages = rc.borrow_mut().__what_provides("a".into()).unwrap();

    assert_eq!(5, packages.len());
    assert_eq!(
        vec!["1", "1-alias", "2", "2-alias", "3"],
        packages.keys().cloned().collect::<Vec<_>>()
    );
    let alias = packages[B("2-alias")].as_alias().unwrap();
    let aliased: shirabe::package::handle::PackageInterfaceHandle = alias.get_alias_of().into();
    assert!(packages[B("2")].ptr_eq(&aliased));
}

#[test]
fn test_search_with_type() {
    let (config, _home) = create_config_read_only();

    let result = str_kv(&[(
        BStr::new("results"),
        PhpMixed::List(vec![str_kv(&[
            (BStr::new("name"), PhpMixed::string(BString::from("foo"))),
            (BStr::new("description"), PhpMixed::Null),
        ])]),
    )]);

    let (http_downloader, _http_guard): (_, HttpDownloaderMockGuard) = get_http_downloader_mock(
        vec![
            http_body(
                BStr::new("http://example.org/packages.json"),
                json_encode(&str_kv(&[(
                    BStr::new("search"),
                    PhpMixed::string(BString::from("/search.json?q=%query%&type=%type%")),
                )])),
            ),
            http_body(
                BStr::new("http://example.org/search.json?q=foo&type=composer-plugin"),
                json_encode(&result),
            ),
            http_body(
                BStr::new("http://example.org/search.json?q=foo&type=library"),
                json_encode(&PhpMixed::List(vec![])),
            ),
        ],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let mut repository = ComposerRepository::new(
        repo_config(BStr::new("http://example.org")),
        null_io(),
        &config,
        http_downloader,
        None,
    )
    .unwrap();

    let plugin_results = repository
        .search(
            "foo".into(),
            SEARCH_FULLTEXT,
            Some("composer-plugin".into()),
        )
        .unwrap();
    assert_eq!(1, plugin_results.len());
    assert_eq!(
        Some("foo".into()),
        plugin_results[0].get(B("name")).and_then(|v| v.as_string())
    );
    assert!(matches!(
        plugin_results[0].get(B("description")),
        Some(PhpMixed::Null)
    ));

    let library_results = repository
        .search("foo".into(), SEARCH_FULLTEXT, Some("library".into()))
        .unwrap();
    assert!(library_results.is_empty());
}

#[test]
fn test_search_with_special_chars() {
    let (config, _home) = create_config_read_only();

    let (http_downloader, _http_guard): (_, HttpDownloaderMockGuard) = get_http_downloader_mock(
        vec![
            http_body(
                BStr::new("http://example.org/packages.json"),
                json_encode(&str_kv(&[(
                    BStr::new("search"),
                    PhpMixed::string(BString::from("/search.json?q=%query%&type=%type%")),
                )])),
            ),
            http_body(
                BStr::new("http://example.org/search.json?q=foo+bar&type="),
                json_encode(&PhpMixed::List(vec![])),
            ),
        ],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let mut repository = ComposerRepository::new(
        repo_config(BStr::new("http://example.org")),
        null_io(),
        &config,
        http_downloader,
        None,
    )
    .unwrap();

    let results = repository
        .search("foo bar".into(), SEARCH_FULLTEXT, None)
        .unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_search_with_abandoned_packages() {
    let (config, _home) = create_config_read_only();

    let result = str_kv(&[(
        BStr::new("results"),
        PhpMixed::List(vec![
            str_kv(&[
                (BStr::new("name"), PhpMixed::string(BString::from("foo1"))),
                (BStr::new("description"), PhpMixed::Null),
                (BStr::new("abandoned"), PhpMixed::Bool(true)),
            ]),
            str_kv(&[
                (BStr::new("name"), PhpMixed::string(BString::from("foo2"))),
                (BStr::new("description"), PhpMixed::Null),
                (
                    BStr::new("abandoned"),
                    PhpMixed::string(BString::from("bar")),
                ),
            ]),
        ]),
    )]);

    let (http_downloader, _http_guard): (_, HttpDownloaderMockGuard) = get_http_downloader_mock(
        vec![
            http_body(
                BStr::new("http://example.org/packages.json"),
                json_encode(&str_kv(&[(
                    BStr::new("search"),
                    PhpMixed::string(BString::from("/search.json?q=%query%")),
                )])),
            ),
            http_body(
                BStr::new("http://example.org/search.json?q=foo"),
                json_encode(&result),
            ),
        ],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let mut repository = ComposerRepository::new(
        repo_config(BStr::new("http://example.org")),
        null_io(),
        &config,
        http_downloader,
        None,
    )
    .unwrap();

    let results = repository
        .search("foo".into(), SEARCH_FULLTEXT, None)
        .unwrap();

    assert_eq!(2, results.len());
    assert_eq!(
        Some("foo1".into()),
        results[0].get(B("name")).and_then(|v| v.as_string())
    );
    assert!(matches!(
        results[0].get(B("description")),
        Some(PhpMixed::Null)
    ));
    assert!(matches!(
        results[0].get(B("abandoned")),
        Some(PhpMixed::Bool(true))
    ));
    assert_eq!(
        Some("foo2".into()),
        results[1].get(B("name")).and_then(|v| v.as_string())
    );
    assert!(matches!(
        results[1].get(B("description")),
        Some(PhpMixed::Null)
    ));
    assert_eq!(
        Some("bar".into()),
        results[1].get(B("abandoned")).and_then(|v| v.as_string())
    );
}

fn canonicalize_url_test_cases() -> Vec<(&'static BStr, &'static BStr, &'static BStr)> {
    vec![
        (
            BStr::new("https://example.org/path/to/file"),
            BStr::new("/path/to/file"),
            BStr::new("https://example.org"),
        ),
        (
            BStr::new("https://example.org/canonic_url"),
            BStr::new("https://example.org/canonic_url"),
            BStr::new("https://should-not-see-me.test"),
        ),
        (
            BStr::new("file:///path/to/repository/file"),
            BStr::new("/path/to/repository/file"),
            BStr::new("file:///path/to/repository"),
        ),
        // Repository URL returned unchanged if it is not a URL (BC test).
        (
            BStr::new("invalid_repo_url"),
            BStr::new("/path/to/file"),
            BStr::new("invalid_repo_url"),
        ),
        // URLs may contain sequences resembling preg_replace() pattern references
        // without messing up the result (regression test).
        (
            BStr::new("https://example.org/path/to/unusual_$0_filename"),
            BStr::new("/path/to/unusual_$0_filename"),
            BStr::new("https://example.org"),
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
            BStr::new("http://example.org/packages.json"),
            json_encode(&str_kv(&[
                (
                    BStr::new("providers-lazy-url"),
                    PhpMixed::string(BString::from("/foo/p/%package%.json")),
                ),
                (
                    BStr::new("packages"),
                    str_kv(&[(
                        BStr::new("foo/bar"),
                        str_kv(&[
                            (
                                BStr::new("dev-branch"),
                                str_kv(&[(
                                    BStr::new("name"),
                                    PhpMixed::string(BString::from("foo/bar")),
                                )]),
                            ),
                            (
                                BStr::new("v1.0.0"),
                                str_kv(&[(
                                    BStr::new("name"),
                                    PhpMixed::string(BString::from("foo/bar")),
                                )]),
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
        repo_config(BStr::new("http://example.org/packages.json")),
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
                BStr::new("https://example.org/packages.json"),
                json_encode(&str_kv(&[
                    (
                        BStr::new("packages"),
                        str_kv(&[(
                            BStr::new("foo/bar"),
                            str_kv(&[
                                (
                                    BStr::new("dev-branch"),
                                    str_kv(&[(
                                        BStr::new("name"),
                                        PhpMixed::string(BString::from("foo/bar")),
                                    )]),
                                ),
                                (
                                    BStr::new("v1.0.0"),
                                    str_kv(&[(
                                        BStr::new("name"),
                                        PhpMixed::string(BString::from("foo/bar")),
                                    )]),
                                ),
                            ]),
                        )]),
                    ),
                    (
                        BStr::new("metadata-url"),
                        PhpMixed::string(BString::from("https://example.org/p2/%package%.json")),
                    ),
                    (
                        BStr::new("security-advisories"),
                        str_kv(&[(
                            BStr::new("api-url"),
                            PhpMixed::string(BString::from(
                                "https://example.org/security-advisories",
                            )),
                        )]),
                    ),
                ])),
            ),
            http_body(
                BStr::new("https://example.org/security-advisories"),
                json_encode(&str_kv(&[(
                    BStr::new("advisories"),
                    PhpMixed::List(vec![]),
                )])),
            ),
        ],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let mut repo_config: IndexMap<BString, PhpMixed> = IndexMap::new();
    repo_config.insert(
        "url".into(),
        PhpMixed::string(BString::from("https://example.org/packages.json")),
    );
    repo_config.insert(
        "options".into(),
        str_kv(&[(
            BStr::new("http"),
            str_kv(&[(BStr::new("verify_peer"), PhpMixed::Bool(false))]),
        )]),
    );

    let mut repository =
        ComposerRepository::new(repo_config, null_io(), &config, http_downloader, None).unwrap();

    let constraint: AnyConstraint =
        SimpleConstraint::new("=", BString::from("1.0.0.0"), None).into();
    let mut map: IndexMap<BString, AnyConstraint> = IndexMap::new();
    map.insert("foo/bar".into(), constraint);

    let result = repository.get_security_advisories(map, false).unwrap();
    assert!(result.names_found.is_empty());
    assert!(result.advisories.is_empty());
}

fn generate_security_advisory(
    package_name: &BStr,
    cve: Option<&BStr>,
    affected_versions: &BStr,
) -> (bstr::BString, PhpMixed) {
    let advisory_id = uniqid("PKSA-");
    let advisory = str_kv(&[
        (
            BStr::new("advisoryId"),
            PhpMixed::string(advisory_id.clone()),
        ),
        (
            BStr::new("packageName"),
            PhpMixed::string(package_name.to_php_string()),
        ),
        (
            BStr::new("remoteId"),
            PhpMixed::string(BString::from("test")),
        ),
        (
            BStr::new("title"),
            PhpMixed::string(BString::from("Security Advisory")),
        ),
        (BStr::new("link"), PhpMixed::Null),
        (
            BStr::new("cve"),
            match cve {
                Some(c) => PhpMixed::string(c.to_php_string()),
                None => PhpMixed::Null,
            },
        ),
        (
            BStr::new("affectedVersions"),
            PhpMixed::string(affected_versions.to_php_string()),
        ),
        (
            BStr::new("source"),
            PhpMixed::string(BString::from("Tests")),
        ),
        (
            BStr::new("reportedAt"),
            PhpMixed::string(BString::from("2024-04-31 12:37:47")),
        ),
        (
            BStr::new("composerRepository"),
            PhpMixed::string(BString::from("Package Repository")),
        ),
        (
            BStr::new("severity"),
            PhpMixed::string(BString::from("high")),
        ),
        (
            BStr::new("sources"),
            PhpMixed::List(vec![str_kv(&[
                (
                    BStr::new("name"),
                    PhpMixed::string(BString::from("Security Advisory")),
                ),
                (
                    BStr::new("remoteId"),
                    PhpMixed::string(BString::from("test")),
                ),
            ])]),
        ),
    ]);
    (advisory_id, advisory)
}

// Ported and exercising the real getSecurityAdvisories path, but blocked by an unimplemented
// production shim: constructing a full SecurityAdvisory parses `reportedAt` via
// shirabe_php_shim::date_create (todo!(): needs the strtotime grammar parser).
#[test]
fn test_get_security_advisories_assert_repository_advisories_is_zero_indexed_array_with_consecutive_keys()
 {
    let (config, _home) = create_config();

    let package_name = BStr::new("foo/bar");
    let (advisory1_id, advisory1) = generate_security_advisory(
        package_name,
        Some(BStr::new("CVE-1999-1000")),
        BStr::new(">=1.0.0,<1.1.0"),
    );
    let (_advisory2_id, advisory2) = generate_security_advisory(
        package_name,
        Some(BStr::new("CVE-1999-1000")),
        BStr::new(">=2.0.0"),
    );
    let (advisory3_id, advisory3) = generate_security_advisory(
        package_name,
        Some(BStr::new("CVE-1999-1000")),
        BStr::new(">=1.0.0,<1.1.0"),
    );

    let expected_advisory_ids = [advisory1_id, advisory3_id];

    let (http_downloader, _http_guard): (_, HttpDownloaderMockGuard) = get_http_downloader_mock(
        vec![
            http_body(
                BStr::new("https://example.org/packages.json"),
                json_encode(&str_kv(&[
                    (
                        BStr::new("packages"),
                        str_kv(&[(
                            package_name,
                            str_kv(&[
                                (
                                    BStr::new("dev-branch"),
                                    str_kv(&[(
                                        BStr::new("name"),
                                        PhpMixed::string(package_name.to_php_string()),
                                    )]),
                                ),
                                (
                                    BStr::new("v1.0.0"),
                                    str_kv(&[(
                                        BStr::new("name"),
                                        PhpMixed::string(package_name.to_php_string()),
                                    )]),
                                ),
                            ]),
                        )]),
                    ),
                    (
                        BStr::new("metadata-url"),
                        PhpMixed::string(BString::from("https://example.org/p2/%package%.json")),
                    ),
                    (
                        BStr::new("security-advisories"),
                        str_kv(&[(
                            BStr::new("api-url"),
                            PhpMixed::string(BString::from(
                                "https://example.org/security-advisories",
                            )),
                        )]),
                    ),
                ])),
            ),
            http_body(
                BStr::new("https://example.org/security-advisories"),
                json_encode(&str_kv(&[(
                    BStr::new("advisories"),
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

    let mut repo_config: IndexMap<BString, PhpMixed> = IndexMap::new();
    repo_config.insert(
        "url".into(),
        PhpMixed::string(BString::from("https://example.org/packages.json")),
    );
    repo_config.insert(
        "options".into(),
        str_kv(&[(
            BStr::new("http"),
            str_kv(&[(BStr::new("verify_peer"), PhpMixed::Bool(false))]),
        )]),
    );

    let mut repository =
        ComposerRepository::new(repo_config, null_io(), &config, http_downloader, None).unwrap();

    let constraint: AnyConstraint =
        SimpleConstraint::new("=", BString::from("1.0.0.0"), None).into();
    let mut map: IndexMap<BString, AnyConstraint> = IndexMap::new();
    map.insert(package_name.into(), constraint);

    let result = repository.get_security_advisories(map, false).unwrap();

    let actual = result.advisories.get(bstr::B(package_name)).unwrap();
    assert_eq!(expected_advisory_ids.len(), actual.len());
    for (i, expected_id) in expected_advisory_ids.iter().enumerate() {
        assert_eq!(expected_id, actual[i].advisory_id());
    }
}
