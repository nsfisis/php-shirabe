//! ref: composer/tests/Composer/Test/Downloader/GitDownloaderTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use serial_test::serial;
use shirabe::config::Config;
use shirabe::downloader::VcsDownloader;
use shirabe::downloader::git_downloader::GitDownloader;
use shirabe::io::IOInterface;
use shirabe::package::handle::{CompletePackageHandle, PackageInterfaceHandle};
use shirabe::util::Git as GitUtil;
use shirabe::util::ProcessExecutor;
use shirabe::util::filesystem::Filesystem;
use shirabe_php_shim::PhpMixed;
use shirabe_semver::VersionParser;
use tempfile::TempDir;

use crate::config_stub::ConfigStubBuilder;
use crate::io_stub::IOStub;
use crate::process_executor_mock::{cmd, cmd_full, get_process_executor_mock};

fn run<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(future)
}

fn set_up() -> TempDir {
    // skipIfNotExecutable('git') is irrelevant because every git invocation is mocked.

    // initGitVersion('1.0.0'): seed the Composer\Util\Git static version cache.
    GitUtil::__set_version(Some("1.0.0".to_string()));

    TempDir::new().unwrap()
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
    name: &str,
    pretty_version: &str,
    source_reference: Option<&str>,
    source_url: Option<&str>,
) -> PackageInterfaceHandle {
    let norm_version = VersionParser.normalize(pretty_version, None).unwrap();
    let package =
        CompletePackageHandle::new(name.to_string(), norm_version, pretty_version.to_string());
    package.__set_source_type(Some("git".to_string()));
    package.set_source_reference(source_reference.map(|s| s.to_string()));
    package.set_source_url(source_url.map(|s| s.to_string()));
    package.into()
}

/// ref: GitDownloaderTest::setupConfig (seeds a temp `home` when none is set)
fn setup_config(config: Config) -> Config {
    let mut config = config;
    if !config.has("home") {
        let tmp_dir = TempDir::new().unwrap().keep();
        let mut top: IndexMap<String, PhpMixed> = IndexMap::new();
        let mut section: IndexMap<String, PhpMixed> = IndexMap::new();
        section.insert(
            "home".to_string(),
            PhpMixed::String(tmp_dir.to_string_lossy().into_owned()),
        );
        top.insert("config".to_string(), PhpMixed::Array(section));
        config.merge(&top, Config::SOURCE_UNKNOWN);
    }
    config
}

/// ref: GitDownloaderTest::getDownloaderMock (defaults the IO/Config/Filesystem)
fn get_downloader_mock(
    io: Option<Rc<RefCell<dyn IOInterface>>>,
    config: Option<Config>,
    process: Rc<RefCell<ProcessExecutor>>,
    filesystem: Option<Rc<RefCell<Filesystem>>>,
) -> GitDownloader {
    let io =
        io.unwrap_or_else(|| Rc::new(RefCell::new(IOStub::new())) as Rc<RefCell<dyn IOInterface>>);
    let config = Rc::new(RefCell::new(setup_config(
        config.unwrap_or_else(|| ConfigStubBuilder::new().build()),
    )));
    GitDownloader::new(io, config, Some(process), filesystem)
}

#[serial]
#[test]
fn test_download_for_package_without_source_reference() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());

    let package = get_package("dummy/pkg", "1.0.0", None, None);

    let (process, _guard) = get_process_executor_mock(vec![], false, Default::default());
    let mut downloader = get_downloader_mock(None, None, process, None);

    let result = run(async {
        downloader.download(package.clone(), "/path", None).await?;
        downloader
            .prepare("install", package.clone(), "/path", None)
            .await?;
        downloader.install(package.clone(), "/path").await?;
        downloader.cleanup("install", package, "/path", None).await
    });

    let e = result.expect_err("missing source reference should throw");
    assert!(e.to_string().contains("missing reference information"));
}

#[serial]
#[test]
fn test_download() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());

    let url = "https://example.com/composer/composer";
    let package = get_package(
        "composer/composer",
        "dev-master",
        Some("1234567890123456789012345678901234567890"),
        Some(url),
    );

    let expected_path = working_dir
        .path()
        .join("composerPath")
        .to_string_lossy()
        .into_owned();

    let (process, _guard) = get_process_executor_mock(
        vec![
            cmd(vec![
                "git",
                "clone",
                "--no-checkout",
                "--",
                url,
                &expected_path,
            ]),
            cmd(vec!["git", "remote", "add", "composer", "--", url]),
            cmd(["git", "fetch", "composer"]),
            cmd(vec!["git", "remote", "set-url", "origin", "--", url]),
            cmd(vec!["git", "remote", "set-url", "composer", "--", url]),
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

    let mut downloader = get_downloader_mock(None, None, process, None);

    run(async {
        downloader
            .download(package.clone(), &expected_path, None)
            .await
            .unwrap();
        downloader
            .prepare("install", package.clone(), &expected_path, None)
            .await
            .unwrap();
        downloader
            .install(package.clone(), &expected_path)
            .await
            .unwrap();
        downloader
            .cleanup("install", package, &expected_path, None)
            .await
            .unwrap();
    });
}

#[serial]
#[test]
fn test_download_with_cache() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());

    let url = "https://example.com/composer/composer";
    let package = get_package(
        "composer/composer",
        "dev-master",
        Some("1234567890123456789012345678901234567890"),
        Some(url),
    );

    // initGitVersion('2.17.0'): enables the cache (`--dissociate`) code path.
    GitUtil::__set_version(Some("2.17.0".to_string()));

    let config = setup_config(Config::new(false, None));
    let cache_vcs_dir = config
        .get("cache-vcs-dir")
        .as_string()
        .unwrap_or("")
        .to_string();
    // ref: GitDownloaderTest cachePath = cache-vcs-dir.'/'.preg_replace('{[^a-z0-9.]}i', '-', url).'/'
    let sanitized: String = url
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let cache_path = format!("{}/{}/", cache_vcs_dir, sanitized);

    let mut filesystem = Filesystem::new(None);
    filesystem.remove_directory(&cache_path).unwrap();

    let expected_path = working_dir
        .path()
        .join("composerPath")
        .to_string_lossy()
        .into_owned();

    let cache_path_for_callback = cache_path.clone();
    let mut clone_mirror = cmd(vec!["git", "clone", "--mirror", "--", url, &cache_path]);
    clone_mirror.callback = Some(Box::new(move || {
        std::fs::create_dir_all(&cache_path_for_callback).ok();
    }));

    let (process, _guard) = get_process_executor_mock(
        vec![
            clone_mirror,
            cmd(["git", "remote", "-v"]),
            cmd(vec!["git", "remote", "set-url", "origin", "--", url]),
            cmd_full(["git", "rev-parse", "--git-dir"], 0, ".", ""),
            cmd(vec![
                "git",
                "rev-parse",
                "--quiet",
                "--verify",
                "1234567890123456789012345678901234567890^{commit}",
            ]),
            cmd(vec![
                "git",
                "clone",
                "--no-checkout",
                &cache_path,
                &expected_path,
                "--dissociate",
                "--reference",
                &cache_path,
            ]),
            cmd(vec!["git", "remote", "set-url", "origin", "--", url]),
            cmd(vec!["git", "remote", "add", "composer", "--", url]),
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

    let mut downloader = get_downloader_mock(None, Some(config), process, None);

    run(async {
        downloader
            .download(package.clone(), &expected_path, None)
            .await
            .unwrap();
        downloader
            .prepare("install", package.clone(), &expected_path, None)
            .await
            .unwrap();
        downloader
            .install(package.clone(), &expected_path)
            .await
            .unwrap();
        downloader
            .cleanup("install", package, &expected_path, None)
            .await
            .unwrap();
    });

    let mut fs = Filesystem::new(None);
    fs.remove_directory(&cache_path).ok();
}

#[ignore = "getSourceUrls returns a mirror list (['https://github.com/mirrors/composer', \
            'https://github.com/composer/composer']) that a real CompletePackage cannot reproduce \
            from a single source_url; no PackageInterface mock with arbitrary getSourceUrls exists"]
#[test]
fn test_download_uses_various_protocols_and_sets_push_url_for_github() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}

#[ignore = "pushUrlProvider configures getSourceUrls to a list a real CompletePackage cannot \
            reproduce; no PackageInterface mock with arbitrary getSourceUrls exists"]
#[test]
fn test_download_and_set_push_url_use_custom_various_protocols_for_github() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}

#[serial]
#[test]
fn test_download_throws_runtime_exception_if_git_command_fails() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());

    let url = "https://example.com/composer/composer";
    let package = get_package("composer/composer", "1.0.0", Some("ref"), Some(url));

    let expected_path = working_dir
        .path()
        .join("composerPath")
        .to_string_lossy()
        .into_owned();

    let (process, _guard) = get_process_executor_mock(
        vec![cmd_full(
            vec!["git", "clone", "--no-checkout", "--", url, &expected_path],
            1,
            "",
            "",
        )],
        false,
        Default::default(),
    );

    let mut downloader = get_downloader_mock(None, None, process, None);

    let result = run(async {
        downloader
            .download(package.clone(), &expected_path, None)
            .await?;
        downloader
            .prepare("install", package.clone(), &expected_path, None)
            .await?;
        downloader.install(package.clone(), &expected_path).await?;
        downloader
            .cleanup("install", package, &expected_path, None)
            .await
    });

    let e = result.expect_err("failed git clone should throw");
    assert!(e.to_string().contains(&format!(
        "Failed to execute git clone --no-checkout -- {} {}",
        url, expected_path
    )));
}

#[serial]
#[test]
fn test_updatefor_package_without_source_reference() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());

    let initial_package = get_package("dummy/pkg", "1.0.0", Some("ref"), None);
    let source_package = get_package("dummy/pkg", "1.0.0", None, None);

    let (process, _guard) = get_process_executor_mock(vec![], false, Default::default());
    let mut downloader = get_downloader_mock(None, None, process, None);

    let result = run(async {
        downloader
            .download(
                source_package.clone(),
                "/path",
                Some(initial_package.clone()),
            )
            .await?;
        downloader
            .prepare(
                "update",
                source_package.clone(),
                "/path",
                Some(initial_package.clone()),
            )
            .await?;
        downloader
            .update(initial_package.clone(), source_package.clone(), "/path")
            .await?;
        downloader
            .cleanup("update", source_package, "/path", Some(initial_package))
            .await
    });

    let e = result.expect_err("missing source reference should throw");
    assert!(e.to_string().contains("missing reference information"));
}

#[serial]
#[test]
fn test_update() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());

    let url = "https://github.com/composer/composer";
    let package = get_package("composer/composer", "1.0.0", Some("ref"), Some(url));

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
            cmd(vec!["git", "remote", "set-url", "composer", "--", url]),
            cmd(["git", "fetch", "composer"]),
            cmd(["git", "fetch", "--tags", "composer"]),
            cmd(["git", "remote", "-v"]),
            cmd(vec!["git", "remote", "set-url", "composer", "--", url]),
            cmd(["git", "branch", "-r"]),
            cmd(["git", "checkout", "ref", "--"]),
            cmd(["git", "reset", "--hard", "ref", "--"]),
            cmd(["git", "remote", "-v"]),
        ],
        true,
        Default::default(),
    );

    let mut fs = Filesystem::new(None);
    fs.ensure_directory_exists(&format!("{}/.git", working_dir.path().to_string_lossy()))
        .unwrap();
    let working_dir_str = working_dir.path().to_string_lossy().into_owned();

    let mut downloader = get_downloader_mock(None, Some(Config::new(false, None)), process, None);

    run(async {
        downloader
            .download(package.clone(), &working_dir_str, Some(package.clone()))
            .await
            .unwrap();
        downloader
            .prepare(
                "update",
                package.clone(),
                &working_dir_str,
                Some(package.clone()),
            )
            .await
            .unwrap();
        downloader
            .update(package.clone(), package.clone(), &working_dir_str)
            .await
            .unwrap();
        downloader
            .cleanup("update", package.clone(), &working_dir_str, Some(package))
            .await
            .unwrap();
    });
}

#[serial]
#[test]
fn test_update_with_new_repo_url() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());

    let url = "https://github.com/composer/composer";
    let package = get_package("composer/composer", "1.0.0", Some("ref"), Some(url));

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
            cmd(vec!["git", "remote", "set-url", "composer", "--", url]),
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
            cmd(vec!["git", "remote", "set-url", "origin", "--", url]),
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
    fs.ensure_directory_exists(&format!("{}/.git", working_dir.path().to_string_lossy()))
        .unwrap();
    let working_dir_str = working_dir.path().to_string_lossy().into_owned();

    let mut downloader = get_downloader_mock(None, Some(Config::new(false, None)), process, None);

    run(async {
        downloader
            .download(package.clone(), &working_dir_str, Some(package.clone()))
            .await
            .unwrap();
        downloader
            .prepare(
                "update",
                package.clone(),
                &working_dir_str,
                Some(package.clone()),
            )
            .await
            .unwrap();
        downloader
            .update(package.clone(), package.clone(), &working_dir_str)
            .await
            .unwrap();
        downloader
            .cleanup("update", package.clone(), &working_dir_str, Some(package))
            .await
            .unwrap();
    });
}

#[ignore = "getSourceUrls returns a multi-URL list a real CompletePackage cannot reproduce; \
            no PackageInterface mock with arbitrary getSourceUrls exists"]
#[test]
fn test_update_throws_runtime_exception_if_git_command_fails() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}

#[ignore = "getSourceUrls returns a multi-URL list (['/' , github]) a real CompletePackage cannot \
            reproduce; no PackageInterface mock with arbitrary getSourceUrls exists"]
#[test]
fn test_update_doesnt_throws_runtime_exception_if_git_command_fails_at_first_but_is_able_to_recover()
 {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}

#[ignore = "getSourceUrls returns a multi-URL list (['/foo/bar', github]) a real CompletePackage \
            cannot reproduce; no PackageInterface mock with arbitrary getSourceUrls exists"]
#[test]
fn test_downgrade_shows_appropriate_message() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}

#[ignore = "getSourceUrls returns a multi-URL list (['/foo/bar', github]) a real CompletePackage \
            cannot reproduce; no PackageInterface mock with arbitrary getSourceUrls exists"]
#[test]
fn test_not_using_downgrading_with_references() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}

#[ignore = "PHP mocks Filesystem::removeDirectoryAsync (asserting it is called once with the \
            working dir). With no Filesystem mock, the real removeDirectoryAsync drives the \
            Filesystem's own ProcessExecutor for `rm -rf`, which requires a Composer\\Loop and \
            cannot be redirected through the mocked ProcessExecutor"]
#[test]
fn test_remove() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}

#[serial]
#[test]
fn test_get_installation_source() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;

    let (process, _guard) = get_process_executor_mock(vec![], false, Default::default());
    let downloader = get_downloader_mock(None, None, process, None);

    assert_eq!("source", downloader.get_installation_source());
}
