//! ref: composer/tests/Composer/Test/Repository/PathRepositoryTest.php sha1:07002f7c81985331ff41fa16bc1c8ecf69b80151

use crate::test_case::get_package;
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use serial_test::serial;
use shirabe::config::Config;
use shirabe::io::{IOInterface, NullIO};
use shirabe::repository::PathRepository;
use shirabe::util::r#loop::Loop;
use shirabe::util::{HttpDownloader, Platform, ProcessExecutor};
use shirabe_php_shim::{
    PhpMixed, bconcat, file_get_contents, hash, path_from_bytes, path_to_bytes, realpath,
    serialize, str_replace,
};
use std::path::Path;

fn fixtures_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../composer/tests/Composer/Test/Repository/Fixtures")
}

/// ref: PathRepositoryTest::createPathRepo
fn create_path_repo(options: IndexMap<BString, PhpMixed>) -> PathRepository {
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));

    let config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(true, None)));
    let proc = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(None)));

    // ref: createPathRepo wires the ProcessExecutor through a Loop so the VersionGuesser's async
    // git calls are permitted; constructing the Loop calls enable_async() on the shared executor.
    let http_downloader = std::rc::Rc::new(std::cell::RefCell::new(
        HttpDownloader::new(
            io.clone(),
            config.clone(),
            shirabe::util::StreamContextOptions::new(),
            false,
        )
        .unwrap(),
    ));
    let _loop = Loop::new(http_downloader, Some(proc.clone()));

    PathRepository::new(options, io, config, None, None, Some(proc)).unwrap()
}

fn coordinates(pairs: Vec<(&BStr, PhpMixed)>) -> IndexMap<BString, PhpMixed> {
    let mut map: IndexMap<BString, PhpMixed> = IndexMap::new();
    for (key, value) in pairs {
        map.insert(bconcat!(key), value);
    }
    map
}

#[test]
fn test_load_package_from_file_system_with_incorrect_path() {
    let repository_url = Path::new(&fixtures_dir()).join("path").join("missing");
    let mut repository = create_path_repo(coordinates(vec![(
        BStr::new("url"),
        PhpMixed::string(path_to_bytes(&repository_url)),
    )]));
    assert!(repository.__get_packages().is_err());
}

#[test]
fn test_load_package_from_file_system_with_version() {
    let repository_url = Path::new(&fixtures_dir()).join("path").join("with-version");
    let mut repository = create_path_repo(coordinates(vec![(
        BStr::new("url"),
        PhpMixed::string(path_to_bytes(&repository_url)),
    )]));
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
    let repository_url = Path::new(&fixtures_dir())
        .join("path")
        .join("without-version");
    let mut repository = create_path_repo(coordinates(vec![(
        BStr::new("url"),
        PhpMixed::string(path_to_bytes(&repository_url)),
    )]));
    let packages = repository.__get_packages().unwrap();

    assert!(repository.__count().unwrap() >= 1);

    let package = &packages[0];
    assert_eq!("test/path-unversioned", package.get_name());

    let package_version = package.get_version();
    assert!(!package_version.is_empty());
}

#[test]
fn test_load_package_from_file_system_with_wildcard() {
    let repository_url = Path::new(&fixtures_dir()).join("path").join("*");
    let mut repository = create_path_repo(coordinates(vec![(
        BStr::new("url"),
        PhpMixed::string(path_to_bytes(&repository_url)),
    )]));
    let packages = repository.__get_packages().unwrap();
    let mut names: Vec<BString> = Vec::new();

    assert!(repository.__count().unwrap() >= 2);

    let package = &packages[0];
    names.push(BString::from(&package.get_name()[..]));

    let package = &packages[1];
    names.push(BString::from(&package.get_name()[..]));

    names.sort();
    assert_eq!(
        vec![
            BString::from("test/path-unversioned"),
            BString::from("test/path-versioned")
        ],
        names
    );
}

#[test]
fn test_load_package_with_explicit_versions() {
    let mut versions: IndexMap<BString, PhpMixed> = IndexMap::new();
    versions.insert(
        "test/path-unversioned".into(),
        PhpMixed::string(BString::from("4.3.2.1")),
    );
    versions.insert(
        "test/path-versioned".into(),
        PhpMixed::string(BString::from("3.2.1.0")),
    );
    let options = coordinates(vec![(BStr::new("versions"), PhpMixed::Array(versions))]);

    let repository_url = Path::new(&fixtures_dir()).join("path").join("*");
    let mut repository = create_path_repo(coordinates(vec![
        (
            BStr::new("url"),
            PhpMixed::string(path_to_bytes(&repository_url)),
        ),
        (BStr::new("options"), PhpMixed::Array(options)),
    ]));
    let packages = repository.__get_packages().unwrap();

    let mut versions: IndexMap<BString, BString> = IndexMap::new();

    assert_eq!(2, repository.__count().unwrap());

    let package = &packages[0];
    versions.insert(
        BString::from(&package.get_name()[..]),
        BString::from(&package.get_version()[..]),
    );

    let package = &packages[1];
    versions.insert(
        BString::from(&package.get_name()[..]),
        BString::from(&package.get_version()[..]),
    );

    versions.sort_keys();
    let expected: IndexMap<BString, BString> = [
        ("test/path-unversioned".into(), "4.3.2.1".into()),
        ("test/path-versioned".into(), "3.2.1.0".into()),
    ]
    .into_iter()
    .collect();
    assert_eq!(expected, versions);
}

/// Restores the previous cwd on drop so a panicking assertion cannot leak the changed cwd into
/// other tests.
struct CwdGuard {
    prev_cwd: std::path::PathBuf,
}

impl CwdGuard {
    fn new(dir: &Path) -> Self {
        let prev_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir).unwrap();
        Self { prev_cwd }
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.prev_cwd);
    }
}

/// Verify relative repository URLs remain relative, see #4439
#[test]
#[serial]
fn test_url_remains_relative() {
    // PHP runs under phpunit with the process cwd inside the composer checkout, an ancestor of
    // __DIR__, so stripping the cwd prefix yields a valid relative path; cargo runs tests from the
    // crate manifest dir, which is not an ancestor of the fixtures, so replicate the phpunit
    // precondition by chdir'ing to the __DIR__ equivalent for the duration of the test. #[serial]
    // keeps other cwd-touching tests in this binary from interleaving.
    let _cwd_guard = CwdGuard::new(path_from_bytes(&str_replace(
        "/Fixtures",
        "",
        path_to_bytes(&fixtures_dir()),
    )));

    // realpath() does not fully expand the paths
    // PHP Bug https://bugs.php.net/bug.php?id=72642
    let repository_url = realpath(
        realpath(path_from_bytes(&str_replace(
            "/Fixtures",
            "",
            path_to_bytes(&fixtures_dir()),
        )))
        .unwrap_or_default(),
    )
    .unwrap_or_default()
    .join("Fixtures")
    .join("path")
    .join("with-version");
    // getcwd() not necessarily match __DIR__
    // PHP Bug https://bugs.php.net/bug.php?id=73797
    let cwd = realpath(realpath(Platform::get_cwd(false).unwrap()).unwrap_or_default())
        .map(|p| BString::from(path_to_bytes(&p)))
        .unwrap_or_default();
    let repository_url = path_to_bytes(&repository_url);
    let relative_url = BString::from(
        repository_url[cwd.len().min(repository_url.len())..]
            .trim_start_with(|c| c == std::path::MAIN_SEPARATOR),
    );

    let mut repository = create_path_repo(coordinates(vec![(
        BStr::new("url"),
        PhpMixed::string(relative_url.clone()),
    )]));
    let packages = repository.__get_packages().unwrap();

    assert_eq!(1, repository.__count().unwrap());

    let package = &packages[0];
    assert_eq!("test/path-versioned", package.get_name());

    // Convert platform specific separators back to generic URL slashes
    let relative_url = str_replace(std::path::MAIN_SEPARATOR_STR, "/", relative_url);
    assert_eq!(Some(relative_url), package.get_dist_url());
}

#[test]
fn test_reference_none() {
    let options = coordinates(vec![(
        BStr::new("reference"),
        PhpMixed::string(BString::from("none")),
    )]);
    let repository_url = Path::new(&fixtures_dir()).join("path").join("*");
    let mut repository = create_path_repo(coordinates(vec![
        (
            BStr::new("url"),
            PhpMixed::string(path_to_bytes(&repository_url)),
        ),
        (BStr::new("options"), PhpMixed::Array(options)),
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
        (
            BStr::new("reference"),
            PhpMixed::string(BString::from("config")),
        ),
        (BStr::new("relative"), PhpMixed::Bool(true)),
    ]);
    let repository_url = Path::new(&fixtures_dir()).join("path").join("*");
    let mut repository = create_path_repo(coordinates(vec![
        (
            BStr::new("url"),
            PhpMixed::string(path_to_bytes(&repository_url)),
        ),
        (BStr::new("options"), PhpMixed::Array(options.clone())),
    ]));
    let packages = repository.__get_packages().unwrap();

    assert!(repository.__count().unwrap() >= 2);

    for package in &packages {
        let dist_url = package.get_dist_url().unwrap_or_default();
        assert_eq!(
            package.get_dist_reference(),
            Some(hash(
                "sha1",
                [
                    file_get_contents(path_from_bytes(&dist_url).join("composer.json"))
                        .unwrap_or_default(),
                    Vec::from(serialize(&PhpMixed::Array(options.clone()))),
                ]
                .concat()
            ))
        );
    }
}
