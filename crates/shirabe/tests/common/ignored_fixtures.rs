//! Upstream fixtures that a fixture-scanning test skips.
//!
//! A test file `foo.rs` that scans an upstream fixture directory lists the fixtures it skips in
//! `foo-ignored-fixtures.json` next to it: an object mapping the fixture's path, relative to the
//! repository root (e.g. `composer/tests/Composer/Test/Fixtures/installer/foo.test`), to the
//! `TODO(<tag>): <reason>` it is skipped for.
//!
//! The test runs its fixtures with [`IgnoredFixtures::run`]. Like the data sets of a PHPUnit data
//! provider, every fixture runs even after one fails, and the result of each is reported on its
//! own line.
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

    /// Runs `run` on each case whose fixture, an absolute path, is not listed, and panics after all
    /// of them ran if any of them panicked.
    ///
    /// Prints `fixture ok: <path>` or `fixture FAILED: <path>` for each fixture that runs, with the
    /// path relative to the repository root.
    /// After a failing fixture, the current directory is restored, so that the next fixture does
    /// not run where the failed one left off.
    pub fn run<T>(
        &self,
        cases: impl IntoIterator<Item = (std::path::PathBuf, T)>,
        mut run: impl FnMut(T),
    ) {
        let mut failed = Vec::new();
        for (fixture, case) in cases {
            if self.is_ignored(&fixture) {
                continue;
            }
            let fixture = fixture.canonicalize().unwrap();
            let relative = BString::from(path_to_bytes(
                fixture.strip_prefix(&self.repository_root).unwrap(),
            ));
            let cwd = std::env::current_dir().unwrap();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(case)));
            if result.is_ok() {
                println!("fixture ok: {}", relative.display_lossy());
            } else {
                std::env::set_current_dir(cwd).unwrap();
                println!("fixture FAILED: {}", relative.display_lossy());
                failed.push(relative);
            }
        }
        assert!(
            failed.is_empty(),
            "{} fixtures failed:\n{}",
            failed.len(),
            bstr::join("\n", &failed).as_bstr().display_lossy()
        );
    }
}
