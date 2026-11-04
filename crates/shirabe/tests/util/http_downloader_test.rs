//! ref: composer/tests/Composer/Test/Util/HttpDownloaderTest.php sha1:9bc6a3fab2209e8ba0ad56c89f921f6b42af94ee

use crate::config_stub::ConfigStubBuilder;
use crate::io_mock::{Expectation, get_io_mock};
use bstr::BString;
use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::downloader::TransportException;
use shirabe::io::buffer_io::BufferIO;
use shirabe::io::{IOInterface, io_interface};
use shirabe::util::Platform;
use shirabe::util::http_downloader::HttpDownloader;
use shirabe_php_shim::{Catch as _, PHP_EOL, PhpMixed, bconcat};
use shirabe_symfony_console::output::VERBOSITY_NORMAL;

// PHP performs a live HTTP get to assert the URL's user:pass is captured via
// setAuthentication. The credential capture happens in `add_job`, before any
// network I/O, so COMPOSER_DISABLE_NETWORK short-circuits the actual request
// (yielding a non-200 TransportException, as PHP's live 404 would) while the
// setAuthentication side effect still runs and is verified through the IOMock.
#[test]
#[serial_test::serial]
fn test_capture_authentication_params_from_url() {
    let (io_mock, _io_guard) = get_io_mock(io_interface::NORMAL).unwrap();
    io_mock
        .borrow_mut()
        .expects(
            vec![Expectation::auth(
                "github.com",
                "user",
                Some(BString::from("pass")),
            )],
            false,
        )
        .unwrap();

    // The PHP Config mock returns [] for github-domains/gitlab-domains.
    let config: std::rc::Rc<std::cell::RefCell<Config>> = ConfigStubBuilder::new()
        .with("github-domains", PhpMixed::Array(IndexMap::new()))
        .with("gitlab-domains", PhpMixed::Array(IndexMap::new()))
        .build_shared();

    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io_mock;

    Platform::put_env("COMPOSER_DISABLE_NETWORK", "1").unwrap();
    let fs = HttpDownloader::new(io, config, IndexMap::new(), false).unwrap();
    Platform::clear_env("COMPOSER_DISABLE_NETWORK").unwrap();

    if let Err(e) = fs.get(
        "https://user:pass@github.com/composer/composer/404",
        IndexMap::new(),
    ) && let Some(te) = e.catch::<TransportException>()
    {
        assert_ne!(200, te.get_code());
    }
}

#[test]
fn test_output_warnings() {
    let io: std::rc::Rc<std::cell::RefCell<BufferIO>> = std::rc::Rc::new(std::cell::RefCell::new(
        BufferIO::new(BString::default(), VERBOSITY_NORMAL, None).unwrap(),
    ));
    let io_dyn: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io.clone();

    HttpDownloader::output_warnings(io_dyn.clone(), "$URL", &IndexMap::new()).unwrap();
    assert_eq!("", io.borrow().get_output());

    let mut data: IndexMap<BString, PhpMixed> = IndexMap::new();
    data.insert(
        "warning".into(),
        PhpMixed::string(BString::from("old warning msg")),
    );
    data.insert(
        "warning-versions".into(),
        PhpMixed::string(BString::from(">=2.0")),
    );
    data.insert(
        "info".into(),
        PhpMixed::string(BString::from("old info msg")),
    );
    data.insert(
        "info-versions".into(),
        PhpMixed::string(BString::from(">=2.0")),
    );

    let mut warning_should_not = IndexMap::new();
    warning_should_not.insert(
        "message".into(),
        PhpMixed::string(BString::from("should not appear")),
    );
    warning_should_not.insert("versions".into(), PhpMixed::string(BString::from("<2.2")));
    let mut warning_visible = IndexMap::new();
    warning_visible.insert(
        "message".into(),
        PhpMixed::string(BString::from("visible warning")),
    );
    warning_visible.insert(
        "versions".into(),
        PhpMixed::string(BString::from(">=2.2-dev")),
    );
    data.insert(
        "warnings".into(),
        PhpMixed::List(vec![
            PhpMixed::Array(warning_should_not),
            PhpMixed::Array(warning_visible),
        ]),
    );

    let mut info_should_not = IndexMap::new();
    info_should_not.insert(
        "message".into(),
        PhpMixed::string(BString::from("should not appear")),
    );
    info_should_not.insert("versions".into(), PhpMixed::string(BString::from("<2.2")));
    let mut info_visible = IndexMap::new();
    info_visible.insert(
        "message".into(),
        PhpMixed::string(BString::from("visible info")),
    );
    info_visible.insert(
        "versions".into(),
        PhpMixed::string(BString::from(">=2.2-dev")),
    );
    data.insert(
        "infos".into(),
        PhpMixed::List(vec![
            PhpMixed::Array(info_should_not),
            PhpMixed::Array(info_visible),
        ]),
    );

    HttpDownloader::output_warnings(io_dyn.clone(), "$URL", &data).unwrap();

    // the <info> tag are consumed by the OutputFormatter, but not <warning> as that is not a default output format
    assert_eq!(
        bconcat!(
            "<warning>Warning from $URL: old warning msg</warning>",
            PHP_EOL,
            "\
             Info from $URL: old info msg",
            PHP_EOL,
            "\
             <warning>Warning from $URL: visible warning</warning>",
            PHP_EOL,
            "\
             Info from $URL: visible info",
            PHP_EOL
        ),
        io.borrow().get_output()
    );
}

// Not in Composer's test suite: the constructor throws the TransportException of
// StreamContextFactory::getTlsDefaults(), which Factory::createHttpDownloader() catches.
#[test]
fn test_constructor_rejects_an_unreadable_cafile() {
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = std::rc::Rc::new(
        std::cell::RefCell::new(BufferIO::new("", VERBOSITY_NORMAL, None).unwrap()),
    );
    let config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(false, None)));
    let mut ssl = IndexMap::new();
    ssl.insert(
        BString::from("cafile"),
        PhpMixed::string("/nonexistent/shirabe/cafile.pem"),
    );
    let mut options = IndexMap::new();
    options.insert(BString::from("ssl"), PhpMixed::Array(ssl));

    let error = HttpDownloader::new(io, config, options, false).unwrap_err();

    assert_eq!(
        "The configured cafile was not valid or could not be read.",
        error.catch::<TransportException>().unwrap().get_message()
    );
}
