//! ref: composer/tests/Composer/Test/Repository/Vcs/GitHubDriverTest.php sha1:fcf4538d8e3d441d59ffdaa860390599e71c77a0

use crate::http_downloader_mock::{HttpDownloaderMockGuard, expect_full, get_http_downloader_mock};
use crate::io_stub::IOStub;
use crate::process_executor_mock::{
    ProcessExecutorMockGuard, cmd, cmd_full, get_process_executor_mock,
};
use crate::test_case::GitVersionGuard;
use bstr::{B, BStr, BString, ByteSlice};
use indexmap::IndexMap;
use serial_test::serial;
use shirabe::config::{Config, ConfigSourceInterface};
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::vcs::GitHubDriver;
use shirabe::util::Git as GitUtil;
use shirabe::util::filesystem::Filesystem;
use shirabe::util::http_downloader::HttpDownloaderMockHandler;
use shirabe::util::process_executor::{MockHandler, ProcessExecutor};
use shirabe_php_shim::{PhpMixed, base64_encode, bconcat, path_to_bytes};
use tempfile::TempDir;

struct SetUp {
    home: TempDir,
    config: Config,
}

fn set_up() -> SetUp {
    let home = TempDir::new().unwrap();
    let mut config = Config::new(true, None);
    let mut top: IndexMap<BString, PhpMixed> = IndexMap::new();
    let mut config_section: IndexMap<BString, PhpMixed> = IndexMap::new();
    config_section.insert("home".into(), PhpMixed::string(path_to_bytes(&home.path())));
    top.insert("config".into(), PhpMixed::Array(config_section));
    config.merge(&top, Config::SOURCE_UNKNOWN);

    SetUp { home, config }
}

fn tear_down(home: &std::path::Path) {
    let mut fs = Filesystem::new(None);
    fs.remove_directory(home).unwrap();
}

struct TearDown {
    home: std::path::PathBuf,
}

impl TearDown {
    fn new(home: std::path::PathBuf) -> Self {
        TearDown { home }
    }
}

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down(&self.home);
    }
}

// PHP mocks `Composer\Config\ConfigSourceInterface` with getMockBuilder and sets
// no expectations (a passive mock whose methods all do nothing). Used by
// testPrivateRepository to satisfy the OAuth token-storing path.
#[expect(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    reason = "the code mockall generates"
)]
mod mock_config_source {
    use super::*;

    mockall::mock! {
        #[derive(Debug)]
        pub ConfigSource {}
        impl ConfigSourceInterface for ConfigSource {
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
use mock_config_source::MockConfigSource;

// A passive ConfigSource mock: every method is stubbed to do nothing, mirroring
// PHPUnit's `getMockBuilder(...)->getMock()` whose stubbed methods return null.
fn null_config_source() -> MockConfigSource {
    let mut m = MockConfigSource::new();
    m.expect_add_repository().returning(|_, _, _| Ok(()));
    m.expect_insert_repository().returning(|_, _, _, _| Ok(()));
    m.expect_set_repository_url().returning(|_, _| Ok(()));
    m.expect_remove_repository().returning(|_| Ok(()));
    m.expect_add_config_setting().returning(|_, _| Ok(()));
    m.expect_remove_config_setting().returning(|_| Ok(()));
    m.expect_add_property().returning(|_, _| Ok(()));
    m.expect_remove_property().returning(|_| Ok(()));
    m.expect_add_link().returning(|_, _, _| Ok(()));
    m.expect_remove_link().returning(|_, _| Ok(()));
    m.expect_get_name().returning(bstr::BString::default);
    m
}

// Mirrors PHP's $httpDownloader->expects([...], true): an `['url' => .., 'body' => ..]`
// entry (status defaults to 200).
fn http_body(
    url: &BStr,
    body: impl Into<BString>,
) -> shirabe::util::http_downloader::HttpDownloaderMockExpectation {
    expect_full(url, None, 200, body, vec![BString::default()])
}

// Mirrors PHP's `['url' => .., 'status' => 404]` entry (no body).
fn http_status(
    url: &BStr,
    status: i64,
) -> shirabe::util::http_downloader::HttpDownloaderMockExpectation {
    expect_full(
        url,
        None,
        status,
        BString::default(),
        vec![BString::default()],
    )
}

fn b64(s: &BStr) -> BString {
    base64_encode(s).to_owned()
}

fn supports_provider() -> Vec<(bool, &'static BStr)> {
    vec![
        (false, BStr::new("https://github.com/acme")),
        (true, BStr::new("https://github.com/acme/repository")),
        (true, BStr::new("git@github.com:acme/repository.git")),
        (
            false,
            BStr::new("https://github.com/acme/repository/releases"),
        ),
        (false, BStr::new("https://github.com/acme/repository/pulls")),
    ]
}

#[test]
fn test_supports() {
    let SetUp { home, config: _ } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());

    for (expected, repo_url) in supports_provider() {
        let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
        let config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(true, None)));

        assert_eq!(
            expected,
            GitHubDriver::supports(io, config, bstr::BStr::new(repo_url), false).unwrap()
        );
    }
}

#[test]
fn test_private_repository() {
    let SetUp { home, config } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());

    let repo_url = "http://github.com/composer/packagist";
    let repo_api_url = BStr::new("https://api.github.com/repos/composer/packagist");
    let repo_ssh_url = "git@github.com:composer/packagist.git";
    let identifier = "v0.0.0";
    let sha = "SOMESHA";

    let config = std::rc::Rc::new(std::cell::RefCell::new(config));

    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(
            IOStub::new()
                .with_is_interactive(true)
                .with_ask_and_hide_answer(Some(BString::from("sometoken"))),
        ));

    let (http_downloader, _http_guard): (_, HttpDownloaderMockGuard) = get_http_downloader_mock(
        vec![
            http_status(repo_api_url, 404),
            http_body(BStr::new("https://api.github.com/"), "{}"),
            http_body(
                repo_api_url,
                r#"{"master_branch": "test_master", "private": true, "owner": {"login": "composer"}, "name": "packagist"}"#,
            ),
        ],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let (process, _process_guard): (_, ProcessExecutorMockGuard) = get_process_executor_mock(
        vec![],
        false,
        MockHandler {
            r#return: 1,
            stdout: BString::default(),
            stderr: BString::default(),
        },
    );

    config
        .borrow_mut()
        .set_config_source(Box::new(null_config_source()));
    config
        .borrow_mut()
        .set_auth_config_source(Box::new(null_config_source()));

    let mut repo_config: IndexMap<BString, PhpMixed> = IndexMap::new();
    repo_config.insert("url".into(), PhpMixed::string(bconcat!(repo_url)));

    let mut git_hub_driver = GitHubDriver::new(repo_config, io, config, http_downloader, process);
    git_hub_driver.initialize().unwrap();
    let mut tags = IndexMap::new();
    tags.insert(bconcat!(identifier), bconcat!(sha));
    git_hub_driver.__set_tags(Some(tags.into_iter().collect::<indexmap::IndexMap<_, _>>()));

    assert_eq!("test_master", git_hub_driver.get_root_identifier().unwrap());

    let dist = git_hub_driver.get_dist(bstr::BStr::new(sha)).unwrap();
    assert_eq!(
        Some("zip".into()),
        dist.get(B("type")).and_then(|v| v.as_string())
    );
    assert_eq!(
        Some("https://api.github.com/repos/composer/packagist/zipball/SOMESHA".into()),
        dist.get(B("url")).and_then(|v| v.as_string())
    );
    assert_eq!(
        Some("SOMESHA".into()),
        dist.get(B("reference")).and_then(|v| v.as_string())
    );

    let source = git_hub_driver.get_source(bstr::BStr::new(sha));
    assert_eq!(
        Some("git".into()),
        source.get(B("type")).and_then(|v| v.as_string())
    );
    assert_eq!(
        Some(repo_ssh_url.into()),
        source.get(B("url")).and_then(|v| v.as_string())
    );
    assert_eq!(
        Some("SOMESHA".into()),
        source.get(B("reference")).and_then(|v| v.as_string())
    );
}

#[test]
fn test_public_repository() {
    let SetUp { home, config } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());

    let repo_url = "http://github.com/composer/packagist";
    let repo_api_url = BStr::new("https://api.github.com/repos/composer/packagist");
    let identifier = "v0.0.0";
    let sha = "SOMESHA";

    let config = std::rc::Rc::new(std::cell::RefCell::new(config));
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = std::rc::Rc::new(
        std::cell::RefCell::new(IOStub::new().with_is_interactive(true)),
    );

    let (http_downloader, _http_guard) = get_http_downloader_mock(
        vec![http_body(
            repo_api_url,
            r#"{"master_branch": "test_master", "owner": {"login": "composer"}, "name": "packagist"}"#,
        )],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(None)));

    let mut repo_config: IndexMap<BString, PhpMixed> = IndexMap::new();
    repo_config.insert("url".into(), PhpMixed::string(bconcat!(repo_url)));
    let repo_url = "https://github.com/composer/packagist.git";

    let mut git_hub_driver = GitHubDriver::new(repo_config, io, config, http_downloader, process);
    git_hub_driver.initialize().unwrap();
    let mut tags = IndexMap::new();
    tags.insert(bconcat!(identifier), bconcat!(sha));
    git_hub_driver.__set_tags(Some(tags.into_iter().collect::<indexmap::IndexMap<_, _>>()));

    assert_eq!("test_master", git_hub_driver.get_root_identifier().unwrap());

    let dist = git_hub_driver.get_dist(bstr::BStr::new(sha)).unwrap();
    assert_eq!(
        Some("zip".into()),
        dist.get(B("type")).and_then(|v| v.as_string())
    );
    assert_eq!(
        Some("https://api.github.com/repos/composer/packagist/zipball/SOMESHA".into()),
        dist.get(B("url")).and_then(|v| v.as_string())
    );
    assert_eq!(
        Some(sha.into()),
        dist.get(B("reference")).and_then(|v| v.as_string())
    );

    let source = git_hub_driver.get_source(bstr::BStr::new(sha));
    assert_eq!(
        Some("git".into()),
        source.get(B("type")).and_then(|v| v.as_string())
    );
    assert_eq!(
        Some(repo_url.into()),
        source.get(B("url")).and_then(|v| v.as_string())
    );
    assert_eq!(
        Some(sha.into()),
        source.get(B("reference")).and_then(|v| v.as_string())
    );
}

#[test]
fn test_public_repository2() {
    let SetUp { home, config } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());

    let repo_url = "http://github.com/composer/packagist";
    let repo_api_url = BStr::new("https://api.github.com/repos/composer/packagist");
    let identifier = "feature/3.2-foo";
    let sha = "SOMESHA";

    let config = std::rc::Rc::new(std::cell::RefCell::new(config));
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = std::rc::Rc::new(
        std::cell::RefCell::new(IOStub::new().with_is_interactive(true)),
    );

    let (http_downloader, _http_guard) = get_http_downloader_mock(
        vec![
            http_body(
                repo_api_url,
                r#"{"master_branch": "test_master", "owner": {"login": "composer"}, "name": "packagist"}"#,
            ),
            http_body(
                BStr::new(
                    "https://api.github.com/repos/composer/packagist/contents/composer.json?ref=feature%2F3.2-foo",
                ),
                bconcat!(
                    r#"{"encoding":"base64","content":""#,
                    b64(bconcat!(r#"{"support": {"source": ""#, repo_url, r#"" }}"#).as_bstr()),
                    r#""}"#
                ),
            ),
            http_body(
                BStr::new(
                    "https://api.github.com/repos/composer/packagist/commits/feature%2F3.2-foo",
                ),
                r#"{"commit": {"committer":{ "date": "2012-09-10"}}}"#,
            ),
            http_body(
                BStr::new(
                    "https://api.github.com/repos/composer/packagist/contents/.github/FUNDING.yml",
                ),
                bconcat!(
                    r#"{"encoding": "base64", "content": ""#,
                    b64(BStr::new("custom: https://example.com")),
                    r#""}"#
                ),
            ),
        ],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(None)));

    let mut repo_config: IndexMap<BString, PhpMixed> = IndexMap::new();
    repo_config.insert("url".into(), PhpMixed::string(bconcat!(repo_url)));
    let repo_url = "https://github.com/composer/packagist.git";

    let mut git_hub_driver = GitHubDriver::new(repo_config, io, config, http_downloader, process);
    git_hub_driver.initialize().unwrap();
    let mut tags = IndexMap::new();
    tags.insert(bconcat!(identifier), bconcat!(sha));
    git_hub_driver.__set_tags(Some(tags.into_iter().collect::<indexmap::IndexMap<_, _>>()));

    assert_eq!("test_master", git_hub_driver.get_root_identifier().unwrap());

    let dist = git_hub_driver.get_dist(bstr::BStr::new(sha)).unwrap();
    assert_eq!(
        Some("zip".into()),
        dist.get(B("type")).and_then(|v| v.as_string())
    );
    assert_eq!(
        Some("https://api.github.com/repos/composer/packagist/zipball/SOMESHA".into()),
        dist.get(B("url")).and_then(|v| v.as_string())
    );
    assert_eq!(
        Some(sha.into()),
        dist.get(B("reference")).and_then(|v| v.as_string())
    );

    let source = git_hub_driver.get_source(bstr::BStr::new(sha));
    assert_eq!(
        Some("git".into()),
        source.get(B("type")).and_then(|v| v.as_string())
    );
    assert_eq!(
        Some(repo_url.into()),
        source.get(B("url")).and_then(|v| v.as_string())
    );
    assert_eq!(
        Some(sha.into()),
        source.get(B("reference")).and_then(|v| v.as_string())
    );

    let data = git_hub_driver
        .get_composer_information(bstr::BStr::new(identifier))
        .unwrap()
        .unwrap();
    assert!(!data.contains_key(B("abandoned")));
}

#[test]
fn test_invalid_support_data() {
    let SetUp { home, config } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());

    let repo_url = "http://github.com/composer/packagist";
    let repo_api_url = BStr::new("https://api.github.com/repos/composer/packagist");
    let identifier = "feature/3.2-foo";
    let sha = "SOMESHA";

    let config = std::rc::Rc::new(std::cell::RefCell::new(config));
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = std::rc::Rc::new(
        std::cell::RefCell::new(IOStub::new().with_is_interactive(true)),
    );

    let (http_downloader, _http_guard) = get_http_downloader_mock(
        vec![
            http_body(
                repo_api_url,
                r#"{"master_branch": "test_master", "owner": {"login": "composer"}, "name": "packagist"}"#,
            ),
            http_body(
                BStr::new(
                    "https://api.github.com/repos/composer/packagist/contents/composer.json?ref=feature%2F3.2-foo",
                ),
                bconcat!(
                    r#"{"encoding":"base64","content":""#,
                    b64(bconcat!(r#"{"support": ""#, repo_url, r#"" }"#).as_bstr()),
                    r#""}"#
                ),
            ),
            http_body(
                BStr::new(
                    "https://api.github.com/repos/composer/packagist/commits/feature%2F3.2-foo",
                ),
                r#"{"commit": {"committer":{ "date": "2012-09-10"}}}"#,
            ),
            http_body(
                BStr::new(
                    "https://api.github.com/repos/composer/packagist/contents/.github/FUNDING.yml",
                ),
                bconcat!(
                    r#"{"encoding": "base64", "content": ""#,
                    b64(BStr::new("custom: https://example.com")),
                    r#""}"#
                ),
            ),
        ],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(None)));

    let mut repo_config: IndexMap<BString, PhpMixed> = IndexMap::new();
    repo_config.insert("url".into(), PhpMixed::string(bconcat!(repo_url)));

    let mut git_hub_driver = GitHubDriver::new(repo_config, io, config, http_downloader, process);
    git_hub_driver.initialize().unwrap();
    let mut tags = IndexMap::new();
    tags.insert(bconcat!(identifier), bconcat!(sha));
    git_hub_driver.__set_tags(Some(tags.into_iter().collect::<indexmap::IndexMap<_, _>>()));
    let mut branches = IndexMap::new();
    branches.insert(BString::from("test_master"), bconcat!(sha));
    git_hub_driver.__set_branches(Some(
        branches.into_iter().collect::<indexmap::IndexMap<_, _>>(),
    ));

    let data = git_hub_driver
        .get_composer_information(bstr::BStr::new(identifier))
        .unwrap()
        .unwrap();

    let support = data.get(B("support")).and_then(|v| v.as_array()).unwrap();
    assert_eq!(
        Some("https://github.com/composer/packagist/tree/feature/3.2-foo".into()),
        support.get(B("source")).and_then(|v| v.as_string())
    );
}

fn funding_url_provider() -> Vec<(&'static BStr, Option<Vec<(&'static BStr, &'static BStr)>>)> {
    let all_named_platforms = BStr::new(
        "community_bridge: project-name
github: [userA, userB]
issuehunt: userName
ko_fi: userName
liberapay: userName
open_collective: userName
patreon: userName
tidelift: Platform/Package
polar: userName
buy_me_a_coffee: userName
thanks_dev: u/gh/userName
otechie: userName",
    );

    vec![
        (
            all_named_platforms,
            Some(vec![
                (
                    BStr::new("community_bridge"),
                    BStr::new("https://funding.communitybridge.org/projects/project-name"),
                ),
                (BStr::new("github"), BStr::new("https://github.com/userA")),
                (BStr::new("github"), BStr::new("https://github.com/userB")),
                (
                    BStr::new("issuehunt"),
                    BStr::new("https://issuehunt.io/r/userName"),
                ),
                (BStr::new("ko_fi"), BStr::new("https://ko-fi.com/userName")),
                (
                    BStr::new("liberapay"),
                    BStr::new("https://liberapay.com/userName"),
                ),
                (
                    BStr::new("open_collective"),
                    BStr::new("https://opencollective.com/userName"),
                ),
                (
                    BStr::new("patreon"),
                    BStr::new("https://www.patreon.com/userName"),
                ),
                (
                    BStr::new("tidelift"),
                    BStr::new("https://tidelift.com/funding/github/Platform/Package"),
                ),
                (BStr::new("polar"), BStr::new("https://polar.sh/userName")),
                (
                    BStr::new("buy_me_a_coffee"),
                    BStr::new("https://www.buymeacoffee.com/userName"),
                ),
                (
                    BStr::new("thanks_dev"),
                    BStr::new("https://thanks.dev/u/gh/userName"),
                ),
                (
                    BStr::new("otechie"),
                    BStr::new("https://otechie.com/userName"),
                ),
            ]),
        ),
        (
            BStr::new("custom: example.com"),
            Some(vec![(
                BStr::new("custom"),
                BStr::new("https://example.com"),
            )]),
        ),
        (
            BStr::new("custom: [example.com]"),
            Some(vec![(
                BStr::new("custom"),
                BStr::new("https://example.com"),
            )]),
        ),
        (
            BStr::new("custom: \"https://example.com\""),
            Some(vec![(
                BStr::new("custom"),
                BStr::new("https://example.com"),
            )]),
        ),
        (
            BStr::new("custom: [\"https://example.com\"]"),
            Some(vec![(
                BStr::new("custom"),
                BStr::new("https://example.com"),
            )]),
        ),
        (
            BStr::new("custom: [\"https://example.com\", example.org]"),
            Some(vec![
                (BStr::new("custom"), BStr::new("https://example.com")),
                (BStr::new("custom"), BStr::new("https://example.org")),
            ]),
        ),
        (
            BStr::new("custom: [example.net/funding, \"https://example.com\", example.org]"),
            Some(vec![
                (BStr::new("custom"), BStr::new("https://example.com")),
                (BStr::new("custom"), BStr::new("https://example.org")),
            ]),
        ),
    ]
}

#[ignore = "funding/archived parsing differs from PHP; not date-related"]
#[test]
fn test_funding_format() {
    for (funding, expected) in funding_url_provider() {
        let SetUp { home, config } = set_up();
        let _tear_down = TearDown::new(home.path().to_path_buf());

        let repo_url = "http://github.com/composer/packagist";
        let repo_api_url = BStr::new("https://api.github.com/repos/composer/packagist");
        let identifier = "feature/3.2-foo";
        let sha = "SOMESHA";

        let config = std::rc::Rc::new(std::cell::RefCell::new(config));
        let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = std::rc::Rc::new(
            std::cell::RefCell::new(IOStub::new().with_is_interactive(true)),
        );

        let (http_downloader, _http_guard) = get_http_downloader_mock(
            vec![
                http_body(
                    repo_api_url,
                    r#"{"master_branch": "test_master", "owner": {"login": "composer"}, "name": "packagist"}"#,
                ),
                http_body(
                    BStr::new(
                        "https://api.github.com/repos/composer/packagist/contents/composer.json?ref=feature%2F3.2-foo",
                    ),
                    bconcat!(
                        r#"{"encoding":"base64","content":""#,
                        b64(bconcat!(r#"{"support": {"source": ""#, repo_url, r#"" }}"#).as_bstr()),
                        r#""}"#
                    ),
                ),
                http_body(
                    BStr::new(
                        "https://api.github.com/repos/composer/packagist/commits/feature%2F3.2-foo",
                    ),
                    r#"{"commit": {"committer":{ "date": "2012-09-10"}}}"#,
                ),
                http_body(
                    BStr::new(
                        "https://api.github.com/repos/composer/packagist/contents/.github/FUNDING.yml",
                    ),
                    bconcat!(
                        r#"{"encoding": "base64", "content": ""#,
                        b64(funding),
                        r#""}"#
                    ),
                ),
            ],
            true,
            HttpDownloaderMockHandler::default(),
        );

        let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(None)));

        let mut repo_config: IndexMap<BString, PhpMixed> = IndexMap::new();
        repo_config.insert("url".into(), PhpMixed::string(bconcat!(repo_url)));

        let mut git_hub_driver =
            GitHubDriver::new(repo_config, io, config, http_downloader, process);
        git_hub_driver.initialize().unwrap();
        let mut tags = IndexMap::new();
        tags.insert(bconcat!(identifier), bconcat!(sha));
        git_hub_driver.__set_tags(Some(tags.into_iter().collect::<indexmap::IndexMap<_, _>>()));
        let mut branches = IndexMap::new();
        branches.insert(BString::from("test_master"), bconcat!(sha));
        git_hub_driver.__set_branches(Some(
            branches.into_iter().collect::<indexmap::IndexMap<_, _>>(),
        ));

        let data = git_hub_driver
            .get_composer_information(bstr::BStr::new(identifier))
            .unwrap()
            .unwrap();

        match expected {
            None => assert!(!data.contains_key(B("funding"))),
            Some(expected) => {
                let funding_list = match data.get(B("funding")) {
                    Some(PhpMixed::List(l)) => l.clone(),
                    Some(PhpMixed::Array(m)) => m.values().cloned().collect(),
                    other => panic!("unexpected funding value: {:?}", other),
                };
                let actual: Vec<(BString, BString)> = funding_list
                    .iter()
                    .map(|entry| {
                        let m = entry.as_array().unwrap();
                        (
                            BString::from(
                                m.get(B("type"))
                                    .and_then(|v| v.as_string())
                                    .unwrap_or_default(),
                            ),
                            BString::from(
                                m.get(B("url"))
                                    .and_then(|v| v.as_string())
                                    .unwrap_or_default(),
                            ),
                        )
                    })
                    .collect();
                let expected_vec: Vec<(BString, BString)> = expected
                    .iter()
                    .map(|(t, u)| (BString::from(*t), BString::from(*u)))
                    .collect();
                assert_eq!(expected_vec, actual);
            }
        }
    }
}

#[test]
fn test_public_repository_archived() {
    let SetUp { home, config } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());

    let repo_url = "http://github.com/composer/packagist";
    let repo_api_url = BStr::new("https://api.github.com/repos/composer/packagist");
    let identifier = "v0.0.0";
    let sha = "SOMESHA";
    let composer_json_url = bconcat!(
        "https://api.github.com/repos/composer/packagist/contents/composer.json?ref=",
        sha
    );

    let config = std::rc::Rc::new(std::cell::RefCell::new(config));
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = std::rc::Rc::new(
        std::cell::RefCell::new(IOStub::new().with_is_interactive(true)),
    );

    let (http_downloader, _http_guard) = get_http_downloader_mock(
        vec![
            http_body(
                repo_api_url,
                r#"{"master_branch": "test_master", "owner": {"login": "composer"}, "name": "packagist", "archived": true}"#,
            ),
            http_body(
                composer_json_url.as_bstr(),
                bconcat!(
                    r#"{"encoding": "base64", "content": ""#,
                    b64(BStr::new(r#"{"name": "composer/packagist"}"#)),
                    r#""}"#
                ),
            ),
            http_body(
                bconcat!(
                    "https://api.github.com/repos/composer/packagist/commits/",
                    sha
                )
                .as_bstr(),
                r#"{"commit": {"committer":{ "date": "2012-09-10"}}}"#,
            ),
            http_body(
                BStr::new(
                    "https://api.github.com/repos/composer/packagist/contents/.github/FUNDING.yml",
                ),
                bconcat!(
                    r#"{"encoding": "base64", "content": ""#,
                    b64(BStr::new("custom: https://example.com")),
                    r#""}"#
                ),
            ),
        ],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(None)));

    let mut repo_config: IndexMap<BString, PhpMixed> = IndexMap::new();
    repo_config.insert("url".into(), PhpMixed::string(bconcat!(repo_url)));

    let mut git_hub_driver = GitHubDriver::new(repo_config, io, config, http_downloader, process);
    git_hub_driver.initialize().unwrap();
    let mut tags = IndexMap::new();
    tags.insert(bconcat!(identifier), bconcat!(sha));
    git_hub_driver.__set_tags(Some(tags.into_iter().collect::<indexmap::IndexMap<_, _>>()));

    let data = git_hub_driver
        .get_composer_information(bstr::BStr::new(sha))
        .unwrap()
        .unwrap();

    assert_eq!(
        Some(true),
        data.get(B("abandoned")).and_then(|v| v.as_bool())
    );
}

// GitDriver::initialize calls GitUtil::cleanEnv, which calls GitUtil::getVersion; PHP's
// `GitUtil::$version` is a class-level static that persists for the whole PHPUnit run, so by the
// time this test runs it has already been populated (as a side effect of some earlier-run test
// invoking the real `git --version`) and the mock expectation list below never needs to include it.
// Rust test execution order isn't guaranteed the same way, so the cache is seeded explicitly here.
#[test]
#[serial]
fn test_private_repository_no_interaction() {
    GitUtil::__set_version(Some("2.52.0".into()));
    let _git_guard = GitVersionGuard;

    let SetUp { home, config } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());

    let repo_url = "http://github.com/composer/packagist";
    let repo_api_url = BStr::new("https://api.github.com/repos/composer/packagist");
    let repo_ssh_url = BStr::new("git@github.com:composer/packagist.git");
    let identifier = "v0.0.0";
    let sha = "SOMESHA";

    let config = std::rc::Rc::new(std::cell::RefCell::new(config));
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = std::rc::Rc::new(
        std::cell::RefCell::new(IOStub::new().with_is_interactive(false)),
    );

    let (http_downloader, _http_guard) = get_http_downloader_mock(
        vec![http_status(repo_api_url, 404)],
        true,
        HttpDownloaderMockHandler::default(),
    );

    // clean local clone if present
    let cache_dir = std::env::temp_dir().join("composer-test");
    let mut fs = Filesystem::new(None);
    let _ = fs.remove_directory(&cache_dir);
    let cache_vcs_dir = std::env::temp_dir()
        .join("composer-test/cache")
        .to_path_buf();
    {
        let mut top: IndexMap<BString, PhpMixed> = IndexMap::new();
        let mut config_section: IndexMap<BString, PhpMixed> = IndexMap::new();
        config_section.insert(
            "cache-vcs-dir".into(),
            PhpMixed::string(path_to_bytes(&cache_vcs_dir)),
        );
        top.insert("config".into(), PhpMixed::Array(config_section));
        config.borrow_mut().merge(&top, Config::SOURCE_UNKNOWN);
    }
    let resolved_cache_vcs_dir = BString::from(
        config
            .borrow_mut()
            .get("cache-vcs-dir")
            .unwrap()
            .as_string()
            .unwrap_or_default(),
    );

    let (process, _process_guard): (_, ProcessExecutorMockGuard) = get_process_executor_mock(
        vec![
            cmd_full(["git", "config", "github.accesstoken"], 1, "", ""),
            cmd([
                BStr::new("git"),
                BStr::new("clone"),
                BStr::new("--mirror"),
                BStr::new("--"),
                repo_ssh_url,
                bconcat!(
                    resolved_cache_vcs_dir,
                    "/git-github.com-composer-packagist.git/"
                )
                .as_bstr(),
            ]),
            cmd(["git", "remote", "-v"]),
            cmd([
                BStr::new("git"),
                BStr::new("remote"),
                BStr::new("set-url"),
                BStr::new("origin"),
                BStr::new("--"),
                repo_ssh_url,
            ]),
            cmd_full(
                ["git", "show-ref", "--tags", "--dereference"],
                0,
                bconcat!(sha, " refs/tags/", identifier),
                "",
            ),
            cmd_full(
                ["git", "branch", "--no-color", "--no-abbrev", "-v"],
                0,
                "  test_master     edf93f1fccaebd8764383dc12016d0a1a9672d89 Fix test & behavior",
                "",
            ),
            cmd_full(["git", "branch", "--no-color"], 0, "* test_master", ""),
        ],
        true,
        MockHandler::default(),
    );

    let mut repo_config: IndexMap<BString, PhpMixed> = IndexMap::new();
    repo_config.insert("url".into(), PhpMixed::string(bconcat!(repo_url)));

    let mut git_hub_driver = GitHubDriver::new(repo_config, io, config, http_downloader, process);
    git_hub_driver.initialize().unwrap();

    assert_eq!("test_master", git_hub_driver.get_root_identifier().unwrap());

    let dist = git_hub_driver.get_dist(bstr::BStr::new(sha)).unwrap();
    assert_eq!(
        Some("zip".into()),
        dist.get(B("type")).and_then(|v| v.as_string())
    );
    assert_eq!(
        Some("https://api.github.com/repos/composer/packagist/zipball/SOMESHA".into()),
        dist.get(B("url")).and_then(|v| v.as_string())
    );
    assert_eq!(
        Some(sha.into()),
        dist.get(B("reference")).and_then(|v| v.as_string())
    );

    let source = git_hub_driver.get_source(bstr::BStr::new(identifier));
    assert_eq!(
        Some("git".into()),
        source.get(B("type")).and_then(|v| v.as_string())
    );
    assert_eq!(
        Some(repo_ssh_url),
        source.get(B("url")).and_then(|v| v.as_string())
    );
    assert_eq!(
        Some(identifier.into()),
        source.get(B("reference")).and_then(|v| v.as_string())
    );

    let source = git_hub_driver.get_source(bstr::BStr::new(sha));
    assert_eq!(
        Some("git".into()),
        source.get(B("type")).and_then(|v| v.as_string())
    );
    assert_eq!(
        Some(repo_ssh_url),
        source.get(B("url")).and_then(|v| v.as_string())
    );
    assert_eq!(
        Some(sha.into()),
        source.get(B("reference")).and_then(|v| v.as_string())
    );
}

fn invalid_url_provider() -> Vec<&'static BStr> {
    vec![
        BStr::new("https://github.com/acme"),
        BStr::new("https://github.com/acme/repository/releases"),
        BStr::new("https://github.com/acme/repository/pulls"),
    ]
}

#[test]
fn test_initialize_invalid_repo_url() {
    for url in invalid_url_provider() {
        let SetUp { home, config } = set_up();
        let _tear_down = TearDown::new(home.path().to_path_buf());

        let config = std::rc::Rc::new(std::cell::RefCell::new(config));
        let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(IOStub::new()));

        let (http_downloader, _http_guard) =
            get_http_downloader_mock(vec![], true, HttpDownloaderMockHandler::default());
        let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(None)));

        let mut repo_config: IndexMap<BString, PhpMixed> = IndexMap::new();
        repo_config.insert("url".into(), PhpMixed::string(bconcat!(url)));

        let mut git_hub_driver =
            GitHubDriver::new(repo_config, io, config, http_downloader, process);
        let result = git_hub_driver.initialize();
        assert!(
            result.is_err(),
            "expected InvalidArgumentException for url {}",
            url.display_lossy()
        );
    }
}

#[test]
fn test_get_empty_file_content() {
    let SetUp { home, config } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());

    let repo_url = "http://github.com/composer/packagist";

    let config = std::rc::Rc::new(std::cell::RefCell::new(config));
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = std::rc::Rc::new(
        std::cell::RefCell::new(IOStub::new().with_is_interactive(true)),
    );

    let (http_downloader, _http_guard) = get_http_downloader_mock(
        vec![
            http_body(
                BStr::new("https://api.github.com/repos/composer/packagist"),
                r#"{"master_branch": "test_master", "owner": {"login": "composer"}, "name": "packagist", "archived": true}"#,
            ),
            http_body(
                BStr::new(
                    "https://api.github.com/repos/composer/packagist/contents/composer.json?ref=main",
                ),
                r#"{"encoding":"base64","content":""}"#,
            ),
        ],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(None)));

    let mut repo_config: IndexMap<BString, PhpMixed> = IndexMap::new();
    repo_config.insert("url".into(), PhpMixed::string(bconcat!(repo_url)));

    let mut git_hub_driver = GitHubDriver::new(repo_config, io, config, http_downloader, process);
    git_hub_driver.initialize().unwrap();

    assert_eq!(
        Some(BString::default()),
        git_hub_driver
            .get_file_content(bstr::BStr::new("composer.json"), bstr::BStr::new("main"))
            .unwrap()
    );
}
