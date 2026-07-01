//! ref: composer/tests/Composer/Test/Repository/VcsRepositoryTest.php

use indexmap::{IndexMap, IndexSet};
use serial_test::serial;
use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::VcsRepository;
use shirabe::util::ProcessExecutor;
use shirabe::util::filesystem::Filesystem;
use shirabe::util::http_downloader::HttpDownloader;
use shirabe::util::r#loop::Loop;
use shirabe_php_shim::PhpMixed;
use std::cell::RefCell;
use std::rc::Rc;
use tempfile::TempDir;

struct SetUp {
    composer_home: TempDir,
    git_repo: TempDir,
}

// ref: VcsRepositoryTest::initialize. Builds a fixture git repository on disk by shelling out to
// git. Returns None when git is unavailable (mirroring markTestSkipped).
fn set_up() -> Option<SetUp> {
    which_git()?;

    let composer_home = TempDir::new().unwrap();
    let git_repo = TempDir::new().unwrap();
    let path = git_repo.path();

    let exec = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(path)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .status()
            .unwrap_or_else(|e| panic!("Failed to execute git {:?}: {}", args, e));
        if !status.success() {
            panic!("Failed to execute git {:?}", args);
        }
    };
    let write_file = |name: &str, contents: &str| {
        std::fs::write(path.join(name), contents).unwrap();
    };

    // init
    exec(&["init", "-q"]);
    exec(&["checkout", "-b", "master"]);
    exec(&["config", "user.email", "composertest@example.org"]);
    exec(&["config", "user.name", "ComposerTest"]);
    exec(&["config", "commit.gpgsign", "false"]);
    write_file("foo", "");
    exec(&["add", "foo"]);
    exec(&["commit", "-m", "init"]);

    // non-composed tag & branch
    exec(&["tag", "0.5.0"]);
    exec(&["branch", "oldbranch"]);

    // add composed tag & master branch
    write_file(
        "composer.json",
        &shirabe::json::JsonFile::encode(&composer(None)),
    );
    exec(&["add", "composer.json"]);
    exec(&["commit", "-m", "addcomposer"]);
    exec(&["tag", "0.6.0"]);

    // add feature-a branch
    exec(&["checkout", "-b", "feature/a-1.0-B"]);
    write_file("foo", "bar feature");
    exec(&["add", "foo"]);
    exec(&["commit", "-m", "change-a"]);

    // add foo#bar branch which should result in dev-foo+bar
    exec(&["branch", "foo#bar"]);

    // add version to composer.json
    exec(&["checkout", "master"]);
    write_file(
        "composer.json",
        &shirabe::json::JsonFile::encode(&composer(Some("1.0.0"))),
    );
    exec(&["add", "composer.json"]);
    exec(&["commit", "-m", "addversion"]);

    // create tag with wrong version in it
    exec(&["tag", "0.9.0"]);
    // create tag with correct version in it
    exec(&["tag", "1.0.0"]);

    // add feature-b branch
    exec(&["checkout", "-b", "feature-b"]);
    write_file("foo", "baz feature");
    exec(&["add", "foo"]);
    exec(&["commit", "-m", "change-b"]);

    // add 1.0 branch
    exec(&["checkout", "master"]);
    exec(&["branch", "1.0"]);

    // add 1.0.x branch
    exec(&["branch", "1.1.x"]);

    // update master to 2.0
    write_file(
        "composer.json",
        &shirabe::json::JsonFile::encode(&composer(Some("2.0.0"))),
    );
    exec(&["add", "composer.json"]);
    exec(&["commit", "-m", "bump-version"]);

    Some(SetUp {
        composer_home,
        git_repo,
    })
}

fn composer(version: Option<&str>) -> PhpMixed {
    let mut m: IndexMap<String, PhpMixed> = IndexMap::new();
    m.insert("name".to_string(), PhpMixed::String("a/b".to_string()));
    if let Some(version) = version {
        m.insert("version".to_string(), PhpMixed::String(version.to_string()));
    }
    PhpMixed::Array(m)
}

fn which_git() -> Option<()> {
    std::process::Command::new("git")
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|_| ())
}

fn tear_down(composer_home: &std::path::Path, git_repo: &std::path::Path) {
    let mut fs = Filesystem::new(None);
    fs.remove_directory(composer_home).unwrap();
    fs.remove_directory(git_repo).unwrap();
}

struct TearDown {
    composer_home: std::path::PathBuf,
    git_repo: std::path::PathBuf,
}

impl TearDown {
    fn new(composer_home: std::path::PathBuf, git_repo: std::path::PathBuf) -> Self {
        TearDown {
            composer_home,
            git_repo,
        }
    }
}

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down(&self.composer_home, &self.git_repo);
    }
}

#[test]
#[serial]
fn test_load_versions() {
    let Some(set_up) = set_up() else {
        // git binary unavailable; skip like markTestSkipped.
        return;
    };
    let composer_home = set_up.composer_home.path().to_path_buf();
    let git_repo = set_up.git_repo.path().to_path_buf();

    let mut expected: IndexSet<String> = [
        "0.6.0",
        "1.0.0",
        "1.0.x-dev",
        "1.1.x-dev",
        "dev-feature-b",
        "dev-feature/a-1.0-B",
        "dev-foo+bar",
        "dev-master",
        "9999999-dev", // alias of dev-master
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    let mut config = Config::new(true, None);
    let mut top: IndexMap<String, PhpMixed> = IndexMap::new();
    let mut config_section: IndexMap<String, PhpMixed> = IndexMap::new();
    config_section.insert(
        "home".to_string(),
        PhpMixed::String(composer_home.to_string_lossy().into_owned()),
    );
    top.insert("config".to_string(), PhpMixed::Array(config_section));
    config.merge(&top, Config::SOURCE_UNKNOWN);
    let config = Rc::new(RefCell::new(config));

    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let http_downloader = Rc::new(RefCell::new(HttpDownloader::new(
        io.clone(),
        config.clone(),
        IndexMap::new(),
        false,
    )));
    let process = Rc::new(RefCell::new(ProcessExecutor::new(Some(io.clone()))));
    // VcsRepository's git driver / VersionGuesser run async git processes; constructing a Loop
    // enables async on the shared ProcessExecutor.
    let _loop = Loop::new(http_downloader.clone(), Some(process.clone()));

    let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
    repo_config.insert(
        "url".to_string(),
        PhpMixed::String(git_repo.to_string_lossy().into_owned()),
    );
    repo_config.insert("type".to_string(), PhpMixed::String("vcs".to_string()));

    let mut repo = VcsRepository::new(
        repo_config,
        io,
        config,
        http_downloader,
        None,
        Some(process),
        None,
        None,
    )
    .unwrap();

    let _tear_down = TearDown::new(composer_home, git_repo);

    let packages = repo.__get_packages().unwrap();

    for package in &packages {
        let pretty = package.get_pretty_version();
        assert!(
            expected.shift_remove(&pretty),
            "Unexpected version {}",
            pretty
        );
    }

    assert!(
        expected.is_empty(),
        "Missing versions: {}",
        expected.into_iter().collect::<Vec<_>>().join(", ")
    );
}
