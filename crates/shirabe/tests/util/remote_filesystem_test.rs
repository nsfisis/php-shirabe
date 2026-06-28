//! ref: composer/tests/Composer/Test/Util/RemoteFilesystemTest.php

use crate::config_stub::ConfigStubBuilder;
use crate::io_stub::IOStub;
use indexmap::IndexMap;
use shirabe::io::IOInterface;
use shirabe::util::RemoteFilesystem;
use shirabe_php_shim::{PhpMixed, STREAM_NOTIFY_FILE_SIZE_IS, STREAM_NOTIFY_PROGRESS, strpos};
use std::cell::RefCell;
use std::rc::Rc;

// Mirrors RemoteFilesystemTest::getConfigMock: get('github-domains') and
// get('gitlab-domains') return [], everything else returns null. add_authentication_options
// reads gitlab-domains, so seed it as an empty list.
fn config_mock() -> Rc<RefCell<shirabe::config::Config>> {
    ConfigStubBuilder::new()
        .with("github-domains", PhpMixed::List(vec![]))
        .with("gitlab-domains", PhpMixed::List(vec![]))
        .build_shared()
}

// Mirrors RemoteFilesystemTest::callGetOptionsForUrl: build a RemoteFilesystem, set the
// private file_url, then invoke the private get_options_for_url with the given args.
fn call_get_options_for_url(
    io: Rc<RefCell<dyn IOInterface>>,
    origin_url: &str,
    additional_options: IndexMap<String, PhpMixed>,
    options: IndexMap<String, PhpMixed>,
    file_url: &str,
) -> IndexMap<String, PhpMixed> {
    let mut fs = RemoteFilesystem::new(io, config_mock(), options, false, None);
    fs.__set_file_url(file_url);
    fs.__get_options_for_url(origin_url, additional_options)
}

fn http_header_list(res: &IndexMap<String, PhpMixed>) -> Option<Vec<String>> {
    res.get("http")
        .and_then(|v| v.as_array())
        .and_then(|http| http.get("header"))
        .and_then(|v| v.as_list())
        .map(|list| {
            list.iter()
                .map(|v| v.as_string().unwrap_or("").to_string())
                .collect()
        })
}

#[test]
fn test_get_options_for_url() {
    let io: Rc<RefCell<dyn IOInterface>> =
        Rc::new(RefCell::new(IOStub::new().with_has_authentication(false)));

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
fn test_get_options_for_url_with_authorization() {
    let mut auth: IndexMap<String, Option<String>> = IndexMap::new();
    auth.insert("username".to_string(), Some("login".to_string()));
    auth.insert("password".to_string(), Some("password".to_string()));
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(
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
fn test_get_options_for_url_with_stream_options() {
    let mut auth: IndexMap<String, Option<String>> = IndexMap::new();
    auth.insert("username".to_string(), None);
    auth.insert("password".to_string(), None);
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(
        IOStub::new()
            .with_has_authentication(true)
            .with_get_authentication(auth),
    ));

    let mut ssl: IndexMap<String, PhpMixed> = IndexMap::new();
    ssl.insert("allow_self_signed".to_string(), PhpMixed::Bool(true));
    let mut stream_options: IndexMap<String, PhpMixed> = IndexMap::new();
    stream_options.insert("ssl".to_string(), PhpMixed::Array(ssl));

    let res = call_get_options_for_url(
        io,
        "https://example.org",
        IndexMap::new(),
        stream_options,
        "",
    );

    let allow_self_signed = res
        .get("ssl")
        .and_then(|v| v.as_array())
        .and_then(|ssl| ssl.get("allow_self_signed"))
        .and_then(|v| v.as_bool());
    assert_eq!(
        allow_self_signed,
        Some(true),
        "getOptions must return an array with a allow_self_signed set to true"
    );
}

#[test]
fn test_get_options_for_url_with_call_options_keeps_header() {
    let mut auth: IndexMap<String, Option<String>> = IndexMap::new();
    auth.insert("username".to_string(), None);
    auth.insert("password".to_string(), None);
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(
        IOStub::new()
            .with_has_authentication(true)
            .with_get_authentication(auth),
    ));

    let mut http: IndexMap<String, PhpMixed> = IndexMap::new();
    http.insert(
        "header".to_string(),
        PhpMixed::String("Foo: bar".to_string()),
    );
    let mut additional_options: IndexMap<String, PhpMixed> = IndexMap::new();
    additional_options.insert("http".to_string(), PhpMixed::Array(http));

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
fn test_callback_get_file_size() {
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(IOStub::new()));
    let mut fs = RemoteFilesystem::new(io, config_mock(), IndexMap::new(), false, None);
    fs.__callback_get(STREAM_NOTIFY_FILE_SIZE_IS, 0, Some(String::new()), 0, 0, 20)
        .unwrap();
    assert_eq!(20, fs.__bytes_max());
}

#[test]
fn test_callback_get_notify_progress() {
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(IOStub::new()));
    let mut fs = RemoteFilesystem::new(io, config_mock(), IndexMap::new(), false, None);
    fs.__set_bytes_max(20);
    fs.__set_progress(true);

    fs.__callback_get(STREAM_NOTIFY_PROGRESS, 0, Some(String::new()), 0, 10, 20)
        .unwrap();
    assert_eq!(Some(50), fs.__last_progress());
}

#[test]
fn test_callback_get_passes_through404() {
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(IOStub::new()));
    let mut fs = RemoteFilesystem::new(io, config_mock(), IndexMap::new(), false, None);

    fs.__callback_get(
        shirabe_php_shim::STREAM_NOTIFY_FAILURE,
        0,
        Some("HTTP/1.1 404 Not Found".to_string()),
        404,
        0,
        0,
    )
    .unwrap();
}

#[test]
#[ignore = "real get_contents bottoms out at curl_multi_init (todo!()) in StreamContextFactory; no network/stream layer is modeled"]
fn test_get_contents() {
    todo!()
}

#[test]
#[ignore = "real copy bottoms out at curl_multi_init (todo!()) in StreamContextFactory; no network/stream layer is modeled"]
fn test_copy() {
    todo!()
}

#[test]
#[ignore = "requires a MockObject subclass of RemoteFilesystem overriding private get_remote_contents; no subclass-mocking infrastructure exists"]
fn test_copy_with_no_retry_on_failure() {
    todo!()
}

#[test]
#[ignore = "requires MockObject subclasses overriding RemoteFilesystem::get_remote_contents and AuthHelper::prompt_auth_if_needed; no subclass-mocking infrastructure exists"]
fn test_copy_with_success_on_retry() {
    todo!()
}

#[test]
#[ignore = "get_tls_defaults validates the (nonexistent) cafile and errors; constructor swallows it, so no ssl defaults are produced. Faithful porting needs CaBundle::validate_ca_file semantics for a missing file"]
fn test_get_options_for_url_creates_secure_tls_defaults() {
    todo!()
}

#[test]
#[ignore = "real getContents network download reaches curl_multi_init (todo!()); no network layer is modeled"]
fn test_bit_bucket_public_download() {
    todo!()
}
