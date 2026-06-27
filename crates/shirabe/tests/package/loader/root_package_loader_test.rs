//! ref: composer/tests/Composer/Test/Package/Loader/RootPackageLoaderTest.php

// These build a RootPackageLoader with a mocked RepositoryManager and either a mocked
// ProcessExecutor / VersionGuesser or require constraints whose parsing goes through a
// look-around regex the regex crate cannot compile.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use indexmap::IndexMap;
use serial_test::serial;
use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::package::RootPackage;
use shirabe::package::loader::RootPackageLoader;
use shirabe::package::version::{
    VersionData, VersionGuesser, VersionGuesserInterface, VersionParser,
};
use shirabe::package::{STABILITY_ALPHA, STABILITY_DEV, STABILITY_RC};
use shirabe::repository::RepositoryManager;
use shirabe::util::Git as GitUtil;
use shirabe::util::http_downloader::HttpDownloader;
use shirabe::util::process_executor::{MockExpectation, MockHandler, ProcessExecutor};
use shirabe_php_shim::PhpMixed;

use crate::process_executor_mock::{cmd, cmd_full, get_process_executor_mock};

fn null_io() -> Rc<RefCell<dyn IOInterface>> {
    Rc::new(RefCell::new(NullIO::new()))
}

fn http_downloader(
    io: &Rc<RefCell<dyn IOInterface>>,
    config: &Rc<RefCell<Config>>,
) -> Rc<RefCell<HttpDownloader>> {
    Rc::new(RefCell::new(HttpDownloader::new(
        io.clone(),
        config.clone(),
        IndexMap::new(),
        true,
    )))
}

// `$config = new Config; $config->merge(['repositories' => ['packagist' => false]]);`
fn make_config() -> Rc<RefCell<Config>> {
    let config = Rc::new(RefCell::new(Config::new(true, None)));
    let mut repositories: IndexMap<String, PhpMixed> = IndexMap::new();
    repositories.insert("packagist".to_string(), PhpMixed::Bool(false));
    let mut merge: IndexMap<String, PhpMixed> = IndexMap::new();
    merge.insert("repositories".to_string(), PhpMixed::Array(repositories));
    config.borrow_mut().merge(&merge, "test");
    config
}

// Stands in for `getMockBuilder('Composer\Repository\RepositoryManager')->disableOriginalConstructor()`.
// The loader only stores it and feeds it default repositories, so a real instance suffices.
fn make_manager(
    io: &Rc<RefCell<dyn IOInterface>>,
    config: &Rc<RefCell<Config>>,
) -> Rc<RefCell<RepositoryManager>> {
    Rc::new(RefCell::new(RepositoryManager::new(
        io.clone(),
        config.clone(),
        http_downloader(io, config),
        None,
        None,
    )))
}

fn require_map(entries: &[(&str, &str)]) -> PhpMixed {
    let mut m: IndexMap<String, PhpMixed> = IndexMap::new();
    for (k, v) in entries {
        m.insert(k.to_string(), PhpMixed::String(v.to_string()));
    }
    PhpMixed::Array(m)
}

// Resets the cached git `version` static on drop so a seeded value does not leak into other
// tests in this binary (VersionGuesserTest seeds/resets the same static).
struct GitVersionGuard;

impl Drop for GitVersionGuard {
    fn drop(&mut self) {
        GitUtil::__reset_version();
    }
}

// A test double for the concrete VersionGuesser, supplied through the VersionGuesserInterface seam.
#[derive(Debug)]
struct VersionGuesserMock {
    version_data: VersionData,
    guess_version_calls: Rc<Cell<u32>>,
}

impl VersionGuesserInterface for VersionGuesserMock {
    fn guess_version(
        &mut self,
        _package_config: &IndexMap<String, PhpMixed>,
        _path: &str,
    ) -> anyhow::Result<Option<VersionData>> {
        self.guess_version_calls
            .set(self.guess_version_calls.get() + 1);
        Ok(Some(self.version_data.clone()))
    }

    fn get_root_version_from_env(&self) -> anyhow::Result<String> {
        unreachable!("COMPOSER_ROOT_VERSION is not set in this test")
    }
}

#[test]
#[ignore = "process_executor.enable_async() drives the async stream path, which calls stream_set_blocking (fcntl(2) todo!() in shirabe-php-shim::stream)"]
fn test_stability_flags_parsing() {
    let io = null_io();
    let config = Rc::new(RefCell::new(Config::new(true, None)));
    {
        let mut cfg = IndexMap::new();
        cfg.insert(
            "repositories".to_string(),
            PhpMixed::Array({
                let mut m = IndexMap::new();
                m.insert("packagist".to_string(), PhpMixed::Bool(false));
                m
            }),
        );
        config.borrow_mut().merge(&cfg, "test");
    }

    let manager = Rc::new(RefCell::new(RepositoryManager::new(
        io.clone(),
        config.clone(),
        http_downloader(&io, &config),
        None,
        None,
    )));

    let mut process_executor = ProcessExecutor::new(Some(io.clone()));
    process_executor.enable_async();
    let guesser = VersionGuesser::new(
        config.clone(),
        Rc::new(RefCell::new(process_executor)),
        VersionParser::new(),
        Some(io.clone()),
    );

    let mut loader =
        RootPackageLoader::new(manager, config.clone(), None, Some(Box::new(guesser)), None);

    let mut data = IndexMap::new();
    data.insert(
        "require".to_string(),
        PhpMixed::Array({
            let mut m = IndexMap::new();
            m.insert(
                "foo/bar".to_string(),
                PhpMixed::String("~2.1.0-beta2".to_string()),
            );
            m.insert(
                "bar/baz".to_string(),
                PhpMixed::String("1.0.x-dev as 1.2.0".to_string()),
            );
            m.insert(
                "qux/quux".to_string(),
                PhpMixed::String("1.0.*@rc".to_string()),
            );
            m.insert(
                "zux/complex".to_string(),
                PhpMixed::String("~1.0,>=1.0.2@dev".to_string()),
            );
            m.insert(
                "or/op".to_string(),
                PhpMixed::String("^2.0@dev || ^2.0@dev".to_string()),
            );
            m.insert(
                "multi/lowest-wins".to_string(),
                PhpMixed::String("^2.0@rc || >=3.0@dev , ~3.5@alpha".to_string()),
            );
            m.insert(
                "or/op-without-flags".to_string(),
                PhpMixed::String("dev-master || 2.0 , ~3.5-alpha".to_string()),
            );
            m.insert(
                "or/op-without-flags2".to_string(),
                PhpMixed::String("3.0-beta || 2.0 , ~3.5-alpha".to_string()),
            );
            m
        }),
    );
    data.insert(
        "minimum-stability".to_string(),
        PhpMixed::String("alpha".to_string()),
    );

    let package = loader
        .load(data, "Composer\\Package\\RootPackage", None)
        .unwrap();
    let package = package.as_root().unwrap();

    assert_eq!("alpha", package.get_minimum_stability());

    let mut expected = IndexMap::new();
    expected.insert("bar/baz".to_string(), STABILITY_DEV);
    expected.insert("qux/quux".to_string(), STABILITY_RC);
    expected.insert("zux/complex".to_string(), STABILITY_DEV);
    expected.insert("or/op".to_string(), STABILITY_DEV);
    expected.insert("multi/lowest-wins".to_string(), STABILITY_DEV);
    expected.insert("or/op-without-flags".to_string(), STABILITY_DEV);
    expected.insert("or/op-without-flags2".to_string(), STABILITY_ALPHA);
    assert_eq!(expected, package.get_stability_flags());
}

#[test]
#[serial]
fn test_no_version_is_visible_in_pretty_version() {
    GitUtil::__reset_version();
    let _git_guard = GitVersionGuard;

    let io = null_io();
    let config = make_config();
    let manager = make_manager(&io, &config);

    let (process, _guard) = get_process_executor_mock(
        vec![],
        false,
        MockHandler {
            r#return: 1,
            ..Default::default()
        },
    );
    let guesser = VersionGuesser::new(config.clone(), process, VersionParser::new(), None);

    let mut loader =
        RootPackageLoader::new(manager, config.clone(), None, Some(Box::new(guesser)), None);

    let package = loader
        .load(IndexMap::new(), "Composer\\Package\\RootPackage", None)
        .unwrap();
    let package = package.as_root().unwrap();

    assert_eq!("1.0.0.0", package.get_version());
    assert_eq!(
        RootPackage::DEFAULT_PRETTY_VERSION,
        package.get_pretty_version()
    );
}

#[test]
#[serial]
fn test_pretty_version_for_root_package_in_version_branch() {
    // see #6845
    let io = null_io();
    let config = make_config();
    let manager = make_manager(&io, &config);

    let guess_version_calls = Rc::new(Cell::new(0u32));
    let version_guesser = VersionGuesserMock {
        version_data: VersionData {
            version: Some("3.0.9999999.9999999-dev".to_string()),
            commit: Some("aabbccddee".to_string()),
            pretty_version: Some("3.0-dev".to_string()),
            feature_version: None,
            feature_pretty_version: None,
        },
        guess_version_calls: guess_version_calls.clone(),
    };

    let mut loader = RootPackageLoader::new(
        manager,
        config.clone(),
        None,
        Some(Box::new(version_guesser)),
        None,
    );

    let package = loader
        .load(IndexMap::new(), "Composer\\Package\\RootPackage", None)
        .unwrap();

    assert!(guess_version_calls.get() >= 1);
    assert_eq!("3.0-dev", package.as_root().unwrap().get_pretty_version());
}

#[test]
#[ignore = "feature-branch guessing calls ProcessExecutor::execute_async, whose mock path is todo!()"]
#[serial]
fn test_feature_branch_pretty_version() {
    // proc_open() is always available; the PHP markTestSkipped guard does not apply here.
    GitUtil::__set_version(Some("2.52.0".to_string()));
    let _git_guard = GitVersionGuard;

    let io = null_io();
    let config = make_config();
    let manager = make_manager(&io, &config);

    let expectations: Vec<MockExpectation> = vec![
        cmd_full(
            ["git", "branch", "-a", "--no-color", "--no-abbrev", "-v"],
            0,
            "* latest-production 38137d2f6c70e775e137b2d8a7a7d3eaebf7c7e5 Commit message\n  master 4f6ed96b0bc363d2aa4404c3412de1c011f67c66 Commit message\n",
            "",
        ),
        cmd(["git", "rev-list", "master..latest-production"]),
    ];
    let (process, _guard) = get_process_executor_mock(expectations, true, MockHandler::default());
    let guesser = VersionGuesser::new(config.clone(), process, VersionParser::new(), None);

    let mut loader =
        RootPackageLoader::new(manager, config.clone(), None, Some(Box::new(guesser)), None);

    let mut data = IndexMap::new();
    data.insert(
        "require".to_string(),
        require_map(&[("foo/bar", "self.version")]),
    );

    let package = loader
        .load(data, "Composer\\Package\\RootPackage", None)
        .unwrap();

    assert_eq!(
        "dev-master",
        package.as_root().unwrap().get_pretty_version()
    );
}

#[test]
#[serial]
fn test_non_feature_branch_pretty_version() {
    // proc_open() is always available; the PHP markTestSkipped guard does not apply here.
    GitUtil::__set_version(Some("2.52.0".to_string()));
    let _git_guard = GitVersionGuard;

    let io = null_io();
    let config = make_config();
    let manager = make_manager(&io, &config);

    let expectations: Vec<MockExpectation> = vec![cmd_full(
        ["git", "branch", "-a", "--no-color", "--no-abbrev", "-v"],
        0,
        "* latest-production 38137d2f6c70e775e137b2d8a7a7d3eaebf7c7e5 Commit message\n  master 4f6ed96b0bc363d2aa4404c3412de1c011f67c66 Commit message\n",
        "",
    )];
    let (process, _guard) = get_process_executor_mock(expectations, true, MockHandler::default());
    let guesser = VersionGuesser::new(config.clone(), process, VersionParser::new(), None);

    let mut loader =
        RootPackageLoader::new(manager, config.clone(), None, Some(Box::new(guesser)), None);

    let mut data = IndexMap::new();
    data.insert(
        "require".to_string(),
        require_map(&[("foo/bar", "self.version")]),
    );
    data.insert(
        "non-feature-branches".to_string(),
        PhpMixed::List(vec![PhpMixed::String("latest-.*".to_string())]),
    );

    let package = loader
        .load(data, "Composer\\Package\\RootPackage", None)
        .unwrap();

    assert_eq!(
        "dev-latest-production",
        package.as_root().unwrap().get_pretty_version()
    );
}
