//! ref: composer/tests/Composer/Test/Util/RemoteFilesystemTest.php

use crate::config_stub::ConfigStubBuilder;
use crate::io_stub::IOStub;
use bstr::{B, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe::io::IOInterface;
use shirabe::util::{GetResult, RemoteFilesystem};
use shirabe_php_shim::{
    PhpMixed, STREAM_NOTIFY_FAILURE, STREAM_NOTIFY_FILE_SIZE_IS, STREAM_NOTIFY_PROGRESS, parse_url,
    strpos,
};

// Mirrors RemoteFilesystemTest::getConfigMock: get('github-domains') and
// get('gitlab-domains') return [], everything else returns null. add_authentication_options
// reads gitlab-domains, so seed it as an empty list.
fn config_mock() -> std::rc::Rc<std::cell::RefCell<shirabe::config::Config>> {
    ConfigStubBuilder::new()
        .with("github-domains", PhpMixed::List(vec![]))
        .with("gitlab-domains", PhpMixed::List(vec![]))
        .build_shared()
}

// Mirrors RemoteFilesystemTest::callGetOptionsForUrl: build a RemoteFilesystem, set the
// private file_url, then invoke the private get_options_for_url with the given args.
fn call_get_options_for_url(
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    origin_url: &str,
    additional_options: IndexMap<BString, PhpMixed>,
    options: IndexMap<BString, PhpMixed>,
    file_url: &str,
) -> IndexMap<BString, PhpMixed> {
    let fs = RemoteFilesystem::new(io, config_mock(), options, false);
    fs.__set_file_url(file_url).unwrap();
    fs.__get_options_for_url(origin_url, additional_options)
        .unwrap()
}

fn http_header_list(res: &IndexMap<BString, PhpMixed>) -> Option<Vec<String>> {
    res.get(B("http"))
        .and_then(|v| v.as_array())
        .and_then(|http| http.get(B("header")))
        .and_then(|v| v.as_list())
        .map(|list| {
            list.iter()
                .map(|v| v.as_string().unwrap_or_default().to_string())
                .collect()
        })
}

#[test]
#[ignore = "the PHP RemoteFilesystem instance cannot be built in the worker: the IOStub mock has no proxy stub, construction calls IOInterface::debug, which is not served over RPC, and the AuthHelper it builds is guarded"]
fn test_get_options_for_url() {
    // TODO(plugin): the PHP RemoteFilesystem instance cannot be built in the worker yet: the
    // IOStub mock has no proxy stub, IOInterface::debug is not served over RPC, and the
    // AuthHelper the constructor builds is guarded there.
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = std::rc::Rc::new(
        std::cell::RefCell::new(IOStub::new().with_has_authentication(false)),
    );

    let res = call_get_options_for_url(
        io,
        "http://example.org",
        IndexMap::new(),
        IndexMap::new(),
        "",
    );

    assert!(
        http_header_list(&res).is_some(),
        "getOptions must return an array with headers"
    );
}

#[test]
#[ignore = "the PHP RemoteFilesystem instance cannot be built in the worker: the IOStub mock has no proxy stub, construction calls IOInterface::debug, which is not served over RPC, and the AuthHelper it builds is guarded"]
fn test_get_options_for_url_with_authorization() {
    // TODO(plugin): the PHP RemoteFilesystem instance cannot be built in the worker yet: the
    // IOStub mock has no proxy stub, IOInterface::debug is not served over RPC, and the
    // AuthHelper the constructor builds is guarded there.
    let mut auth: IndexMap<String, Option<String>> = IndexMap::new();
    auth.insert("username".to_string(), Some("login".to_string()));
    auth.insert("password".to_string(), Some("password".to_string()));
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(
            IOStub::new()
                .with_has_authentication(true)
                .with_get_authentication(auth),
        ));

    let options = call_get_options_for_url(
        io,
        "http://example.org",
        IndexMap::new(),
        IndexMap::new(),
        "",
    );

    let mut found = false;
    for header in http_header_list(&options).unwrap_or_default() {
        if strpos(&header, "Authorization: Basic") == Some(0) {
            found = true;
        }
    }
    assert!(found, "getOptions must have an Authorization header");
}

#[test]
#[ignore = "the PHP RemoteFilesystem instance cannot be built in the worker: the IOStub mock has no proxy stub, construction calls IOInterface::debug, which is not served over RPC, and the AuthHelper it builds is guarded"]
fn test_get_options_for_url_with_stream_options() {
    // TODO(plugin): the PHP RemoteFilesystem instance cannot be built in the worker yet: the
    // IOStub mock has no proxy stub, IOInterface::debug is not served over RPC, and the
    // AuthHelper the constructor builds is guarded there.
    let mut auth: IndexMap<String, Option<String>> = IndexMap::new();
    auth.insert("username".to_string(), None);
    auth.insert("password".to_string(), None);
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(
            IOStub::new()
                .with_has_authentication(true)
                .with_get_authentication(auth),
        ));

    let mut ssl: IndexMap<BString, PhpMixed> = IndexMap::new();
    ssl.insert("allow_self_signed".into(), PhpMixed::Bool(true));
    let mut stream_options: IndexMap<BString, PhpMixed> = IndexMap::new();
    stream_options.insert("ssl".into(), PhpMixed::Array(ssl));

    let res = call_get_options_for_url(
        io,
        "https://example.org",
        IndexMap::new(),
        stream_options,
        "",
    );

    let allow_self_signed = res
        .get(B("ssl"))
        .and_then(|v| v.as_array())
        .and_then(|ssl| ssl.get(B("allow_self_signed")))
        .and_then(|v| v.as_bool());
    assert_eq!(
        allow_self_signed,
        Some(true),
        "getOptions must return an array with a allow_self_signed set to true"
    );
}

#[test]
#[ignore = "the PHP RemoteFilesystem instance cannot be built in the worker: the IOStub mock has no proxy stub, construction calls IOInterface::debug, which is not served over RPC, and the AuthHelper it builds is guarded"]
fn test_get_options_for_url_with_call_options_keeps_header() {
    // TODO(plugin): the PHP RemoteFilesystem instance cannot be built in the worker yet: the
    // IOStub mock has no proxy stub, IOInterface::debug is not served over RPC, and the
    // AuthHelper the constructor builds is guarded there.
    let mut auth: IndexMap<String, Option<String>> = IndexMap::new();
    auth.insert("username".to_string(), None);
    auth.insert("password".to_string(), None);
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(
            IOStub::new()
                .with_has_authentication(true)
                .with_get_authentication(auth),
        ));

    let mut http: IndexMap<BString, PhpMixed> = IndexMap::new();
    http.insert("header".into(), PhpMixed::string("Foo: bar".to_string()));
    let mut additional_options: IndexMap<BString, PhpMixed> = IndexMap::new();
    additional_options.insert("http".into(), PhpMixed::Array(http));

    let res = call_get_options_for_url(
        io,
        "https://example.org",
        additional_options,
        IndexMap::new(),
        "",
    );

    let headers = http_header_list(&res);
    assert!(
        headers.is_some(),
        "getOptions must return an array with a http.header key"
    );
    let headers = headers.unwrap();

    let found = headers.iter().any(|header| header == "Foo: bar");
    assert!(found, "getOptions must have a Foo: bar header");
    assert!(headers.len() > 1);
}

#[test]
#[ignore = "the PHP RemoteFilesystem instance cannot be built in the worker: the IOStub mock has no proxy stub, construction calls IOInterface::debug, which is not served over RPC, and the AuthHelper it builds is guarded"]
fn test_callback_get_file_size() {
    // TODO(plugin): the PHP RemoteFilesystem instance cannot be built in the worker yet: the
    // IOStub mock has no proxy stub, IOInterface::debug is not served over RPC, and the
    // AuthHelper the constructor builds is guarded there.
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(IOStub::new()));
    let fs = RemoteFilesystem::new(io, config_mock(), IndexMap::new(), false);
    fs.__callback_get(STREAM_NOTIFY_FILE_SIZE_IS, 0, Some(String::new()), 0, 0, 20)
        .unwrap();
    assert_eq!(20, fs.__bytes_max().unwrap());
}

#[test]
#[ignore = "the PHP RemoteFilesystem instance cannot be built in the worker: the IOStub mock has no proxy stub, construction calls IOInterface::debug, which is not served over RPC, and the AuthHelper it builds is guarded"]
fn test_callback_get_notify_progress() {
    // TODO(plugin): the PHP RemoteFilesystem instance cannot be built in the worker yet: the
    // IOStub mock has no proxy stub, IOInterface::debug is not served over RPC, and the
    // AuthHelper the constructor builds is guarded there.
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(IOStub::new()));
    let fs = RemoteFilesystem::new(io, config_mock(), IndexMap::new(), false);
    fs.__set_bytes_max(20).unwrap();
    fs.__set_progress(true).unwrap();

    fs.__callback_get(STREAM_NOTIFY_PROGRESS, 0, Some(String::new()), 0, 10, 20)
        .unwrap();
    assert_eq!(Some(50), fs.__last_progress().unwrap());
}

#[test]
#[ignore = "the PHP RemoteFilesystem instance cannot be built in the worker: the IOStub mock has no proxy stub, construction calls IOInterface::debug, which is not served over RPC, and the AuthHelper it builds is guarded"]
fn test_callback_get_passes_through404() {
    // TODO(plugin): the PHP RemoteFilesystem instance cannot be built in the worker yet: the
    // IOStub mock has no proxy stub, IOInterface::debug is not served over RPC, and the
    // AuthHelper the constructor builds is guarded there.
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(IOStub::new()));
    let fs = RemoteFilesystem::new(io, config_mock(), IndexMap::new(), false);

    fs.__callback_get(
        STREAM_NOTIFY_FAILURE,
        0,
        Some("HTTP/1.1 404 Not Found".to_string()),
        404,
        0,
        0,
    )
    .unwrap();
}

// Mirrors PHP's `(string)` cast of getContents()'s string|true|false result.
fn get_result_to_string(res: &GetResult) -> String {
    match res {
        GetResult::Content(s) => s.clone(),
        GetResult::True => "1".to_string(),
        GetResult::False => String::new(),
    }
}

#[test]
#[ignore = "requires a PHPUnit mock of RemoteFilesystem overriding getRemoteContents; the instance lives in the PHP worker, which has no mock-building seam"]
fn test_copy_with_no_retry_on_failure() {
    // TODO(mock): the test replaces the protected getRemoteContents of RemoteFilesystem with a
    // PHPUnit mock. The instance lives in the PHP worker, which offers no seam to build one.
    todo!()
}

#[test]
#[ignore = "requires PHPUnit mocks of RemoteFilesystem::getRemoteContents and AuthHelper::promptAuthIfNeeded; the instances live in the PHP worker, which has no mock-building seam"]
fn test_copy_with_success_on_retry() {
    // TODO(mock): the test replaces the protected getRemoteContents of RemoteFilesystem and
    // AuthHelper::promptAuthIfNeeded with PHPUnit mocks. The instances live in the PHP worker,
    // which offers no seam to build them.
    todo!()
}

#[test]
#[ignore = "the PHP RemoteFilesystem instance cannot be built in the worker: the IOStub mock has no proxy stub, construction calls IOInterface::debug, which is not served over RPC, and the AuthHelper it builds is guarded"]
fn test_get_options_for_url_creates_secure_tls_defaults() {
    // TODO(plugin): the PHP RemoteFilesystem instance cannot be built in the worker yet: the
    // IOStub mock has no proxy stub, IOInterface::debug is not served over RPC, and the
    // AuthHelper the constructor builds is guarded there.
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(IOStub::new()));

    let mut ssl: IndexMap<BString, PhpMixed> = IndexMap::new();
    ssl.insert(
        "cafile".into(),
        PhpMixed::string("/some/path/file.crt".to_string()),
    );
    let mut additional_options: IndexMap<BString, PhpMixed> = IndexMap::new();
    additional_options.insert("ssl".into(), PhpMixed::Array(ssl));

    let res = call_get_options_for_url(
        io,
        "example.org",
        additional_options,
        IndexMap::new(),
        "http://www.example.org",
    );

    let ssl_res = res.get(B("ssl")).and_then(|v| v.as_array()).unwrap();
    let ciphers = ssl_res
        .get(B("ciphers"))
        .and_then(|v| v.as_string())
        .unwrap();
    assert!(ciphers.contains_str(
        "!aNULL:!eNULL:!EXPORT:!DES:!3DES:!RC4:!MD5:!PSK:!aECDH:!EDH-DSS-DES-CBC3-SHA:!EDH-RSA-DES-CBC3-SHA:!KRB5-DES-CBC3-SHA"
    ));
    assert_eq!(
        Some(true),
        ssl_res.get(B("verify_peer")).and_then(|v| v.as_bool())
    );
    assert_eq!(
        Some(true),
        ssl_res.get(B("SNI_enabled")).and_then(|v| v.as_bool())
    );
    assert_eq!(
        Some(7),
        ssl_res.get(B("verify_depth")).and_then(|v| v.as_int())
    );
    assert_eq!(
        Some("/some/path/file.crt".into()),
        ssl_res.get(B("cafile")).and_then(|v| v.as_string())
    );
    assert_eq!(
        Some(true),
        ssl_res
            .get(B("disable_compression"))
            .and_then(|v| v.as_bool())
    );
}

// Mirrors RemoteFilesystemTest::provideBitbucketPublicDownloadUrls.
fn provide_bitbucket_public_download_urls() -> Vec<(&'static str, &'static str)> {
    vec![(
        "https://bitbucket.org/seldaek/composer-live-test-repo/raw/master/composer-unit-test-download-me.txt",
        "1234",
    )]
}

#[test]
#[ignore = "performs a real network download; the PHP RemoteFilesystem instance cannot be built in the worker: the IOStub mock has no proxy stub, construction calls IOInterface::debug, which is not served over RPC, and the AuthHelper it builds is guarded"]
fn test_bit_bucket_public_download() {
    // TODO(plugin): the PHP RemoteFilesystem instance cannot be built in the worker yet: the
    // IOStub mock has no proxy stub, IOInterface::debug is not served over RPC, and the
    // AuthHelper the constructor builds is guarded there.
    for (url, contents) in provide_bitbucket_public_download_urls() {
        let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(IOStub::new()));
        let rfs = RemoteFilesystem::new(io, config_mock(), IndexMap::new(), false);
        let hostname = parse_url(url).and_then(|parsed| parsed.host);
        let hostname = hostname.as_deref().unwrap_or_default();

        let result = rfs
            .get_contents(hostname, url, false, IndexMap::new())
            .unwrap();

        assert_eq!(contents, get_result_to_string(&result));
    }
}
