//! ref: composer/tests/Composer/Test/Mock/HttpDownloaderMock.php
#![allow(dead_code)]

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::util::http_downloader::{
    HttpDownloader, HttpDownloaderMockExpectation, HttpDownloaderMockHandler,
};
use shirabe_php_shim::PhpMixed;

// A single HTTP request expectation as written in the PHP tests: a `url` plus an
// optional response (`status`/`body`/`headers`). `options` of `None` matches any
// options; `Some(..)` requires an exact match against the executed options.
pub fn expect(url: impl Into<String>) -> HttpDownloaderMockExpectation {
    HttpDownloaderMockExpectation {
        url: url.into(),
        options: None,
        status: 200,
        body: String::new(),
        headers: vec![String::new()],
    }
}

pub fn expect_full(
    url: impl Into<String>,
    options: Option<IndexMap<String, PhpMixed>>,
    status: i64,
    body: impl Into<String>,
    headers: Vec<String>,
) -> HttpDownloaderMockExpectation {
    HttpDownloaderMockExpectation {
        url: url.into(),
        options,
        status,
        body: body.into(),
        headers,
    }
}

pub struct HttpDownloaderMockGuard(Rc<RefCell<HttpDownloader>>);

impl Drop for HttpDownloaderMockGuard {
    fn drop(&mut self) {
        // Avoid aborting on a double panic when a test assertion is already unwinding.
        if std::thread::panicking() {
            return;
        }
        self.0.borrow().__assert_complete();
    }
}

// For testing only. Mirrors TestCase::getHttpDownloaderMock: returns a shared
// HttpDownloader handle configured with the given expectations, plus a guard that
// runs `__assert_complete` when it drops at the end of the test scope.
pub fn get_http_downloader_mock(
    expectations: Vec<HttpDownloaderMockExpectation>,
    strict: bool,
    default_handler: HttpDownloaderMockHandler,
) -> (Rc<RefCell<HttpDownloader>>, HttpDownloaderMockGuard) {
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let config = Rc::new(RefCell::new(Config::new(false, None)));
    let downloader = Rc::new(RefCell::new(HttpDownloader::__new_mock(io, config)));
    downloader
        .borrow_mut()
        .__expects(expectations, strict, default_handler);
    (downloader.clone(), HttpDownloaderMockGuard(downloader))
}
