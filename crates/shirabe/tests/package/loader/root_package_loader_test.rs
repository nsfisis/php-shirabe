//! ref: composer/tests/Composer/Test/Package/Loader/RootPackageLoaderTest.php

// These build a RootPackageLoader with a mocked RepositoryManager and either a mocked
// ProcessExecutor / VersionGuesser or require constraints whose parsing goes through a
// look-around regex the regex crate cannot compile.

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::package::loader::RootPackageLoader;
use shirabe::package::version::{VersionGuesser, VersionParser};
use shirabe::package::{STABILITY_ALPHA, STABILITY_DEV, STABILITY_RC};
use shirabe::repository::RepositoryManager;
use shirabe::util::http_downloader::HttpDownloader;
use shirabe::util::process_executor::ProcessExecutor;
use shirabe_php_shim::PhpMixed;

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

#[test]
#[ignore]
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

    let mut loader = RootPackageLoader::new(manager, config.clone(), None, Some(guesser), None);

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
#[ignore = "requires getProcessExecutorMock with expects(['return' => 1]); no ProcessExecutorMock mocking infrastructure exists"]
fn test_no_version_is_visible_in_pretty_version() {
    todo!()
}

#[test]
#[ignore = "requires getMockBuilder VersionGuesser mock with guessVersion expectation; no VersionGuesser mocking infrastructure exists"]
fn test_pretty_version_for_root_package_in_version_branch() {
    todo!()
}

#[test]
#[ignore = "requires getProcessExecutorMock with expects() git command expectations; no ProcessExecutorMock mocking infrastructure exists"]
fn test_feature_branch_pretty_version() {
    todo!()
}

#[test]
#[ignore = "requires getProcessExecutorMock with expects() git command expectations; no ProcessExecutorMock mocking infrastructure exists"]
fn test_non_feature_branch_pretty_version() {
    todo!()
}
