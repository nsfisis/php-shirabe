//! ref: composer/tests/Composer/Test/Repository/PathRepositoryTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::{IOInterface, NullIO};
use shirabe::repository::PathRepository;
use shirabe::util::http_downloader::HttpDownloader;
use shirabe::util::r#loop::Loop;
use shirabe::util::{Platform, ProcessExecutor};
use shirabe_php_shim::{
    DIRECTORY_SEPARATOR, PhpMixed, file_get_contents, hash, realpath, serialize,
};

use crate::test_case::get_package;

fn fixtures_dir() -> String {
    format!(
        "{}/../../composer/tests/Composer/Test/Repository/Fixtures",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// ref: PathRepositoryTest::createPathRepo
fn create_path_repo(options: IndexMap<String, PhpMixed>) -> PathRepository {
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));

    let config = Rc::new(RefCell::new(Config::new(true, None)));
    let proc = Rc::new(RefCell::new(ProcessExecutor::new(None)));

    // ref: createPathRepo wires the ProcessExecutor through a Loop so the VersionGuesser's async
    // git calls are permitted; constructing the Loop calls enable_async() on the shared executor.
    let http_downloader = Rc::new(RefCell::new(HttpDownloader::new(
        io.clone(),
        config.clone(),
        IndexMap::new(),
        false,
    )));
    let _loop = Loop::new(http_downloader, Some(proc.clone()));

    PathRepository::new(options, io, config, None, None, Some(proc)).unwrap()
}

fn coordinates(pairs: Vec<(&str, PhpMixed)>) -> IndexMap<String, PhpMixed> {
    let mut map: IndexMap<String, PhpMixed> = IndexMap::new();
    for (key, value) in pairs {
        map.insert(key.to_string(), value);
    }
    map
}

#[test]
fn test_load_package_from_file_system_with_incorrect_path() {
    let repository_url =
        [fixtures_dir(), "path".to_string(), "missing".to_string()].join(DIRECTORY_SEPARATOR);
    let mut repository =
        create_path_repo(coordinates(vec![("url", PhpMixed::String(repository_url))]));
    assert!(repository.__get_packages().is_err());
}

#[test]
fn test_load_package_from_file_system_with_version() {
    let repository_url = [
        fixtures_dir(),
        "path".to_string(),
        "with-version".to_string(),
    ]
    .join(DIRECTORY_SEPARATOR);
    let mut repository =
        create_path_repo(coordinates(vec![("url", PhpMixed::String(repository_url))]));
    repository.__get_packages().unwrap();

    assert_eq!(1, repository.__count().unwrap());
    assert!(
        repository
            .__has_package(get_package("test/path-versioned", "0.0.2"))
            .unwrap()
    );
}

#[test]
fn test_load_package_from_file_system_without_version() {
    let repository_url = [
        fixtures_dir(),
        "path".to_string(),
        "without-version".to_string(),
    ]
    .join(DIRECTORY_SEPARATOR);
    let mut repository =
        create_path_repo(coordinates(vec![("url", PhpMixed::String(repository_url))]));
    let packages = repository.__get_packages().unwrap();

    assert!(repository.__count().unwrap() >= 1);

    let package = &packages[0];
    assert_eq!("test/path-unversioned", package.get_name());

    let package_version = package.get_version();
    assert!(!package_version.is_empty());
}

#[test]
fn test_load_package_from_file_system_with_wildcard() {
    let repository_url =
        [fixtures_dir(), "path".to_string(), "*".to_string()].join(DIRECTORY_SEPARATOR);
    let mut repository =
        create_path_repo(coordinates(vec![("url", PhpMixed::String(repository_url))]));
    let packages = repository.__get_packages().unwrap();
    let mut names: Vec<String> = Vec::new();

    assert!(repository.__count().unwrap() >= 2);

    let package = &packages[0];
    names.push(package.get_name());

    let package = &packages[1];
    names.push(package.get_name());

    names.sort();
    assert_eq!(
        vec![
            "test/path-unversioned".to_string(),
            "test/path-versioned".to_string()
        ],
        names
    );
}

#[test]
fn test_load_package_with_explicit_versions() {
    let mut versions: IndexMap<String, PhpMixed> = IndexMap::new();
    versions.insert(
        "test/path-unversioned".to_string(),
        PhpMixed::String("4.3.2.1".to_string()),
    );
    versions.insert(
        "test/path-versioned".to_string(),
        PhpMixed::String("3.2.1.0".to_string()),
    );
    let options = coordinates(vec![("versions", PhpMixed::Array(versions))]);

    let repository_url =
        [fixtures_dir(), "path".to_string(), "*".to_string()].join(DIRECTORY_SEPARATOR);
    let mut repository = create_path_repo(coordinates(vec![
        ("url", PhpMixed::String(repository_url)),
        ("options", PhpMixed::Array(options)),
    ]));
    let packages = repository.__get_packages().unwrap();

    let mut versions: IndexMap<String, String> = IndexMap::new();

    assert_eq!(2, repository.__count().unwrap());

    let package = &packages[0];
    versions.insert(package.get_name(), package.get_version());

    let package = &packages[1];
    versions.insert(package.get_name(), package.get_version());

    versions.sort_keys();
    let expected: IndexMap<String, String> = [
        ("test/path-unversioned".to_string(), "4.3.2.1".to_string()),
        ("test/path-versioned".to_string(), "3.2.1.0".to_string()),
    ]
    .into_iter()
    .collect();
    assert_eq!(expected, versions);
}

/// Verify relative repository URLs remain relative, see #4439
#[ignore = "relies on the process cwd being the test's __DIR__ (the Repository fixtures dir) so the computed relative url resolves; under cargo the cwd is the crate manifest dir, so the relative url does not point at the fixture and getPackages errors"]
#[test]
fn test_url_remains_relative() {
    // realpath() does not fully expand the paths
    // PHP Bug https://bugs.php.net/bug.php?id=72642
    let repository_url = [
        realpath(&realpath(&fixtures_dir().replace("/Fixtures", "")).unwrap_or_default())
            .unwrap_or_default(),
        "Fixtures".to_string(),
        "path".to_string(),
        "with-version".to_string(),
    ]
    .join(DIRECTORY_SEPARATOR);
    // getcwd() not necessarily match __DIR__
    // PHP Bug https://bugs.php.net/bug.php?id=73797
    let cwd = realpath(&realpath(&Platform::get_cwd(false).unwrap()).unwrap_or_default())
        .unwrap_or_default();
    let relative_url = repository_url[cwd.len().min(repository_url.len())..]
        .trim_start_matches(DIRECTORY_SEPARATOR)
        .to_string();

    let mut repository = create_path_repo(coordinates(vec![(
        "url",
        PhpMixed::String(relative_url.clone()),
    )]));
    let packages = repository.__get_packages().unwrap();

    assert_eq!(1, repository.__count().unwrap());

    let package = &packages[0];
    assert_eq!("test/path-versioned", package.get_name());

    // Convert platform specific separators back to generic URL slashes
    let relative_url = relative_url.replace(DIRECTORY_SEPARATOR, "/");
    assert_eq!(Some(relative_url), package.get_dist_url());
}

#[test]
fn test_reference_none() {
    let options = coordinates(vec![("reference", PhpMixed::String("none".to_string()))]);
    let repository_url =
        [fixtures_dir(), "path".to_string(), "*".to_string()].join(DIRECTORY_SEPARATOR);
    let mut repository = create_path_repo(coordinates(vec![
        ("url", PhpMixed::String(repository_url)),
        ("options", PhpMixed::Array(options)),
    ]));
    let packages = repository.__get_packages().unwrap();

    assert!(repository.__count().unwrap() >= 2);

    for package in &packages {
        assert_eq!(package.get_dist_reference(), None);
    }
}

#[test]
fn test_reference_config() {
    let options = coordinates(vec![
        ("reference", PhpMixed::String("config".to_string())),
        ("relative", PhpMixed::Bool(true)),
    ]);
    let repository_url =
        [fixtures_dir(), "path".to_string(), "*".to_string()].join(DIRECTORY_SEPARATOR);
    let mut repository = create_path_repo(coordinates(vec![
        ("url", PhpMixed::String(repository_url)),
        ("options", PhpMixed::Array(options.clone())),
    ]));
    let packages = repository.__get_packages().unwrap();

    assert!(repository.__count().unwrap() >= 2);

    for package in &packages {
        let dist_url = package.get_dist_url().unwrap_or_default();
        assert_eq!(
            package.get_dist_reference(),
            Some(hash(
                "sha1",
                &format!(
                    "{}{}",
                    file_get_contents(format!("{}/composer.json", dist_url)).unwrap_or_default(),
                    serialize(&PhpMixed::Array(options.clone()))
                )
            ))
        );
    }
}
