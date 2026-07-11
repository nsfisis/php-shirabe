//! ref: composer/tests/Composer/Test/Repository/RepositoryManagerTest.php

// These construct a RepositoryManager (which builds an HttpDownloader reaching
// curl_multi_init, todo!()) with a mocked IO/Config/EventDispatcher and exercise repo
// creation/prepending/wrapping.

use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::{
    ArrayRepository, FilterRepository, PathRepository, RepositoryInterfaceHandle, RepositoryManager,
};
use shirabe::util::filesystem::Filesystem;
use shirabe::util::http_downloader::HttpDownloader;
use shirabe_php_shim::PhpMixed;
use tempfile::TempDir;

struct SetUp {
    tmpdir: TempDir,
}

fn set_up() -> SetUp {
    let tmpdir = TempDir::new().unwrap();
    SetUp { tmpdir }
}

fn tear_down(tmpdir: &std::path::Path) {
    if tmpdir.is_dir() {
        let mut fs = Filesystem::new(None);
        fs.remove_directory(tmpdir).unwrap();
    }
}

struct TearDown {
    tmpdir: std::path::PathBuf,
}

impl TearDown {
    fn new(tmpdir: std::path::PathBuf) -> Self {
        TearDown { tmpdir }
    }
}

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down(&self.tmpdir);
    }
}

fn null_io() -> std::rc::Rc<std::cell::RefCell<dyn IOInterface>> {
    std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()))
}

fn http_downloader(
    io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
) -> std::rc::Rc<std::cell::RefCell<HttpDownloader>> {
    let config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(false, None)));
    std::rc::Rc::new(std::cell::RefCell::new(HttpDownloader::new(
        io.clone(),
        config,
        IndexMap::new(),
        true,
    )))
}

fn str_config(pairs: &[(&str, PhpMixed)]) -> IndexMap<String, PhpMixed> {
    let mut c: IndexMap<String, PhpMixed> = IndexMap::new();
    for (k, v) in pairs {
        c.insert(k.to_string(), v.clone());
    }
    c
}

#[test]
fn test_prepend() {
    let SetUp { tmpdir } = set_up();
    let _tear_down = TearDown::new(tmpdir.path().to_path_buf());

    let io = null_io();
    let config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(false, None)));
    let mut rm = RepositoryManager::new(io.clone(), config, http_downloader(&io), None, None);

    let repository1 = RepositoryInterfaceHandle::new(ArrayRepository::new(vec![]).unwrap());
    let repository2 = RepositoryInterfaceHandle::new(ArrayRepository::new(vec![]).unwrap());
    rm.add_repository(repository1.clone());
    rm.prepend_repository(repository2.clone());

    assert_eq!(&vec![repository2, repository1], rm.get_repositories());
}

#[test]
#[ignore = "create_repository routes to RepositoryManager::create_repository_by_class, which is todo!() (dynamic instantiation by class name not yet ported)"]
fn test_repo_creation() {
    let SetUp { tmpdir } = set_up();
    let _tear_down = TearDown::new(tmpdir.path().to_path_buf());

    let io = null_io();
    let config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(false, None)));
    let mut rm =
        RepositoryManager::new(io.clone(), config.clone(), http_downloader(&io), None, None);

    config.borrow_mut().merge(
        &str_config(&[(
            "config",
            PhpMixed::Array(str_config(&[(
                "cache-repo-dir",
                PhpMixed::String(tmpdir.path().to_string_lossy().to_string()),
            )])),
        )]),
        "unknown",
    );

    rm.set_repository_class("composer", "Composer\\Repository\\ComposerRepository");
    rm.set_repository_class("vcs", "Composer\\Repository\\VcsRepository");
    rm.set_repository_class("package", "Composer\\Repository\\PackageRepository");
    rm.set_repository_class("pear", "Composer\\Repository\\PearRepository");
    rm.set_repository_class("git", "Composer\\Repository\\VcsRepository");
    rm.set_repository_class("svn", "Composer\\Repository\\VcsRepository");
    rm.set_repository_class("perforce", "Composer\\Repository\\VcsRepository");
    rm.set_repository_class("hg", "Composer\\Repository\\VcsRepository");
    rm.set_repository_class("artifact", "Composer\\Repository\\ArtifactRepository");

    let cases: Vec<(&str, IndexMap<String, PhpMixed>)> = vec![
        (
            "composer",
            str_config(&[("url", PhpMixed::String("http://example.org".to_string()))]),
        ),
        (
            "vcs",
            str_config(&[(
                "url",
                PhpMixed::String("http://github.com/foo/bar".to_string()),
            )]),
        ),
        (
            "git",
            str_config(&[(
                "url",
                PhpMixed::String("http://github.com/foo/bar".to_string()),
            )]),
        ),
        (
            "git",
            str_config(&[(
                "url",
                PhpMixed::String("git@example.org:foo/bar.git".to_string()),
            )]),
        ),
        (
            "svn",
            str_config(&[(
                "url",
                PhpMixed::String("svn://example.org/foo/bar".to_string()),
            )]),
        ),
        (
            "package",
            str_config(&[("package", PhpMixed::Array(IndexMap::new()))]),
        ),
        (
            "artifact",
            str_config(&[("url", PhpMixed::String("/path/to/zips".to_string()))]),
        ),
    ];

    for (r#type, options) in cases {
        rm.create_repository(
            "composer",
            str_config(&[("url", PhpMixed::String("http://example.org".to_string()))]),
            None,
        )
        .unwrap();
        rm.create_repository(r#type, options, None).unwrap();
    }
}

#[test]
fn test_invalid_repo_creation_throws() {
    let SetUp { tmpdir } = set_up();
    let _tear_down = TearDown::new(tmpdir.path().to_path_buf());

    let io = null_io();
    let config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(false, None)));
    let rm = RepositoryManager::new(io.clone(), config.clone(), http_downloader(&io), None, None);

    config.borrow_mut().merge(
        &str_config(&[(
            "config",
            PhpMixed::Array(str_config(&[(
                "cache-repo-dir",
                PhpMixed::String(tmpdir.path().to_string_lossy().to_string()),
            )])),
        )]),
        "unknown",
    );

    let cases: Vec<(&str, IndexMap<String, PhpMixed>)> = vec![
        (
            "pear",
            str_config(&[(
                "url",
                PhpMixed::String("http://pear.example.org/foo".to_string()),
            )]),
        ),
        ("invalid", IndexMap::new()),
    ];

    for (r#type, options) in cases {
        assert!(rm.create_repository(r#type, options, None).is_err());
    }
}

#[test]
fn test_filter_repo_wrapping() {
    let SetUp { tmpdir } = set_up();
    let _tear_down = TearDown::new(tmpdir.path().to_path_buf());

    let io = null_io();
    let config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(false, None)));
    let mut rm = RepositoryManager::new(io.clone(), config, http_downloader(&io), None, None);

    rm.set_repository_class("path", "Composer\\Repository\\PathRepository");
    let repo = rm
        .create_repository(
            "path",
            str_config(&[
                ("type", PhpMixed::String("path".to_string())),
                (
                    "url",
                    PhpMixed::String(format!("{}/tests/repository", env!("CARGO_MANIFEST_DIR"))),
                ),
                (
                    "only",
                    PhpMixed::List(vec![PhpMixed::String("foo/bar".to_string())]),
                ),
            ]),
            None,
        )
        .unwrap();

    assert!(repo.is::<FilterRepository>());
    let filter = repo.downcast_rc::<FilterRepository>().unwrap();
    assert!(filter.borrow().get_repository().is::<PathRepository>());
}
