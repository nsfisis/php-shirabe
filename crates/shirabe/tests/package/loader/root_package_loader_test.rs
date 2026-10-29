//! ref: composer/tests/Composer/Test/Package/Loader/RootPackageLoaderTest.php

// These build a RootPackageLoader with a mocked RepositoryManager and either a mocked
// ProcessExecutor / VersionGuesser or require constraints whose parsing goes through a
// look-around regex the regex crate cannot compile.

use crate::process_executor_mock::{cmd, cmd_full, get_process_executor_mock};
use crate::test_case::GitVersionGuard;
use bstr::BString;
use indexmap::IndexMap;
use serial_test::serial;
use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::package::loader::RootPackageLoader;
use shirabe::package::version::{
    VersionData, VersionGuesser, VersionGuesserInterface, VersionParser,
};
use shirabe::package::{RootPackage, STABILITY_ALPHA, STABILITY_DEV, STABILITY_RC};
use shirabe::repository::RepositoryManager;
use shirabe::util::Git as GitUtil;
use shirabe::util::http_downloader::HttpDownloader;
use shirabe::util::process_executor::{MockExpectation, MockHandler, ProcessExecutor};
use shirabe_php_shim::PhpMixed;

fn null_io() -> std::rc::Rc<std::cell::RefCell<dyn IOInterface>> {
    std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()))
}

fn http_downloader(
    io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    config: &std::rc::Rc<std::cell::RefCell<Config>>,
) -> std::rc::Rc<std::cell::RefCell<HttpDownloader>> {
    std::rc::Rc::new(std::cell::RefCell::new(HttpDownloader::new(
        io.clone(),
        config.clone(),
        IndexMap::new(),
        true,
    )))
}

// `$config = new Config; $config->merge(['repositories' => ['packagist' => false]]);`
fn make_config() -> std::rc::Rc<std::cell::RefCell<Config>> {
    let config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(true, None)));
    let mut repositories: IndexMap<BString, PhpMixed> = IndexMap::new();
    repositories.insert("packagist".into(), PhpMixed::Bool(false));
    let mut merge: IndexMap<BString, PhpMixed> = IndexMap::new();
    merge.insert("repositories".into(), PhpMixed::Array(repositories));
    config.borrow_mut().merge(&merge, "test");
    config
}

// Stands in for `getMockBuilder('Composer\Repository\RepositoryManager')->disableOriginalConstructor()`.
// The loader only stores it and feeds it default repositories, so a real instance suffices.
fn make_manager(
    io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    config: &std::rc::Rc<std::cell::RefCell<Config>>,
) -> std::rc::Rc<std::cell::RefCell<RepositoryManager>> {
    std::rc::Rc::new(std::cell::RefCell::new(RepositoryManager::new(
        io.clone(),
        config.clone(),
        http_downloader(io, config),
        None,
        None,
    )))
}

fn require_map(entries: &[(&str, &str)]) -> PhpMixed {
    let mut m: IndexMap<BString, PhpMixed> = IndexMap::new();
    for (k, v) in entries {
        m.insert(k.to_string().into(), PhpMixed::string(v.to_string()));
    }
    PhpMixed::Array(m)
}

// A test double for the concrete VersionGuesser, supplied through the VersionGuesserInterface seam.
mockall::mock! {
    #[derive(Debug)]
    pub VersionGuesser {}
    impl VersionGuesserInterface for VersionGuesser {
        fn guess_version(
            &mut self,
            package_config: &IndexMap<BString, PhpMixed>,
            path: &str,
        ) -> anyhow::Result<Option<VersionData>>;

        fn get_root_version_from_env(&self) -> anyhow::Result<String>;
    }
}

#[test]
fn test_stability_flags_parsing() {
    let io = null_io();
    let config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(true, None)));
    {
        let mut cfg = IndexMap::new();
        cfg.insert(
            "repositories".into(),
            PhpMixed::Array({
                let mut m = IndexMap::new();
                m.insert("packagist".into(), PhpMixed::Bool(false));
                m
            }),
        );
        config.borrow_mut().merge(&cfg, "test");
    }

    let manager = std::rc::Rc::new(std::cell::RefCell::new(RepositoryManager::new(
        io.clone(),
        config.clone(),
        http_downloader(&io, &config),
        None,
        None,
    )));

    let process_executor = ProcessExecutor::new(Some(io.clone()));
    process_executor.enable_async();
    let guesser = VersionGuesser::new(
        config.clone(),
        std::rc::Rc::new(std::cell::RefCell::new(process_executor)),
        VersionParser::new(),
        Some(io.clone()),
    );

    let mut loader = RootPackageLoader::new(manager, config, None, Some(Box::new(guesser)), None);

    let mut data = IndexMap::new();
    data.insert(
        "require".into(),
        PhpMixed::Array({
            let mut m = IndexMap::new();
            m.insert(
                "foo/bar".into(),
                PhpMixed::string("~2.1.0-beta2".to_string()),
            );
            m.insert(
                "bar/baz".into(),
                PhpMixed::string("1.0.x-dev as 1.2.0".to_string()),
            );
            m.insert("qux/quux".into(), PhpMixed::string("1.0.*@rc".to_string()));
            m.insert(
                "zux/complex".into(),
                PhpMixed::string("~1.0,>=1.0.2@dev".to_string()),
            );
            m.insert(
                "or/op".into(),
                PhpMixed::string("^2.0@dev || ^2.0@dev".to_string()),
            );
            m.insert(
                "multi/lowest-wins".into(),
                PhpMixed::string("^2.0@rc || >=3.0@dev , ~3.5@alpha".to_string()),
            );
            m.insert(
                "or/op-without-flags".into(),
                PhpMixed::string("dev-master || 2.0 , ~3.5-alpha".to_string()),
            );
            m.insert(
                "or/op-without-flags2".into(),
                PhpMixed::string("3.0-beta || 2.0 , ~3.5-alpha".to_string()),
            );
            m
        }),
    );
    data.insert(
        "minimum-stability".into(),
        PhpMixed::string("alpha".to_string()),
    );

    let package = loader
        .load(data, "Composer\\Package\\RootPackage", None)
        .unwrap();
    let package = package.as_root().unwrap();

    assert_eq!("alpha", package.get_minimum_stability());

    let mut expected = IndexMap::new();
    expected.insert(BString::from("bar/baz"), STABILITY_DEV);
    expected.insert(BString::from("qux/quux"), STABILITY_RC);
    expected.insert(BString::from("zux/complex"), STABILITY_DEV);
    expected.insert(BString::from("or/op"), STABILITY_DEV);
    expected.insert(BString::from("multi/lowest-wins"), STABILITY_DEV);
    expected.insert(BString::from("or/op-without-flags"), STABILITY_DEV);
    expected.insert(BString::from("or/op-without-flags2"), STABILITY_ALPHA);
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

    let mut loader = RootPackageLoader::new(manager, config, None, Some(Box::new(guesser)), None);

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

    let mut version_guesser = MockVersionGuesser::new();
    version_guesser
        .expect_guess_version()
        .times(1..)
        .returning(|_, _| {
            Ok(Some(VersionData {
                version: Some("3.0.9999999.9999999-dev".to_string()),
                commit: Some("aabbccddee".to_string()),
                pretty_version: Some("3.0-dev".to_string()),
                feature_version: None,
                feature_pretty_version: None,
            }))
        });

    let mut loader =
        RootPackageLoader::new(manager, config, None, Some(Box::new(version_guesser)), None);

    let package = loader
        .load(IndexMap::new(), "Composer\\Package\\RootPackage", None)
        .unwrap();

    assert_eq!("3.0-dev", package.as_root().unwrap().get_pretty_version());
}

#[test]
#[serial]
fn test_feature_branch_pretty_version() {
    // proc_open() is always available; the PHP markTestSkipped guard does not apply here.
    GitUtil::__set_version(Some("2.52.0".into()));
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

    let mut loader = RootPackageLoader::new(manager, config, None, Some(Box::new(guesser)), None);

    let mut data = IndexMap::new();
    data.insert(
        "require".into(),
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
    GitUtil::__set_version(Some("2.52.0".into()));
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

    let mut loader = RootPackageLoader::new(manager, config, None, Some(Box::new(guesser)), None);

    let mut data = IndexMap::new();
    data.insert(
        "require".into(),
        require_map(&[("foo/bar", "self.version")]),
    );
    data.insert(
        "non-feature-branches".into(),
        PhpMixed::List(vec![PhpMixed::string("latest-.*".to_string())]),
    );

    let package = loader
        .load(data, "Composer\\Package\\RootPackage", None)
        .unwrap();

    assert_eq!(
        "dev-latest-production",
        package.as_root().unwrap().get_pretty_version()
    );
}
