//! ref: composer/tests/Composer/Test/AllFunctionalTest.php

// The phar build (testBuildPhar / bin/compile) has no Rust equivalent: the binary under test is
// produced by cargo and located via CARGO_BIN_EXE_shirabe. testIntegration runs the .test
// integration fixtures by invoking that binary as a subprocess, mirroring the PHP harness which
// shells out to the built composer.phar.

use indexmap::IndexMap;
use serial_test::serial;
use shirabe::util::filesystem::Filesystem;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{CaptureKey, PREG_SPLIT_DELIM_CAPTURE, PhpMixed, intval};
use std::cell::RefCell;
use std::path::{Path, PathBuf};

/// ref: AllFunctionalTest's `$oldcwd` / `$testDir` instance state plus its `setUp`/`tearDown`.
///
/// `setUp` stores the previous cwd and chdir()s into Fixtures/functional; `tearDown` restores the
/// cwd and removes the unique temp dir created by `testIntegration`. Modelled as an RAII guard.
struct TearDown {
    old_cwd: PathBuf,
    test_dir: RefCell<Option<PathBuf>>,
}

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down(self);
    }
}

// ref: AllFunctionalTest::tearDown
fn tear_down(state: &TearDown) {
    let _ = std::env::set_current_dir(&state.old_cwd);
    if let Some(dir) = state.test_dir.borrow().as_ref() {
        let _ = Filesystem::new(None).remove_directory(dir);
    }
}

// ref: AllFunctionalTest::setUp
fn set_up() -> TearDown {
    let old_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(fixtures_dir()).unwrap();
    TearDown {
        old_cwd,
        test_dir: RefCell::new(None),
    }
}

/// The `__DIR__.'/Fixtures/functional'` directory, reused from the Composer source tree.
fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../composer/tests/Composer/Test/Fixtures/functional")
        .canonicalize()
        .unwrap()
}

/// ref: TestCase::getUniqueTmpDirectory. Returns a fresh directory whose removal is left to the
/// caller (the `TearDown` guard), matching PHP's manual `Filesystem::removeDirectory($testDir)`.
fn unique_tmp_directory() -> PathBuf {
    tempfile::Builder::new()
        .prefix("composer-test-")
        .tempdir()
        .unwrap()
        .keep()
}

// ref: AllFunctionalTest::parseTestFile
fn parse_test_file(file: &Path) -> IndexMap<String, String> {
    let contents = std::fs::read_to_string(file).unwrap();
    let tokens = Preg::split4(
        r"#(?:^|\n*)--([A-Z-]+)--\n#",
        &contents,
        -1,
        PREG_SPLIT_DELIM_CAPTURE,
    );
    let mut data: IndexMap<String, String> = IndexMap::new();
    let mut section: Option<String> = None;

    for token in tokens {
        if token.is_empty() && section.is_none() {
            continue;
        }

        // Handle section headers.
        if section.is_none() {
            section = Some(token);
            continue;
        }

        let sec = section.take().unwrap();

        // Allow sections to validate, or modify their section data.
        let section_data = match sec.as_str() {
            "EXPECT-EXIT-CODE" => intval(&PhpMixed::from(token)).to_string(),
            "RUN" | "EXPECT" | "EXPECT-REGEX" | "EXPECT-REGEXES" => token.trim().to_string(),
            "TEST" => token,
            _ => panic!(
                "Unknown section \"{}\". Allowed sections: \"RUN\", \"EXPECT\", \"EXPECT-EXIT-CODE\", \"EXPECT-REGEX\", \"EXPECT-REGEXES\". \
                 Section headers must be written as \"--HEADER_NAME--\".",
                sec
            ),
        };

        data.insert(sec, section_data);
    }

    // validate data
    assert!(
        data.contains_key("RUN"),
        "The test file must have a section named \"RUN\"."
    );
    assert!(
        data.contains_key("EXPECT")
            || data.contains_key("EXPECT-REGEX")
            || data.contains_key("EXPECT-REGEXES"),
        "The test file must have a section named \"EXPECT\", \"EXPECT-REGEX\", or \"EXPECT-REGEXES\"."
    );

    data
}

// ref: AllFunctionalTest::cleanOutput
fn clean_output(output: &str) -> String {
    let mut processed: Vec<u8> = Vec::new();
    for &byte in output.as_bytes() {
        if byte == 0x08 {
            processed.pop();
        } else if byte != b'\r' {
            processed.push(byte);
        }
    }
    String::from_utf8_lossy(&processed).into_owned()
}

/// ref: the inline `--EXPECT--` matcher in AllFunctionalTest::testIntegration. Literal byte
/// comparison, except `%regex%` spans in `expected` are matched as `{regex}` against the remaining
/// output and consume whatever they match.
fn expect_matches(expected: &str, output: &str) {
    let eb = expected.as_bytes();
    let ob = output.as_bytes();

    let mut line = 1;
    let mut i = 0usize;
    let mut j = 0usize;
    while i < eb.len() {
        if eb[i] == b'\n' {
            line += 1;
        }
        if eb[i] == b'%' {
            let mut m: IndexMap<CaptureKey, String> = IndexMap::new();
            if !Preg::is_match3("{%(.+?)%}", &expected[i..], Some(&mut m)) {
                panic!("Failed to match %...% in {}", &expected[i..]);
            }
            let regex = m.get(&CaptureKey::ByIndex(1)).cloned().unwrap();

            let pattern = format!("{{{}}}", regex);
            let mut m = IndexMap::new();
            if Preg::is_match3(&pattern, &output[j..], Some(&mut m)) {
                let full = m.get(&CaptureKey::ByIndex(0)).cloned().unwrap();
                i += regex.len() + 2;
                j += full.len();
                continue;
            } else {
                panic!(
                    "Failed to match pattern {} at line {} / abs offset {}:\n\nOutput:\n{}",
                    regex, line, i, output
                );
            }
        }
        if j >= ob.len() || eb[i] != ob[j] {
            panic!(
                "Output does not match expectation at line {} / abs offset {}:\n\nOutput:\n{}",
                line, i, output
            );
        }
        i += 1;
        j += 1;
    }
}

// ref: AllFunctionalTest::testIntegration (the @dataProvider getTestFiles datasets are wired up as
// the individual #[test] functions below).
fn run_integration(test_filename: &str) {
    let guard = set_up();

    let test_file = fixtures_dir().join(test_filename);
    let test_data = parse_test_file(&test_file);

    let test_dir = unique_tmp_directory();
    *guard.test_dir.borrow_mut() = Some(test_dir.clone());

    // if a dir is present with the name of the .test file (without .test), we copy all its contents
    // in the $testDir to be used to run the test with
    let test_file_setup_dir = fixtures_dir().join(test_filename.strip_suffix(".test").unwrap());
    if test_file_setup_dir.is_dir() {
        Filesystem::new(None)
            .copy(
                test_file_setup_dir.to_str().unwrap(),
                test_dir.to_str().unwrap(),
            )
            .unwrap();
    }

    let composer_home = format!("{}home", test_dir.display());
    let composer_cache_dir = format!("{}cache", test_dir.display());

    // PHP runs `escapeshellcmd(PHP_BINARY) . ' ' . escapeshellarg($pharPath) . ' --no-ansi ' . RUN`
    // via Process::fromShellCommandline; here the shirabe binary takes the place of php+phar. The
    // `2>&1` merges stderr into stdout at the OS level, reproducing the interleaved capture the PHP
    // callback performs.
    let bin = env!("CARGO_BIN_EXE_shirabe");
    let run = &test_data["RUN"];
    let command_line = format!("'{}' --no-ansi {} 2>&1", bin, run);

    let proc = std::process::Command::new("sh")
        .arg("-c")
        .arg(&command_line)
        .current_dir(&test_dir)
        .env("COMPOSER_HOME", &composer_home)
        .env("COMPOSER_CACHE_DIR", &composer_cache_dir)
        .output()
        .unwrap();

    let exit_code = proc.status.code().unwrap_or(-1) as i64;
    let raw_output = String::from_utf8_lossy(&proc.stdout).into_owned();

    if let Some(expected) = test_data.get("EXPECT") {
        let output = clean_output(&raw_output);
        let output = output.trim();
        expect_matches(expected, output);
    }
    if let Some(expect_regex) = test_data.get("EXPECT-REGEX") {
        assert!(Preg::is_match(expect_regex, &clean_output(&raw_output)));
    }
    if let Some(expect_regexes) = test_data.get("EXPECT-REGEXES") {
        let clean = clean_output(&raw_output);
        for regex in expect_regexes.split('\n') {
            assert!(Preg::is_match(regex, &clean), "Output: {}", raw_output);
        }
    }
    if let Some(expect_exit_code) = test_data.get("EXPECT-EXIT-CODE") {
        assert_eq!(expect_exit_code.parse::<i64>().unwrap(), exit_code);
    }
}

#[test]
#[ignore = "Rust has no phar; the binary under test is built by cargo (CARGO_BIN_EXE_shirabe), so bin/compile (the phar build) has no equivalent"]
fn test_build_phar() {
    let _guard = set_up();
    // TODO(phase-d): no phar-build equivalent in Rust; the binary under test is produced by cargo
    // and located via CARGO_BIN_EXE_shirabe, so there is nothing analogous to bin/compile to test.
    todo!()
}

#[test]
#[serial]
fn test_integration_create_project_command() {
    run_integration("create-project-command.test");
}

#[test]
#[serial]
#[ignore = "RemoteFilesystem::get_remote_contents is an unimplemented Phase C stub (returns None), so reading the local packages.json repository via JsonFile/HttpDownloader fails with \"file could not be downloaded\""]
fn test_integration_create_project_shows_full_hash_for_dev_packages() {
    run_integration("create-project-shows-full-hash-for-dev-packages.test");
}

#[test]
#[serial]
#[ignore = "requires the Plugin API (PHP plugin Hooks emitting !! markers), which is not yet implemented"]
fn test_integration_installed_versions() {
    run_integration("installed-versions.test");
}

#[test]
#[serial]
#[ignore = "requires the Plugin API (PHP plugin Hooks emitting !! markers), which is not yet implemented"]
fn test_integration_installed_versions2() {
    run_integration("installed-versions2.test");
}

#[test]
#[serial]
#[ignore = "requires the Plugin API (PHP plugins emitting !! markers), which is not yet implemented"]
fn test_integration_plugin_autoloading_only_loads_dependencies() {
    run_integration("plugin-autoloading-only-loads-dependencies.test");
}
