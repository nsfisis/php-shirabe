//! ref: composer/tests/Composer/Test/Downloader/GitDownloaderTest.php sha1:a9bcb3ee839fdf412f8d8c9842be34da113389ef

use crate::async_runtime::run;
use crate::config_stub::ConfigStubBuilder;
use crate::io_mock::{Expectation, get_io_mock};
use crate::io_stub::IOStub;
use crate::process_executor_mock::{cmd, cmd_full, get_process_executor_mock};
use crate::test_case::get_unique_tmp_directory;
use bstr::{BStr, BString, ByteSlice as _};
use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::downloader::{GitDownloader, VcsDownloader};
use shirabe::io::{IOInterface, io_interface};
use shirabe::package::{CompletePackageHandle, Mirror, PackageInterfaceHandle};
use shirabe::util::{Filesystem, FilesystemMock, Git as GitUtil, ProcessExecutor};
use shirabe_php_shim::{
    PhpMixed, ToPhpString, bformat, error_message, path_from_bytes, path_to_bytes,
};
use shirabe_semver::VersionParser;

fn set_up() -> tempfile::TempDir {
    // skipIfNotExecutable('git') is irrelevant because every git invocation is mocked.

    // initGitVersion('1.0.0'): seed the Composer\Util\Git static version cache.
    GitUtil::__set_version(Some("1.0.0".into()));

    get_unique_tmp_directory()
}

fn tear_down(working_dir: &std::path::Path) {
    if working_dir.is_dir() {
        let mut fs = Filesystem::new(None);
        fs.remove_directory(working_dir).unwrap();
    }
    // initGitVersion(false): drop the static version cache so it is re-detected next time.
    GitUtil::__set_version(None);
}

struct TearDown {
    working_dir: std::path::PathBuf,
}

impl TearDown {
    fn new(working_dir: std::path::PathBuf) -> Self {
        TearDown { working_dir }
    }
}

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down(&self.working_dir);
    }
}

/// ref: TestCase::getMockBuilder('Composer\Package\PackageInterface')->getMock()
///
/// A `getMockBuilder(PackageInterface)` mock returns null/0 for every unstubbed
/// method, so a real CompletePackage seeded with the stubbed values is a faithful
/// stand-in only when the resulting `getSourceUrls()` equals `[getSourceUrl()]`.
/// The multi-URL/mirror cases in PHP configure `getSourceUrls` to an arbitrary
/// list that a real package cannot reproduce, so those tests stay ignored.
fn get_package(
    name: &BStr,
    pretty_version: &BStr,
    source_reference: Option<&BStr>,
    source_url: Option<&BStr>,
) -> PackageInterfaceHandle {
    let norm_version = VersionParser.normalize(pretty_version, None).unwrap();
    let package = CompletePackageHandle::new(
        name.to_php_string(),
        norm_version,
        pretty_version.to_php_string(),
    );
    package.__set_source_type(Some(BString::from("git")));
    package.set_source_reference(source_reference.map(|s| s.to_php_string()));
    package.set_source_url(source_url.map(|s| s.to_php_string()));
    package.into()
}

/// ref: GitDownloaderTest::setupConfig (seeds a temp `home` when none is set)
fn setup_config(config: Config) -> Config {
    let mut config = config;
    if !config.has("home") {
        let tmp_dir = tempfile::TempDir::new().unwrap().keep();
        let mut top: IndexMap<BString, PhpMixed> = IndexMap::new();
        let mut section: IndexMap<BString, PhpMixed> = IndexMap::new();
        section.insert("home".into(), PhpMixed::string(path_to_bytes(&tmp_dir)));
        top.insert("config".into(), PhpMixed::Array(section));
        config.merge(&top, Config::SOURCE_UNKNOWN);
    }
    config
}

/// ref: GitDownloaderTest::getDownloaderMock (defaults the IO/Config/Filesystem)
fn get_downloader_mock(
    io: Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>,
    config: Option<Config>,
    process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    filesystem: Option<std::rc::Rc<std::cell::RefCell<Filesystem>>>,
) -> GitDownloader {
    let io = io.unwrap_or_else(|| {
        std::rc::Rc::new(std::cell::RefCell::new(IOStub::new()))
            as std::rc::Rc<std::cell::RefCell<dyn IOInterface>>
    });
    let config = std::rc::Rc::new(std::cell::RefCell::new(setup_config(
        config.unwrap_or_else(|| ConfigStubBuilder::new().build()),
    )));
    GitDownloader::new(io, config, Some(process), filesystem)
}

#[serial_test::serial]
#[test]
fn test_download_for_package_without_source_reference() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());

    let package = get_package(BStr::new("dummy/pkg"), BStr::new("1.0.0"), None, None);

    let (process, _guard) = get_process_executor_mock(vec![], false, Default::default());
    let downloader = get_downloader_mock(None, None, process, None);

    let result = run(async {
        downloader
            .download(package.clone(), std::path::Path::new("/path"), None)
            .await?;
        downloader
            .prepare(
                BStr::new("install"),
                package.clone(),
                std::path::Path::new("/path"),
                None,
            )
            .await?;
        downloader
            .install(package.clone(), std::path::Path::new("/path"))
            .await?;
        downloader
            .cleanup(
                BStr::new("install"),
                package,
                std::path::Path::new("/path"),
                None,
            )
            .await
    });

    let e = result.expect_err("missing source reference should throw");
    assert!(error_message(&e).contains_str("missing reference information"));
}

#[serial_test::serial]
#[test]
fn test_download() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());

    let url = BStr::new("https://example.com/composer/composer");
    let package = get_package(
        BStr::new("composer/composer"),
        BStr::new("dev-master"),
        Some(BStr::new("1234567890123456789012345678901234567890")),
        Some(url),
    );

    let expected_path = working_dir.path().join("composerPath").to_php_string();

    let (process, _guard) = get_process_executor_mock(
        vec![
            cmd(vec![
                BStr::new("git"),
                BStr::new("clone"),
                BStr::new("--no-checkout"),
                BStr::new("--"),
                url,
                expected_path.as_bstr(),
            ]),
            cmd(vec![
                BStr::new("git"),
                BStr::new("remote"),
                BStr::new("add"),
                BStr::new("composer"),
                BStr::new("--"),
                url,
            ]),
            cmd(["git", "fetch", "composer"]),
            cmd(vec![
                BStr::new("git"),
                BStr::new("remote"),
                BStr::new("set-url"),
                BStr::new("origin"),
                BStr::new("--"),
                url,
            ]),
            cmd(vec![
                BStr::new("git"),
                BStr::new("remote"),
                BStr::new("set-url"),
                BStr::new("composer"),
                BStr::new("--"),
                url,
            ]),
            cmd(["git", "branch", "-r"]),
            cmd(["git", "checkout", "master", "--"]),
            cmd(vec![
                "git",
                "reset",
                "--hard",
                "1234567890123456789012345678901234567890",
                "--",
            ]),
        ],
        true,
        Default::default(),
    );

    let downloader = get_downloader_mock(None, None, process, None);

    run(async {
        downloader
            .download(package.clone(), path_from_bytes(&expected_path), None)
            .await
            .unwrap();
        downloader
            .prepare(
                BStr::new("install"),
                package.clone(),
                path_from_bytes(&expected_path),
                None,
            )
            .await
            .unwrap();
        downloader
            .install(package.clone(), path_from_bytes(&expected_path))
            .await
            .unwrap();
        downloader
            .cleanup(
                BStr::new("install"),
                package,
                path_from_bytes(&expected_path),
                None,
            )
            .await
            .unwrap();
    });
}

#[serial_test::serial]
#[test]
fn test_download_with_cache() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());

    let url = BStr::new("https://example.com/composer/composer");
    let package = get_package(
        BStr::new("composer/composer"),
        BStr::new("dev-master"),
        Some(BStr::new("1234567890123456789012345678901234567890")),
        Some(url),
    );

    // initGitVersion('2.17.0'): enables the cache (`--dissociate`) code path.
    GitUtil::__set_version(Some("2.17.0".into()));

    let config = setup_config(Config::new(false, None));
    let cache_vcs_dir = config
        .get("cache-vcs-dir")
        .unwrap()
        .as_string()
        .unwrap_or_default()
        .to_owned();
    // ref: GitDownloaderTest cachePath = cache-vcs-dir.'/'.preg_replace('{[^a-z0-9.]}i', '-', url).'/'
    let sanitized: BString = url
        .iter()
        .map(|&c| {
            if c.is_ascii_alphanumeric() || c == b'.' {
                c
            } else {
                b'-'
            }
        })
        .collect();
    let cache_path = bformat!("{cache_vcs_dir}/{sanitized}/");

    let mut filesystem = Filesystem::new(None);
    filesystem
        .remove_directory(path_from_bytes(&cache_path))
        .unwrap();

    let expected_path = working_dir.path().join("composerPath").to_php_string();

    let cache_path_for_callback = cache_path.clone();
    let mut clone_mirror = cmd(vec![
        BStr::new("git"),
        BStr::new("clone"),
        BStr::new("--mirror"),
        BStr::new("--"),
        url,
        cache_path.as_bstr(),
    ]);
    clone_mirror.callback = Some(Box::new(move || {
        std::fs::create_dir_all(path_from_bytes(&cache_path_for_callback)).ok();
    }));

    let (process, _guard) = get_process_executor_mock(
        vec![
            clone_mirror,
            cmd(["git", "remote", "-v"]),
            cmd(vec![
                BStr::new("git"),
                BStr::new("remote"),
                BStr::new("set-url"),
                BStr::new("origin"),
                BStr::new("--"),
                url,
            ]),
            cmd_full(["git", "rev-parse", "--git-dir"], 0, ".", ""),
            cmd(vec![
                "git",
                "rev-parse",
                "--quiet",
                "--verify",
                "1234567890123456789012345678901234567890^{commit}",
            ]),
            cmd(vec![
                BStr::new("git"),
                BStr::new("clone"),
                BStr::new("--no-checkout"),
                cache_path.as_bstr(),
                expected_path.as_bstr(),
                BStr::new("--dissociate"),
                BStr::new("--reference"),
                cache_path.as_bstr(),
            ]),
            cmd(vec![
                BStr::new("git"),
                BStr::new("remote"),
                BStr::new("set-url"),
                BStr::new("origin"),
                BStr::new("--"),
                url,
            ]),
            cmd(vec![
                BStr::new("git"),
                BStr::new("remote"),
                BStr::new("add"),
                BStr::new("composer"),
                BStr::new("--"),
                url,
            ]),
            cmd(["git", "branch", "-r"]),
            cmd_full(["git", "checkout", "master", "--"], 1, "", ""),
            cmd(vec![
                "git",
                "checkout",
                "-B",
                "master",
                "composer/master",
                "--",
            ]),
            cmd(vec![
                "git",
                "reset",
                "--hard",
                "1234567890123456789012345678901234567890",
                "--",
            ]),
        ],
        true,
        Default::default(),
    );

    let downloader = get_downloader_mock(None, Some(config), process, None);

    run(async {
        downloader
            .download(package.clone(), path_from_bytes(&expected_path), None)
            .await
            .unwrap();
        downloader
            .prepare(
                BStr::new("install"),
                package.clone(),
                path_from_bytes(&expected_path),
                None,
            )
            .await
            .unwrap();
        downloader
            .install(package.clone(), path_from_bytes(&expected_path))
            .await
            .unwrap();
        downloader
            .cleanup(
                BStr::new("install"),
                package,
                path_from_bytes(&expected_path),
                None,
            )
            .await
            .unwrap();
    });

    let mut fs = Filesystem::new(None);
    fs.remove_directory(path_from_bytes(&cache_path)).ok();
}

#[serial_test::serial]
#[test]
fn test_download_uses_various_protocols_and_sets_push_url_for_github() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());

    let package = get_package(
        BStr::new("composer/composer"),
        BStr::new("1.0.0"),
        Some(BStr::new("ref")),
        Some(BStr::new("https://github.com/composer/composer")),
    );
    // A preferred mirror with no `%...%` placeholders is passed through
    // ComposerMirror::process_git_url unchanged, so getSourceUrls() lists it (unmodified) ahead
    // of the source url, mirroring the PHP mock's
    // `willReturn(['https://github.com/mirrors/composer', 'https://github.com/composer/composer'])`.
    package.set_source_mirrors(Some(vec![Mirror {
        url: BString::from("https://github.com/mirrors/composer"),
        preferred: true,
    }]));

    let expected_path = working_dir.path().join("composerPath").to_php_string();

    let (process, _guard) = get_process_executor_mock(
        vec![
            cmd_full(
                vec![
                    BStr::new("git"),
                    BStr::new("clone"),
                    BStr::new("--no-checkout"),
                    BStr::new("--"),
                    BStr::new("https://github.com/mirrors/composer"),
                    expected_path.as_bstr(),
                ],
                1,
                "",
                "Error1",
            ),
            cmd(vec![
                BStr::new("git"),
                BStr::new("clone"),
                BStr::new("--no-checkout"),
                BStr::new("--"),
                BStr::new("git@github.com:mirrors/composer"),
                expected_path.as_bstr(),
            ]),
            cmd(vec![
                "git",
                "remote",
                "add",
                "composer",
                "--",
                "git@github.com:mirrors/composer",
            ]),
            cmd(["git", "fetch", "composer"]),
            cmd(vec![
                "git",
                "remote",
                "set-url",
                "origin",
                "--",
                "git@github.com:mirrors/composer",
            ]),
            cmd(vec![
                "git",
                "remote",
                "set-url",
                "composer",
                "--",
                "git@github.com:mirrors/composer",
            ]),
            cmd(vec![
                "git",
                "remote",
                "set-url",
                "origin",
                "--",
                "https://github.com/composer/composer",
            ]),
            cmd(vec![
                "git",
                "remote",
                "set-url",
                "--push",
                "origin",
                "--",
                "git@github.com:composer/composer.git",
            ]),
            cmd(["git", "branch", "-r"]),
            cmd(["git", "checkout", "ref", "--"]),
            cmd(["git", "reset", "--hard", "ref", "--"]),
        ],
        true,
        Default::default(),
    );

    let downloader = get_downloader_mock(None, None, process, None);

    run(async {
        downloader
            .download(package.clone(), path_from_bytes(&expected_path), None)
            .await
            .unwrap();
        downloader
            .prepare(
                BStr::new("install"),
                package.clone(),
                path_from_bytes(&expected_path),
                None,
            )
            .await
            .unwrap();
        downloader
            .install(package.clone(), path_from_bytes(&expected_path))
            .await
            .unwrap();
        downloader
            .cleanup(
                BStr::new("install"),
                package,
                path_from_bytes(&expected_path),
                None,
            )
            .await
            .unwrap();
    });
}

#[serial_test::serial]
#[test]
fn test_download_and_set_push_url_use_custom_various_protocols_for_github() {
    // ref: pushUrlProvider — (github-protocols, fetch url, push url).
    let cases: Vec<(Vec<&BStr>, &BStr, &BStr)> = vec![
        (
            vec![BStr::new("ssh")],
            BStr::new("git@github.com:composer/composer"),
            BStr::new("git@github.com:composer/composer.git"),
        ),
        (
            vec![BStr::new("https"), BStr::new("ssh"), BStr::new("git")],
            BStr::new("https://github.com/composer/composer"),
            BStr::new("git@github.com:composer/composer.git"),
        ),
        (
            vec![BStr::new("https")],
            BStr::new("https://github.com/composer/composer"),
            BStr::new("https://github.com/composer/composer.git"),
        ),
    ];

    for (protocols, url, push_url) in cases {
        let working_dir = set_up();
        let _tear_down = TearDown::new(working_dir.path().to_path_buf());

        let package = get_package(
            BStr::new("composer/composer"),
            BStr::new("1.0.0"),
            Some(BStr::new("ref")),
            Some(BStr::new("https://github.com/composer/composer")),
        );

        let expected_path = working_dir.path().join("composerPath").to_php_string();

        let (process, _guard) = get_process_executor_mock(
            vec![
                cmd(vec![
                    BStr::new("git"),
                    BStr::new("clone"),
                    BStr::new("--no-checkout"),
                    BStr::new("--"),
                    url,
                    expected_path.as_bstr(),
                ]),
                cmd(vec![
                    BStr::new("git"),
                    BStr::new("remote"),
                    BStr::new("add"),
                    BStr::new("composer"),
                    BStr::new("--"),
                    url,
                ]),
                cmd(["git", "fetch", "composer"]),
                cmd(vec![
                    BStr::new("git"),
                    BStr::new("remote"),
                    BStr::new("set-url"),
                    BStr::new("origin"),
                    BStr::new("--"),
                    url,
                ]),
                cmd(vec![
                    BStr::new("git"),
                    BStr::new("remote"),
                    BStr::new("set-url"),
                    BStr::new("composer"),
                    BStr::new("--"),
                    url,
                ]),
                cmd(vec![
                    BStr::new("git"),
                    BStr::new("remote"),
                    BStr::new("set-url"),
                    BStr::new("--push"),
                    BStr::new("origin"),
                    BStr::new("--"),
                    push_url,
                ]),
                cmd(["git", "branch", "-r"]),
                cmd(["git", "checkout", "ref", "--"]),
                cmd(["git", "reset", "--hard", "ref", "--"]),
            ],
            true,
            Default::default(),
        );

        let mut config = Config::new(false, None);
        let mut top: IndexMap<BString, PhpMixed> = IndexMap::new();
        let mut section: IndexMap<BString, PhpMixed> = IndexMap::new();
        section.insert(
            "github-protocols".into(),
            PhpMixed::List(
                protocols
                    .iter()
                    .map(|p| PhpMixed::string(p.to_php_string()))
                    .collect(),
            ),
        );
        top.insert("config".into(), PhpMixed::Array(section));
        config.merge(&top, Config::SOURCE_UNKNOWN);

        let downloader = get_downloader_mock(None, Some(config), process, None);

        run(async {
            downloader
                .download(package.clone(), path_from_bytes(&expected_path), None)
                .await
                .unwrap();
            downloader
                .prepare(
                    BStr::new("install"),
                    package.clone(),
                    path_from_bytes(&expected_path),
                    None,
                )
                .await
                .unwrap();
            downloader
                .install(package.clone(), path_from_bytes(&expected_path))
                .await
                .unwrap();
            downloader
                .cleanup(
                    BStr::new("install"),
                    package,
                    path_from_bytes(&expected_path),
                    None,
                )
                .await
                .unwrap();
        });
    }
}

#[serial_test::serial]
#[test]
fn test_download_throws_runtime_exception_if_git_command_fails() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());

    let url = BStr::new("https://example.com/composer/composer");
    let package = get_package(
        BStr::new("composer/composer"),
        BStr::new("1.0.0"),
        Some(BStr::new("ref")),
        Some(url),
    );

    let expected_path = working_dir.path().join("composerPath").to_php_string();

    let (process, _guard) = get_process_executor_mock(
        vec![cmd_full(
            vec![
                BStr::new("git"),
                BStr::new("clone"),
                BStr::new("--no-checkout"),
                BStr::new("--"),
                url,
                expected_path.as_bstr(),
            ],
            1,
            "",
            "",
        )],
        false,
        Default::default(),
    );

    let downloader = get_downloader_mock(None, None, process, None);

    let result = run(async {
        downloader
            .download(package.clone(), path_from_bytes(&expected_path), None)
            .await?;
        downloader
            .prepare(
                BStr::new("install"),
                package.clone(),
                path_from_bytes(&expected_path),
                None,
            )
            .await?;
        downloader
            .install(package.clone(), path_from_bytes(&expected_path))
            .await?;
        downloader
            .cleanup(
                BStr::new("install"),
                package,
                path_from_bytes(&expected_path),
                None,
            )
            .await
    });

    let e = result.expect_err("failed git clone should throw");
    assert!(error_message(&e).contains_str(bformat!(
        "Failed to execute git clone --no-checkout -- {url} {expected_path}"
    )));
}

#[serial_test::serial]
#[test]
fn test_updatefor_package_without_source_reference() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());

    let initial_package = get_package(
        BStr::new("dummy/pkg"),
        BStr::new("1.0.0"),
        Some(BStr::new("ref")),
        None,
    );
    let source_package = get_package(BStr::new("dummy/pkg"), BStr::new("1.0.0"), None, None);

    let (process, _guard) = get_process_executor_mock(vec![], false, Default::default());
    let downloader = get_downloader_mock(None, None, process, None);

    let result = run(async {
        downloader
            .download(
                source_package.clone(),
                std::path::Path::new("/path"),
                Some(initial_package.clone()),
            )
            .await?;
        downloader
            .prepare(
                BStr::new("update"),
                source_package.clone(),
                std::path::Path::new("/path"),
                Some(initial_package.clone()),
            )
            .await?;
        downloader
            .update(
                initial_package.clone(),
                source_package.clone(),
                std::path::Path::new("/path"),
            )
            .await?;
        downloader
            .cleanup(
                BStr::new("update"),
                source_package,
                std::path::Path::new("/path"),
                Some(initial_package),
            )
            .await
    });

    let e = result.expect_err("missing source reference should throw");
    assert!(error_message(&e).contains_str("missing reference information"));
}

#[serial_test::serial]
#[test]
fn test_update() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());

    let url = BStr::new("https://github.com/composer/composer");
    let package = get_package(
        BStr::new("composer/composer"),
        BStr::new("1.0.0"),
        Some(BStr::new("ref")),
        Some(url),
    );

    let (process, _guard) = get_process_executor_mock(
        vec![
            cmd(["git", "show-ref", "--head", "-d"]),
            cmd(["git", "status", "--porcelain", "--untracked-files=no"]),
            cmd_full(
                ["git", "rev-parse", "--quiet", "--verify", "ref^{commit}"],
                1,
                "",
                "",
            ),
            // fallback commands for the above failing
            cmd(["git", "remote", "-v"]),
            cmd(vec![
                BStr::new("git"),
                BStr::new("remote"),
                BStr::new("set-url"),
                BStr::new("composer"),
                BStr::new("--"),
                url,
            ]),
            cmd(["git", "fetch", "composer"]),
            cmd(["git", "fetch", "--tags", "composer"]),
            cmd(["git", "remote", "-v"]),
            cmd(vec![
                BStr::new("git"),
                BStr::new("remote"),
                BStr::new("set-url"),
                BStr::new("composer"),
                BStr::new("--"),
                url,
            ]),
            cmd(["git", "branch", "-r"]),
            cmd(["git", "checkout", "ref", "--"]),
            cmd(["git", "reset", "--hard", "ref", "--"]),
            cmd(["git", "remote", "-v"]),
        ],
        true,
        Default::default(),
    );

    let mut fs = Filesystem::new(None);
    fs.ensure_directory_exists(working_dir.path().join(".git"))
        .unwrap();
    let working_dir_str = working_dir.path().to_path_buf();

    let downloader = get_downloader_mock(None, Some(Config::new(false, None)), process, None);

    run(async {
        downloader
            .download(
                package.clone(),
                std::path::Path::new(&working_dir_str),
                Some(package.clone()),
            )
            .await
            .unwrap();
        downloader
            .prepare(
                BStr::new("update"),
                package.clone(),
                std::path::Path::new(&working_dir_str),
                Some(package.clone()),
            )
            .await
            .unwrap();
        downloader
            .update(
                package.clone(),
                package.clone(),
                std::path::Path::new(&working_dir_str),
            )
            .await
            .unwrap();
        downloader
            .cleanup(
                BStr::new("update"),
                package.clone(),
                std::path::Path::new(&working_dir_str),
                Some(package),
            )
            .await
            .unwrap();
    });
}

#[serial_test::serial]
#[test]
fn test_update_with_new_repo_url() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());

    let url = BStr::new("https://github.com/composer/composer");
    let package = get_package(
        BStr::new("composer/composer"),
        BStr::new("1.0.0"),
        Some(BStr::new("ref")),
        Some(url),
    );

    let (process, _guard) = get_process_executor_mock(
        vec![
            cmd(["git", "show-ref", "--head", "-d"]),
            cmd(["git", "status", "--porcelain", "--untracked-files=no"]),
            cmd_full(
                ["git", "rev-parse", "--quiet", "--verify", "ref^{commit}"],
                0,
                "",
                "",
            ),
            cmd(["git", "remote", "-v"]),
            cmd(vec![
                BStr::new("git"),
                BStr::new("remote"),
                BStr::new("set-url"),
                BStr::new("composer"),
                BStr::new("--"),
                url,
            ]),
            cmd(["git", "branch", "-r"]),
            cmd(["git", "checkout", "ref", "--"]),
            cmd(["git", "reset", "--hard", "ref", "--"]),
            cmd_full(
                ["git", "remote", "-v"],
                0,
                "origin https://github.com/old/url (fetch)\n\
                 origin https://github.com/old/url (push)\n\
                 composer https://github.com/old/url (fetch)\n\
                 composer https://github.com/old/url (push)\n",
                "",
            ),
            cmd(vec![
                BStr::new("git"),
                BStr::new("remote"),
                BStr::new("set-url"),
                BStr::new("origin"),
                BStr::new("--"),
                url,
            ]),
            cmd(vec![
                "git",
                "remote",
                "set-url",
                "--push",
                "origin",
                "--",
                "git@github.com:composer/composer.git",
            ]),
        ],
        true,
        Default::default(),
    );

    let mut fs = Filesystem::new(None);
    fs.ensure_directory_exists(working_dir.path().join(".git"))
        .unwrap();
    let working_dir_str = working_dir.path().to_path_buf();

    let downloader = get_downloader_mock(None, Some(Config::new(false, None)), process, None);

    run(async {
        downloader
            .download(
                package.clone(),
                std::path::Path::new(&working_dir_str),
                Some(package.clone()),
            )
            .await
            .unwrap();
        downloader
            .prepare(
                BStr::new("update"),
                package.clone(),
                std::path::Path::new(&working_dir_str),
                Some(package.clone()),
            )
            .await
            .unwrap();
        downloader
            .update(
                package.clone(),
                package.clone(),
                std::path::Path::new(&working_dir_str),
            )
            .await
            .unwrap();
        downloader
            .cleanup(
                BStr::new("update"),
                package.clone(),
                std::path::Path::new(&working_dir_str),
                Some(package),
            )
            .await
            .unwrap();
    });
}

#[serial_test::serial]
#[test]
fn test_update_throws_runtime_exception_if_git_command_fails() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());

    let url = BStr::new("https://github.com/composer/composer");
    let package = get_package(
        BStr::new("composer/composer"),
        BStr::new("1.0.0"),
        Some(BStr::new("ref")),
        Some(url),
    );

    let (process, _guard) = get_process_executor_mock(
        vec![
            cmd(["git", "show-ref", "--head", "-d"]),
            cmd(["git", "status", "--porcelain", "--untracked-files=no"]),
            // commit not yet in so we try to fetch
            cmd_full(
                ["git", "rev-parse", "--quiet", "--verify", "ref^{commit}"],
                1,
                "",
                "",
            ),
            // fail first fetch
            cmd(["git", "remote", "-v"]),
            cmd(vec![
                BStr::new("git"),
                BStr::new("remote"),
                BStr::new("set-url"),
                BStr::new("composer"),
                BStr::new("--"),
                url,
            ]),
            cmd_full(["git", "fetch", "composer"], 1, "", ""),
            // fail second fetch
            cmd(vec![
                "git",
                "remote",
                "set-url",
                "composer",
                "--",
                "git@github.com:composer/composer",
            ]),
            cmd_full(["git", "fetch", "composer"], 1, "", ""),
            cmd(["git", "--version"]),
        ],
        true,
        Default::default(),
    );

    let mut fs = Filesystem::new(None);
    fs.ensure_directory_exists(working_dir.path().join(".git"))
        .unwrap();
    let working_dir_str = working_dir.path().to_path_buf();

    let config = Config::new(false, None);

    let downloader = get_downloader_mock(None, Some(config), process, None);

    let result = run(async {
        downloader
            .download(
                package.clone(),
                std::path::Path::new(&working_dir_str),
                Some(package.clone()),
            )
            .await?;
        downloader
            .prepare(
                BStr::new("update"),
                package.clone(),
                std::path::Path::new(&working_dir_str),
                Some(package.clone()),
            )
            .await?;
        downloader
            .update(
                package.clone(),
                package.clone(),
                std::path::Path::new(&working_dir_str),
            )
            .await?;
        downloader
            .cleanup(
                BStr::new("update"),
                package.clone(),
                std::path::Path::new(&working_dir_str),
                Some(package),
            )
            .await
    });

    let e = result.expect_err("failing git fetch should throw");
    assert!(error_message(&e).contains_str(
        "Failed to clone https://github.com/composer/composer via https, ssh protocols, aborting."
    ));
    assert!(error_message(&e).contains_str("git@github.com:composer/composer"));
}

#[serial_test::serial]
#[test]
fn test_update_doesnt_throws_runtime_exception_if_git_command_fails_at_first_but_is_able_to_recover()
 {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());

    let package = get_package(
        BStr::new("composer/composer"),
        BStr::new("1.0.0"),
        Some(BStr::new("ref")),
        Some(BStr::new("/")),
    );
    // A non-preferred mirror is appended after the source url, mirroring the PHP mock's
    // `willReturn(['/', 'https://github.com/composer/composer'])`: the first ("/") url fails,
    // and VcsDownloader::update retries with the second (the mirror).
    package.set_source_mirrors(Some(vec![Mirror {
        url: BString::from("https://github.com/composer/composer"),
        preferred: false,
    }]));

    let (process, _guard) = get_process_executor_mock(
        vec![
            cmd(["git", "show-ref", "--head", "-d"]),
            cmd(["git", "status", "--porcelain", "--untracked-files=no"]),
            // commit not yet in so we try to fetch
            cmd_full(
                ["git", "rev-parse", "--quiet", "--verify", "ref^{commit}"],
                1,
                "",
                "",
            ),
            // fail first source URL
            cmd(["git", "remote", "-v"]),
            cmd(vec!["git", "remote", "set-url", "composer", "--", "/"]),
            cmd_full(["git", "fetch", "composer"], 1, "", ""),
            cmd(["git", "--version"]),
            // commit not yet in so we try to fetch
            cmd_full(
                ["git", "rev-parse", "--quiet", "--verify", "ref^{commit}"],
                1,
                "",
                "",
            ),
            // pass second source URL
            cmd(["git", "remote", "-v"]),
            cmd(vec![
                "git",
                "remote",
                "set-url",
                "composer",
                "--",
                "https://github.com/composer/composer",
            ]),
            cmd(["git", "fetch", "composer"]),
            cmd(["git", "fetch", "--tags", "composer"]),
            cmd(["git", "remote", "-v"]),
            cmd(vec![
                "git",
                "remote",
                "set-url",
                "composer",
                "--",
                "https://github.com/composer/composer",
            ]),
            cmd(["git", "branch", "-r"]),
            cmd(["git", "checkout", "ref", "--"]),
            cmd(["git", "reset", "--hard", "ref", "--"]),
            cmd(["git", "remote", "-v"]),
        ],
        true,
        Default::default(),
    );

    let mut fs = Filesystem::new(None);
    fs.ensure_directory_exists(working_dir.path().join(".git"))
        .unwrap();
    let working_dir_str = working_dir.path().to_path_buf();

    let downloader = get_downloader_mock(None, Some(Config::new(false, None)), process, None);

    run(async {
        downloader
            .download(
                package.clone(),
                std::path::Path::new(&working_dir_str),
                Some(package.clone()),
            )
            .await
            .unwrap();
        downloader
            .prepare(
                BStr::new("update"),
                package.clone(),
                std::path::Path::new(&working_dir_str),
                Some(package.clone()),
            )
            .await
            .unwrap();
        downloader
            .update(
                package.clone(),
                package.clone(),
                std::path::Path::new(&working_dir_str),
            )
            .await
            .unwrap();
        downloader
            .cleanup(
                BStr::new("update"),
                package.clone(),
                std::path::Path::new(&working_dir_str),
                Some(package),
            )
            .await
            .unwrap();
    });
}

#[serial_test::serial]
#[test]
fn test_downgrade_shows_appropriate_message() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());

    let url = BStr::new("https://github.com/composer/composer");

    let old_package = get_package(
        BStr::new("composer/composer"),
        BStr::new("1.2.0"),
        Some(BStr::new("ref")),
        Some(BStr::new("/foo/bar")),
    );
    old_package.set_source_mirrors(Some(vec![Mirror {
        url: url.to_php_string(),
        preferred: false,
    }]));
    let new_package = get_package(
        BStr::new("composer/composer"),
        BStr::new("1.0.0"),
        Some(BStr::new("ref")),
        Some(url),
    );

    let (process, _guard) = get_process_executor_mock(vec![], false, Default::default());

    let (io_mock, _io_guard) = get_io_mock(io_interface::NORMAL).unwrap();
    io_mock
        .borrow_mut()
        .expects(vec![Expectation::text_regex("{Downgrading .*}")], false)
        .unwrap();
    let io = io_mock as std::rc::Rc<std::cell::RefCell<dyn IOInterface>>;

    let mut fs = Filesystem::new(None);
    fs.ensure_directory_exists(working_dir.path().join(".git"))
        .unwrap();
    let working_dir_str = working_dir.path().to_path_buf();

    let downloader = get_downloader_mock(Some(io), None, process, None);

    run(async {
        downloader
            .download(
                new_package.clone(),
                std::path::Path::new(&working_dir_str),
                Some(old_package.clone()),
            )
            .await
            .unwrap();
        downloader
            .prepare(
                BStr::new("update"),
                new_package.clone(),
                std::path::Path::new(&working_dir_str),
                Some(old_package.clone()),
            )
            .await
            .unwrap();
        downloader
            .update(
                old_package.clone(),
                new_package.clone(),
                std::path::Path::new(&working_dir_str),
            )
            .await
            .unwrap();
        downloader
            .cleanup(
                BStr::new("update"),
                new_package,
                std::path::Path::new(&working_dir_str),
                Some(old_package),
            )
            .await
            .unwrap();
    });
}

#[serial_test::serial]
#[test]
fn test_not_using_downgrading_with_references() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());

    let url = "https://github.com/composer/composer";

    // dev versions: getVersion() is the (non-normalized) branch name.
    let old_package = CompletePackageHandle::new(
        BString::from("composer/composer"),
        BString::from("dev-ref"),
        BString::from("dev-ref"),
    );
    old_package.__set_source_type(Some(BString::from("git")));
    old_package.set_source_reference(Some(BString::from("ref")));
    old_package.set_source_url(Some(BString::from("/foo/bar")));
    old_package.set_source_mirrors(Some(vec![Mirror {
        url: url.to_php_string(),
        preferred: false,
    }]));
    let old_package: PackageInterfaceHandle = old_package.into();

    let new_package = CompletePackageHandle::new(
        BString::from("composer/composer"),
        BString::from("dev-ref2"),
        BString::from("dev-ref2"),
    );
    new_package.__set_source_type(Some(BString::from("git")));
    new_package.set_source_reference(Some(BString::from("ref")));
    new_package.set_source_url(Some(url.to_php_string()));
    let new_package: PackageInterfaceHandle = new_package.into();

    let (process, _guard) = get_process_executor_mock(vec![], false, Default::default());

    let (io_mock, _io_guard) = get_io_mock(io_interface::NORMAL).unwrap();
    io_mock
        .borrow_mut()
        .expects(vec![Expectation::text_regex("{Upgrading .*}")], false)
        .unwrap();
    let io = io_mock as std::rc::Rc<std::cell::RefCell<dyn IOInterface>>;

    let mut fs = Filesystem::new(None);
    fs.ensure_directory_exists(working_dir.path().join(".git"))
        .unwrap();
    let working_dir_str = working_dir.path().to_path_buf();

    let downloader = get_downloader_mock(Some(io), None, process, None);

    run(async {
        downloader
            .download(
                new_package.clone(),
                std::path::Path::new(&working_dir_str),
                Some(old_package.clone()),
            )
            .await
            .unwrap();
        downloader
            .prepare(
                BStr::new("update"),
                new_package.clone(),
                std::path::Path::new(&working_dir_str),
                Some(old_package.clone()),
            )
            .await
            .unwrap();
        downloader
            .update(
                old_package.clone(),
                new_package.clone(),
                std::path::Path::new(&working_dir_str),
            )
            .await
            .unwrap();
        downloader
            .cleanup(
                BStr::new("update"),
                new_package,
                std::path::Path::new(&working_dir_str),
                Some(old_package),
            )
            .await
            .unwrap();
    });
}

#[serial_test::serial]
#[test]
fn test_remove() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let working_dir_str = working_dir.path().to_path_buf();

    let package = get_package(BStr::new("dummy/pkg"), BStr::new("1.0.0"), None, None);
    let (process, _guard) = get_process_executor_mock(
        vec![
            cmd(vec!["git", "show-ref", "--head", "-d"]),
            cmd(vec!["git", "status", "--porcelain", "--untracked-files=no"]),
        ],
        true,
        Default::default(),
    );

    Filesystem::new(None)
        .ensure_directory_exists(working_dir.path().join(".git"))
        .unwrap();

    let mut filesystem = Filesystem::new(None);
    filesystem.__set_mock(FilesystemMock {
        remove_directory_async_result: Some(true),
        ..Default::default()
    });
    let filesystem = std::rc::Rc::new(std::cell::RefCell::new(filesystem));

    let downloader = get_downloader_mock(None, None, process, Some(filesystem.clone()));
    run(async {
        downloader
            .prepare(
                BStr::new("uninstall"),
                package.clone(),
                std::path::Path::new(&working_dir_str),
                None,
            )
            .await
            .unwrap();
        downloader
            .remove(package.clone(), std::path::Path::new(&working_dir_str))
            .await
            .unwrap();
        downloader
            .cleanup(
                BStr::new("uninstall"),
                package,
                std::path::Path::new(&working_dir_str),
                None,
            )
            .await
            .unwrap();
    });

    assert_eq!(
        filesystem.borrow().__remove_directory_async_paths(),
        vec![working_dir_str]
    );
}

#[serial_test::serial]
#[test]
fn test_get_installation_source() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;

    let (process, _guard) = get_process_executor_mock(vec![], false, Default::default());
    let downloader = get_downloader_mock(None, None, process, None);

    assert_eq!("source", downloader.get_installation_source());
}
