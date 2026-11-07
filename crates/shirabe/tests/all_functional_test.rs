//! ref: composer/tests/Composer/Test/AllFunctionalTest.php sha1:fc0a05aadf7b6357625d063090ec1024ebd63948

// The phar build (testBuildPhar / bin/compile) has no Rust equivalent: the binary under test is
// produced by cargo and located via CARGO_BIN_EXE_shirabe. testIntegration runs the .test
// integration fixtures by invoking that binary as a subprocess, mirroring the PHP harness which
// shells out to the built composer.phar.

#[path = "common/bootstrap.rs"]
mod bootstrap;
#[path = "common/test_case.rs"]
mod test_case;

use crate::test_case::get_unique_tmp_directory;
use bstr::{B, BStr, BString, ByteSlice};
use indexmap::IndexMap;
use serial_test::serial;
use shirabe::util::Filesystem;
use shirabe_php_shim::{
    PhpMixed, bconcat, intval, os_str_from_bytes, path_from_bytes, php_regex, preg_match,
    preg_split_delim_capture, trim, utf8_text,
};
use std::path::{Path, PathBuf};

/// ref: AllFunctionalTest's `$oldcwd` / `$testDir` instance state plus its `setUp`/`tearDown`.
///
/// `setUp` stores the previous cwd and chdir()s into Fixtures/functional; `tearDown` restores the
/// cwd and removes the unique temp dir created by `testIntegration`. Modelled as an RAII guard.
struct TearDown {
    old_cwd: PathBuf,
    test_dir: std::cell::RefCell<Option<PathBuf>>,
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
        test_dir: std::cell::RefCell::new(None),
    }
}

/// The `__DIR__.'/Fixtures/functional'` directory, reused from the Composer source tree.
fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../composer/tests/Composer/Test/Fixtures/functional")
        .canonicalize()
        .unwrap()
}

// ref: AllFunctionalTest::parseTestFile
fn parse_test_file(file: &Path) -> IndexMap<BString, BString> {
    let contents = std::fs::read(file).unwrap();
    let tokens = preg_split_delim_capture(php_regex!(r"#(?:^|\n*)--([A-Z-]+)--\n#"), &contents);
    let mut data: IndexMap<BString, BString> = IndexMap::new();
    let mut section: Option<BString> = None;

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
        let section_data = match sec.as_bytes() {
            b"EXPECT-EXIT-CODE" => bconcat!(intval(&PhpMixed::from(token))),
            b"RUN" | b"EXPECT" | b"EXPECT-REGEX" | b"EXPECT-REGEXES" => trim(&token),
            b"TEST" => token,
            _ => panic!(
                "Unknown section \"{}\". Allowed sections: \"RUN\", \"EXPECT\", \"EXPECT-EXIT-CODE\", \"EXPECT-REGEX\", \"EXPECT-REGEXES\". \
                 Section headers must be written as \"--HEADER_NAME--\".",
                sec.display_lossy()
            ),
        };

        data.insert(sec, section_data);
    }

    // validate data
    assert!(
        data.contains_key(B("RUN")),
        "The test file must have a section named \"RUN\"."
    );
    assert!(
        data.contains_key(B("EXPECT"))
            || data.contains_key(B("EXPECT-REGEX"))
            || data.contains_key(B("EXPECT-REGEXES")),
        "The test file must have a section named \"EXPECT\", \"EXPECT-REGEX\", or \"EXPECT-REGEXES\"."
    );

    data
}

// ref: AllFunctionalTest::cleanOutput
fn clean_output(output: &[u8]) -> BString {
    let mut processed: Vec<u8> = Vec::new();
    for &byte in output {
        if byte == 0x08 {
            processed.pop();
        } else if byte != b'\r' {
            processed.push(byte);
        }
    }
    processed.into()
}

/// The fixtures come from Composer's tree and name the `composer` binary in the hints they expect
/// on stdout; Shirabe prints its own name there.
fn rename_binary_in_expected_output(expected: &[u8]) -> BString {
    regex::bytes::Regex::new(r"\bcomposer (?P<cmd>config|fund|suggest|update)\b")
        .unwrap()
        .replace_all(expected, &b"shirabe $cmd"[..])
        .into_owned()
        .into()
}

/// ref: the inline `--EXPECT--` matcher in AllFunctionalTest::testIntegration. Literal byte
/// comparison, except `%regex%` spans in `expected` are matched as `{regex}` against the remaining
/// output and consume whatever they match.
fn expect_matches(expected: &[u8], output: &[u8]) {
    let eb = expected;
    let ob = output;

    let mut line = 1;
    let mut i = 0usize;
    let mut j = 0usize;
    while i < eb.len() {
        if eb[i] == b'\n' {
            line += 1;
        }
        if eb[i] == b'%' {
            let Some(m) = preg_match(php_regex!("{%(.+?)%}"), &expected[i..]) else {
                panic!(
                    "Failed to match %...% in {}",
                    expected[i..].as_bstr().display_lossy()
                );
            };
            let regex = m.get(1).unwrap().to_owned();

            let pattern = bconcat!("{", regex, "}");
            if let Some(m) = preg_match(&pattern, &output[j..]) {
                let full = m.get(0).unwrap().to_owned();
                i += regex.len() + 2;
                j += full.len();
                continue;
            } else {
                panic!(
                    "Failed to match pattern {} at line {} / abs offset {}:\n\nOutput:\n{}",
                    regex.display_lossy(),
                    line,
                    i,
                    output.as_bstr().display_lossy()
                );
            }
        }
        if j >= ob.len() || eb[i] != ob[j] {
            panic!(
                "Output does not match expectation at line {} / abs offset {}:\n\nOutput:\n{}",
                line,
                i,
                output.as_bstr().display_lossy()
            );
        }
        i += 1;
        j += 1;
    }
}

// ref: AllFunctionalTest::testIntegration (the @dataProvider getTestFiles datasets are wired up as
// the individual #[test] functions below).
fn run_integration(test_filename: &BStr) {
    let guard = set_up();

    let test_file = fixtures_dir().join(path_from_bytes(test_filename));
    let test_data = parse_test_file(&test_file);

    let test_dir = get_unique_tmp_directory().keep();
    *guard.test_dir.borrow_mut() = Some(test_dir.clone());

    // if a dir is present with the name of the .test file (without .test), we copy all its contents
    // in the $testDir to be used to run the test with
    let test_file_setup_dir = fixtures_dir().join(path_from_bytes(
        test_filename.strip_suffix(b".test").unwrap(),
    ));
    if test_file_setup_dir.is_dir() {
        Filesystem::new(None)
            .copy(&test_file_setup_dir, &test_dir)
            .unwrap();
    }

    let composer_home = bconcat!(test_dir, "home");
    let composer_cache_dir = bconcat!(test_dir, "cache");

    // PHP runs `escapeshellcmd(PHP_BINARY) . ' ' . escapeshellarg($pharPath) . ' --no-ansi ' . RUN`
    // via Process::fromShellCommandline; here the shirabe binary takes the place of php+phar. The
    // `2>&1` merges stderr into stdout at the OS level, reproducing the interleaved capture the PHP
    // callback performs.
    let bin = env!("CARGO_BIN_EXE_shirabe");
    let run = &test_data[B("RUN")];
    let command_line = bconcat!("'", bin, "' --no-ansi ", run, " 2>&1");

    let proc = std::process::Command::new("sh")
        .arg("-c")
        .arg(os_str_from_bytes(&command_line))
        .current_dir(&test_dir)
        .env("COMPOSER_HOME", os_str_from_bytes(&composer_home))
        .env("COMPOSER_CACHE_DIR", os_str_from_bytes(&composer_cache_dir))
        .output()
        .unwrap();

    let exit_code = proc.status.code().unwrap_or(-1) as i64;
    let raw_output = proc.stdout;

    if let Some(expected) = test_data.get(B("EXPECT")) {
        let expected = rename_binary_in_expected_output(expected);
        let output = clean_output(&raw_output);
        let output = trim(&output);
        expect_matches(&expected, &output);
    }
    if let Some(expect_regex) = test_data.get(B("EXPECT-REGEX")) {
        assert!(preg_match(expect_regex, &clean_output(&raw_output)).is_some());
    }
    if let Some(expect_regexes) = test_data.get(B("EXPECT-REGEXES")) {
        let clean = clean_output(&raw_output);
        for regex in expect_regexes.split_str("\n") {
            assert!(
                preg_match(regex.as_bstr(), &clean).is_some(),
                "Output: {}",
                raw_output.as_bstr().display_lossy()
            );
        }
    }
    if let Some(expect_exit_code) = test_data.get(B("EXPECT-EXIT-CODE")) {
        assert_eq!(
            utf8_text(expect_exit_code).unwrap().parse::<i64>().unwrap(),
            exit_code
        );
    }
}

#[test]
#[ignore = "Rust has no phar; the binary under test is built by cargo (CARGO_BIN_EXE_shirabe), so bin/compile (the phar build) has no equivalent"]
fn test_build_phar() {
    let _guard = set_up();
    // TODO(distribution): no phar-build equivalent in Rust; the binary under test is produced by cargo
    // and located via CARGO_BIN_EXE_shirabe, so there is nothing analogous to bin/compile to test.
    todo!()
}

#[test]
#[serial]
fn test_integration_create_project_command() {
    run_integration(BStr::new("create-project-command.test"));
}

#[test]
#[serial]
fn test_integration_create_project_shows_full_hash_for_dev_packages() {
    run_integration(BStr::new(
        "create-project-shows-full-hash-for-dev-packages.test",
    ));
}

#[test]
#[serial]
#[ignore = "the fixture's Hooks read Composer\\InstalledVersions, which is not autoloadable in the plugin worker (the fixture ships no vendor/ directory), so the pre-update hook aborts with `Class \"Composer\\InstalledVersions\" not found`"]
fn test_integration_installed_versions() {
    run_integration(BStr::new("installed-versions.test"));
}

#[test]
#[serial]
#[ignore = "the plugin worker's Composer\\InstalledVersions is only reloaded on a repository write, never seeded from the project's installed.php at boot, so the fixture's plugins fail with `Package \"plugin/a\" is not installed`"]
fn test_integration_installed_versions2() {
    run_integration(BStr::new("installed-versions2.test"));
}

#[test]
#[serial]
fn test_integration_plugin_autoloading_only_loads_dependencies() {
    run_integration(BStr::new("plugin-autoloading-only-loads-dependencies.test"));
}
