//! ref: composer/tests/Composer/Test/Util/HttpDownloaderTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::io::IOInterface;
use shirabe::io::buffer_io::BufferIO;
use shirabe::util::http_downloader::HttpDownloader;
use shirabe_external_packages::symfony::console::output::output_interface::VERBOSITY_NORMAL;
use shirabe_php_shim::{PHP_EOL, PhpMixed};

#[test]
#[ignore = "asserts IOInterface mock ->expects()->method('setAuthentication')->with(...) and performs a live HTTP get; no mock infrastructure exists"]
fn test_capture_authentication_params_from_url() {
    todo!()
}

#[test]
fn test_output_warnings() {
    let io: Rc<RefCell<BufferIO>> = Rc::new(RefCell::new(
        BufferIO::new(String::new(), VERBOSITY_NORMAL, None).unwrap(),
    ));
    let io_dyn: Rc<RefCell<dyn IOInterface>> = io.clone();

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
