//! ref: composer/tests/Composer/Test/Util/HttpDownloaderTest.php

use crate::config_stub::ConfigStubBuilder;
use crate::io_mock::{Expectation, get_io_mock};
use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::downloader::TransportException;
use shirabe::io::IOInterface;
use shirabe::io::buffer_io::BufferIO;
use shirabe::io::io_interface;
use shirabe::util::Platform;
use shirabe::util::http_downloader::HttpDownloader;
use shirabe_external_packages::symfony::console::output::output_interface::VERBOSITY_NORMAL;
use shirabe_php_shim::{PHP_EOL, PhpMixed};

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
                Some("pass".to_string()),
            )],
            false,
        )
        .unwrap();

    // The PHP Config mock returns [] for github-domains/gitlab-domains.
    let config: std::rc::Rc<std::cell::RefCell<Config>> = ConfigStubBuilder::new()
        .with("github-domains", PhpMixed::Array(IndexMap::new()))
        .with("gitlab-domains", PhpMixed::Array(IndexMap::new()))
        .build_shared();

    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io_mock.clone();

    Platform::put_env("COMPOSER_DISABLE_NETWORK", "1");
    let mut fs = HttpDownloader::new(io, config, IndexMap::new(), false);
    Platform::clear_env("COMPOSER_DISABLE_NETWORK");

    if let Err(e) = fs.get(
        "https://user:pass@github.com/composer/composer/404",
        IndexMap::new(),
    ) && let Some(te) = e.downcast_ref::<TransportException>()
    {
        assert_ne!(200, te.get_code());
    }
}

#[test]
fn test_output_warnings() {
    let io: std::rc::Rc<std::cell::RefCell<BufferIO>> = std::rc::Rc::new(std::cell::RefCell::new(
        BufferIO::new(String::new(), VERBOSITY_NORMAL, None).unwrap(),
    ));
    let io_dyn: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io.clone();

    HttpDownloader::output_warnings(io_dyn.clone(), "$URL", &IndexMap::new()).unwrap();
    assert_eq!("", io.borrow().get_output());

    let mut data: IndexMap<String, PhpMixed> = IndexMap::new();
    data.insert(
        "warning".to_string(),
        PhpMixed::String("old warning msg".to_string()),
    );
    data.insert(
        "warning-versions".to_string(),
        PhpMixed::String(">=2.0".to_string()),
    );
    data.insert(
        "info".to_string(),
        PhpMixed::String("old info msg".to_string()),
    );
    data.insert(
        "info-versions".to_string(),
        PhpMixed::String(">=2.0".to_string()),
    );

    let mut warning_should_not = IndexMap::new();
    warning_should_not.insert(
        "message".to_string(),
        PhpMixed::String("should not appear".to_string()),
    );
    warning_should_not.insert("versions".to_string(), PhpMixed::String("<2.2".to_string()));
    let mut warning_visible = IndexMap::new();
    warning_visible.insert(
        "message".to_string(),
        PhpMixed::String("visible warning".to_string()),
    );
    warning_visible.insert(
        "versions".to_string(),
        PhpMixed::String(">=2.2-dev".to_string()),
    );
    data.insert(
        "warnings".to_string(),
        PhpMixed::List(vec![
            PhpMixed::Array(warning_should_not),
            PhpMixed::Array(warning_visible),
        ]),
    );

    let mut info_should_not = IndexMap::new();
    info_should_not.insert(
        "message".to_string(),
        PhpMixed::String("should not appear".to_string()),
    );
    info_should_not.insert("versions".to_string(), PhpMixed::String("<2.2".to_string()));
    let mut info_visible = IndexMap::new();
    info_visible.insert(
        "message".to_string(),
        PhpMixed::String("visible info".to_string()),
    );
    info_visible.insert(
        "versions".to_string(),
        PhpMixed::String(">=2.2-dev".to_string()),
    );
    data.insert(
        "infos".to_string(),
        PhpMixed::List(vec![
            PhpMixed::Array(info_should_not),
            PhpMixed::Array(info_visible),
        ]),
    );

    HttpDownloader::output_warnings(io_dyn.clone(), "$URL", &data).unwrap();

    // the <info> tag are consumed by the OutputFormatter, but not <warning> as that is not a default output format
    assert_eq!(
        format!(
            "<warning>Warning from $URL: old warning msg</warning>{eol}\
             Info from $URL: old info msg{eol}\
             <warning>Warning from $URL: visible warning</warning>{eol}\
             Info from $URL: visible info{eol}",
            eol = PHP_EOL
        ),
        io.borrow().get_output()
    );
}
