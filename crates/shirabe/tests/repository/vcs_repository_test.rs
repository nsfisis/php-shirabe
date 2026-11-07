//! ref: composer/tests/Composer/Test/Repository/VcsRepositoryTest.php sha1:628daae845b6a291274618b8cd28d0fd8069a6a9

use bstr::{BStr, BString, ByteSlice};
use indexmap::{IndexMap, IndexSet};
use serial_test::serial;
use shirabe::config::Config;
use shirabe::io::{IOInterface, NullIO};
use shirabe::repository::VcsRepository;
use shirabe::util::r#loop::Loop;
use shirabe::util::{Filesystem, HttpDownloader, ProcessExecutor};
use shirabe_php_shim::{
    PhpMixed, bconcat, implode, os_str_from_bytes, path_from_bytes, path_to_bytes,
};
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

    let exec = |args: &[&BStr]| {
        let status = std::process::Command::new("git")
            .args(args.iter().map(os_str_from_bytes))
            .current_dir(path)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .status()
            .unwrap_or_else(|e| panic!("Failed to execute git {:?}: {}", args, e));
        if !status.success() {
            panic!("Failed to execute git {:?}", args);
        }
    };
    let write_file = |name: &BStr, contents: &BStr| {
        std::fs::write(path.join(path_from_bytes(&name)), contents).unwrap();
    };

    // init
    exec(&[BStr::new("init"), BStr::new("-q")]);
    exec(&[BStr::new("checkout"), BStr::new("-b"), BStr::new("master")]);
    exec(&[
        BStr::new("config"),
        BStr::new("user.email"),
        BStr::new("composertest@example.org"),
    ]);
    exec(&[
        BStr::new("config"),
        BStr::new("user.name"),
        BStr::new("ComposerTest"),
    ]);
    exec(&[
        BStr::new("config"),
        BStr::new("commit.gpgsign"),
        BStr::new("false"),
    ]);
    write_file(BStr::new("foo"), BStr::new(""));
    exec(&[BStr::new("add"), BStr::new("foo")]);
    exec(&[BStr::new("commit"), BStr::new("-m"), BStr::new("init")]);

    // non-composed tag & branch
    exec(&[BStr::new("tag"), BStr::new("0.5.0")]);
    exec(&[BStr::new("branch"), BStr::new("oldbranch")]);

    // add composed tag & master branch
    write_file(
        BStr::new("composer.json"),
        shirabe::json::JsonFile::encode(&composer(None))
            .unwrap()
            .as_bstr(),
    );
    exec(&[BStr::new("add"), BStr::new("composer.json")]);
    exec(&[
        BStr::new("commit"),
        BStr::new("-m"),
        BStr::new("addcomposer"),
    ]);
    exec(&[BStr::new("tag"), BStr::new("0.6.0")]);

    // add feature-a branch
    exec(&[
        BStr::new("checkout"),
        BStr::new("-b"),
        BStr::new("feature/a-1.0-B"),
    ]);
    write_file(BStr::new("foo"), BStr::new("bar feature"));
    exec(&[BStr::new("add"), BStr::new("foo")]);
    exec(&[BStr::new("commit"), BStr::new("-m"), BStr::new("change-a")]);

    // add foo#bar branch which should result in dev-foo+bar
    exec(&[BStr::new("branch"), BStr::new("foo#bar")]);

    // add version to composer.json
    exec(&[BStr::new("checkout"), BStr::new("master")]);
    write_file(
        BStr::new("composer.json"),
        shirabe::json::JsonFile::encode(&composer(Some(BStr::new("1.0.0"))))
            .unwrap()
            .as_bstr(),
    );
    exec(&[BStr::new("add"), BStr::new("composer.json")]);
    exec(&[
        BStr::new("commit"),
        BStr::new("-m"),
        BStr::new("addversion"),
    ]);

    // create tag with wrong version in it
    exec(&[BStr::new("tag"), BStr::new("0.9.0")]);
    // create tag with correct version in it
    exec(&[BStr::new("tag"), BStr::new("1.0.0")]);

    // add feature-b branch
    exec(&[
        BStr::new("checkout"),
        BStr::new("-b"),
        BStr::new("feature-b"),
    ]);
    write_file(BStr::new("foo"), BStr::new("baz feature"));
    exec(&[BStr::new("add"), BStr::new("foo")]);
    exec(&[BStr::new("commit"), BStr::new("-m"), BStr::new("change-b")]);

    // add 1.0 branch
    exec(&[BStr::new("checkout"), BStr::new("master")]);
    exec(&[BStr::new("branch"), BStr::new("1.0")]);

    // add 1.0.x branch
    exec(&[BStr::new("branch"), BStr::new("1.1.x")]);

    // update master to 2.0
    write_file(
        BStr::new("composer.json"),
        shirabe::json::JsonFile::encode(&composer(Some(BStr::new("2.0.0"))))
            .unwrap()
            .as_bstr(),
    );
    exec(&[BStr::new("add"), BStr::new("composer.json")]);
    exec(&[
        BStr::new("commit"),
        BStr::new("-m"),
        BStr::new("bump-version"),
    ]);

    Some(SetUp {
        composer_home,
        git_repo,
    })
}

fn composer(version: Option<&BStr>) -> PhpMixed {
    let mut m: IndexMap<BString, PhpMixed> = IndexMap::new();
    m.insert("name".into(), PhpMixed::string(BString::from("a/b")));
    if let Some(version) = version {
        m.insert("version".into(), PhpMixed::string(bconcat!(version)));
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

    let mut expected: IndexSet<BString> = [
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
    .map(|s| BString::from(*s))
    .collect();

    let mut config = Config::new(true, None);
    let mut top: IndexMap<BString, PhpMixed> = IndexMap::new();
    let mut config_section: IndexMap<BString, PhpMixed> = IndexMap::new();
    config_section.insert(
        "home".into(),
        PhpMixed::string(path_to_bytes(&composer_home)),
    );
    top.insert("config".into(), PhpMixed::Array(config_section));
    config.merge(&top, Config::SOURCE_UNKNOWN);
    let config = std::rc::Rc::new(std::cell::RefCell::new(config));

    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let http_downloader = std::rc::Rc::new(std::cell::RefCell::new(
        HttpDownloader::new(
            io.clone(),
            config.clone(),
            shirabe::util::StreamContextOptions::new(),
            false,
        )
        .unwrap(),
    ));
    let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
        io.clone(),
    ))));
    // VcsRepository's git driver / VersionGuesser run async git processes; constructing a Loop
    // enables async on the shared ProcessExecutor.
    let _loop = Loop::new(http_downloader.clone(), Some(process.clone()));

    let mut repo_config: IndexMap<BString, PhpMixed> = IndexMap::new();
    repo_config.insert("url".into(), PhpMixed::string(path_to_bytes(&git_repo)));
    repo_config.insert("type".into(), PhpMixed::string(BString::from("vcs")));

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
        let pretty = BString::from(&package.get_pretty_version()[..]);
        assert!(
            expected.shift_remove(&pretty),
            "Unexpected version {}",
            pretty.display_lossy()
        );
    }

    assert!(
        expected.is_empty(),
        "Missing versions: {}",
        implode(", ", &expected.into_iter().collect::<Vec<_>>()).display_lossy()
    );
}
