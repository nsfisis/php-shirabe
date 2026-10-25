//! ref: composer/tests/Composer/Test/Util/RemoteFilesystemTest.php
//!
//! The local reads of `RemoteFilesystemTest` (`testGetContents`, `testCopy`), which `HttpDownloader`
//! hands to `LocalFilesystem` rather than to `RemoteFilesystem`.

use crate::config_stub::ConfigStubBuilder;
use crate::io_stub::IOStub;
use indexmap::IndexMap;
use shirabe::io::IOInterface;
use shirabe::util::{GetResult, LocalFilesystem};
use shirabe_php_shim::{PhpMixed, file_get_contents, strpos, unlink};

// Mirrors RemoteFilesystemTest::getConfigMock: get('github-domains') and
// get('gitlab-domains') return [], everything else returns null.
fn config_mock() -> std::rc::Rc<std::cell::RefCell<shirabe::config::Config>> {
    ConfigStubBuilder::new()
        .with("github-domains", PhpMixed::List(vec![]))
        .with("gitlab-domains", PhpMixed::List(vec![]))
        .build_shared()
}

// Mirrors PHP's `(string)` cast of getContents()'s string|true|false result.
fn get_result_to_string(res: &GetResult) -> String {
    match res {
        GetResult::Content(s) => s.clone(),
        GetResult::True => "1".to_string(),
        GetResult::False => String::new(),
    }
}

// Mirrors PHP's `__FILE__`: the path to this test source file.
fn this_file() -> String {
    format!(
        "{}/tests/util/local_filesystem_test.rs",
        env!("CARGO_MANIFEST_DIR")
    )
}

#[test]
fn test_get_contents() {
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(IOStub::new()));
    let fs = LocalFilesystem::new(io, config_mock());

    let res = fs
        .get_contents(
            "http://example.org",
            &format!("file://{}", this_file()),
            false,
            IndexMap::new(),
        )
        .unwrap();
    assert!(strpos(&get_result_to_string(&res), "testGetContents").is_some());
}

#[test]
fn test_copy() {
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(IOStub::new()));
    let fs = LocalFilesystem::new(io, config_mock());

    let file = tempfile::NamedTempFile::new().unwrap();
    let file = file.path().to_str().unwrap().to_string();
    assert!(matches!(
        fs.copy(
            "http://example.org",
            &format!("file://{}", this_file()),
            &file,
            false,
            IndexMap::new()
        )
        .unwrap(),
        GetResult::True
    ));
    assert!(std::path::Path::new(&file).exists());
    // TODO(bytes)
    assert!(
        strpos(
            &String::from_utf8_lossy(&file_get_contents(&file).unwrap_or_default()),
            "testCopy"
        )
        .is_some()
    );
    unlink(&file);
}
