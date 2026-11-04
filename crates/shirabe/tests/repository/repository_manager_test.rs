//! ref: composer/tests/Composer/Test/Repository/RepositoryManagerTest.php sha1:61d442a8b5929f119f7c7f98e2d7a4002e93a852

// These construct a RepositoryManager (which builds an HttpDownloader reaching
// curl_multi_init, todo!()) with a mocked IO/Config/EventDispatcher and exercise repo
// creation/prepending/wrapping.

use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::{
    ArrayRepository, FilterRepository, PathRepository, RepositoryInterfaceHandle, RepositoryManager,
};
use shirabe::util::filesystem::Filesystem;
use shirabe::util::http_downloader::HttpDownloader;
use shirabe_php_shim::{PhpMixed, bconcat, path_to_bytes};
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

fn str_config(pairs: &[(&BStr, PhpMixed)]) -> IndexMap<BString, PhpMixed> {
    let mut c: IndexMap<BString, PhpMixed> = IndexMap::new();
    for (k, v) in pairs {
        c.insert(bconcat!(k), v.clone());
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
fn test_repo_creation() {
    let SetUp { tmpdir } = set_up();
    let _tear_down = TearDown::new(tmpdir.path().to_path_buf());

    let io = null_io();
    let config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(false, None)));
    let mut rm =
        RepositoryManager::new(io.clone(), config.clone(), http_downloader(&io), None, None);

    config.borrow_mut().merge(
        &str_config(&[(
            BStr::new("config"),
            PhpMixed::Array(str_config(&[(
                BStr::new("cache-repo-dir"),
                PhpMixed::string(path_to_bytes(&tmpdir.path())),
            )])),
        )]),
        "unknown",
    );

    rm.set_repository_class(
        bstr::BStr::new("composer"),
        BStr::new("Composer\\Repository\\ComposerRepository"),
    );
    rm.set_repository_class(
        bstr::BStr::new("vcs"),
        BStr::new("Composer\\Repository\\VcsRepository"),
    );
    rm.set_repository_class(
        bstr::BStr::new("package"),
        BStr::new("Composer\\Repository\\PackageRepository"),
    );
    rm.set_repository_class(
        bstr::BStr::new("pear"),
        BStr::new("Composer\\Repository\\PearRepository"),
    );
    rm.set_repository_class(
        bstr::BStr::new("git"),
        BStr::new("Composer\\Repository\\VcsRepository"),
    );
    rm.set_repository_class(
        bstr::BStr::new("svn"),
        BStr::new("Composer\\Repository\\VcsRepository"),
    );
    rm.set_repository_class(
        bstr::BStr::new("perforce"),
        BStr::new("Composer\\Repository\\VcsRepository"),
    );
    rm.set_repository_class(
        bstr::BStr::new("hg"),
        BStr::new("Composer\\Repository\\VcsRepository"),
    );
    rm.set_repository_class(
        bstr::BStr::new("artifact"),
        BStr::new("Composer\\Repository\\ArtifactRepository"),
    );

    let cases: Vec<(&BStr, IndexMap<BString, PhpMixed>)> = vec![
        (
            BStr::new("composer"),
            str_config(&[(
                BStr::new("url"),
                PhpMixed::string(BString::from("http://example.org")),
            )]),
        ),
        (
            BStr::new("vcs"),
            str_config(&[(
                BStr::new("url"),
                PhpMixed::string(BString::from("http://github.com/foo/bar")),
            )]),
        ),
        (
            BStr::new("git"),
            str_config(&[(
                BStr::new("url"),
                PhpMixed::string(BString::from("http://github.com/foo/bar")),
            )]),
        ),
        (
            BStr::new("git"),
            str_config(&[(
                BStr::new("url"),
                PhpMixed::string(BString::from("git@example.org:foo/bar.git")),
            )]),
        ),
        (
            BStr::new("svn"),
            str_config(&[(
                BStr::new("url"),
                PhpMixed::string(BString::from("svn://example.org/foo/bar")),
            )]),
        ),
        (
            BStr::new("package"),
            str_config(&[(BStr::new("package"), PhpMixed::Array(IndexMap::new()))]),
        ),
        (
            BStr::new("artifact"),
            str_config(&[(
                BStr::new("url"),
                PhpMixed::string(BString::from("/path/to/zips")),
            )]),
        ),
    ];

    for (r#type, options) in cases {
        rm.create_repository(
            bstr::BStr::new("composer"),
            str_config(&[(
                BStr::new("url"),
                PhpMixed::string(BString::from("http://example.org")),
            )]),
            None,
        )
        .unwrap();
        rm.create_repository(bstr::BStr::new(r#type), options, None)
            .unwrap();
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
            BStr::new("config"),
            PhpMixed::Array(str_config(&[(
                BStr::new("cache-repo-dir"),
                PhpMixed::string(path_to_bytes(&tmpdir.path())),
            )])),
        )]),
        "unknown",
    );

    let cases: Vec<(&BStr, IndexMap<BString, PhpMixed>)> = vec![
        (
            BStr::new("pear"),
            str_config(&[(
                BStr::new("url"),
                PhpMixed::string(BString::from("http://pear.example.org/foo")),
            )]),
        ),
        (BStr::new("invalid"), IndexMap::new()),
    ];

    for (r#type, options) in cases {
        assert!(
            rm.create_repository(bstr::BStr::new(r#type), options, None)
                .is_err()
        );
    }
}

#[test]
fn test_filter_repo_wrapping() {
    let SetUp { tmpdir } = set_up();
    let _tear_down = TearDown::new(tmpdir.path().to_path_buf());

    let io = null_io();
    let config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(false, None)));
    let mut rm = RepositoryManager::new(io.clone(), config, http_downloader(&io), None, None);

    rm.set_repository_class(
        bstr::BStr::new("path"),
        BStr::new("Composer\\Repository\\PathRepository"),
    );
    let repo = rm
        .create_repository(
            bstr::BStr::new("path"),
            str_config(&[
                (BStr::new("type"), PhpMixed::string(BString::from("path"))),
                (
                    BStr::new("url"),
                    PhpMixed::string(bstr::BString::from(path_to_bytes(
                        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/repository"),
                    ))),
                ),
                (
                    BStr::new("only"),
                    PhpMixed::List(vec![PhpMixed::string(BString::from("foo/bar"))]),
                ),
            ]),
            None,
        )
        .unwrap();

    assert!(repo.is::<FilterRepository>());
    let filter = repo.downcast_rc::<FilterRepository>().unwrap();
    assert!(filter.borrow().get_repository().is::<PathRepository>());
}
