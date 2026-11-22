//! Upstream fixtures that a fixture-scanning test skips.
//!
//! A test file `foo.rs` that scans an upstream fixture directory lists the fixtures it skips in
//! `foo-ignored-fixtures.json` next to it: an object mapping the fixture's path, relative to the
//! repository root (e.g. `composer/tests/Composer/Test/Fixtures/installer/foo.test`), to the
//! `TODO(<tag>): <reason>` it is skipped for.
#![allow(dead_code)]

use bstr::{BStr, BString, ByteSlice as _};
use indexmap::IndexMap;
use shirabe_php_shim::{bformat, path_from_bytes, path_to_bytes};

#[derive(Debug)]
pub struct IgnoredFixtures {
    repository_root: std::path::PathBuf,
    entries: IndexMap<BString, BString>,
}

fn repository_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

impl IgnoredFixtures {
    /// Loads the list of the test file `test_file`, given as `BStr::new(file!())`.
    pub fn load(test_file: &BStr) -> Self {
        let repository_root = repository_root();
        let test_file = test_file.strip_suffix(b".rs").unwrap();
        let path = repository_root.join(path_from_bytes(&bformat!(
            "{}-ignored-fixtures.json",
            test_file.as_bstr()
        )));
        let json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();

        let mut entries = IndexMap::new();
        for (fixture, reason) in json.as_object().unwrap() {
            assert!(
                repository_root.join(fixture).is_file(),
                "{fixture} does not exist"
            );
            entries.insert(
                BString::from(fixture.as_str()),
                BString::from(reason.as_str().unwrap()),
            );
        }

        IgnoredFixtures {
            repository_root,
            entries,
        }
    }

    /// Whether the fixture at `fixture`, an absolute path, is to be skipped.
    pub fn is_ignored(&self, fixture: &std::path::Path) -> bool {
        let fixture = fixture.canonicalize().unwrap();
        let key = fixture.strip_prefix(&self.repository_root).unwrap();
        self.entries.contains_key(path_to_bytes(key).as_bstr())
    }
}
