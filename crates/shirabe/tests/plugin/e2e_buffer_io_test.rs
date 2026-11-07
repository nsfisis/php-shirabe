//! BufferIO E2E compatibility check: upstream Composer and Shirabe each install a fixture project
//! whose plugin builds its own `BufferIO` instances and writes what each one captured to a trace
//! file. Upstream has no test that builds a BufferIO from plugin code, so the whole fixture is
//! Shirabe-authored (`fixtures/e2e-buffer-io/`) and nothing has to be fetched; the test skips only
//! while the PHP runtime or the Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::php_worker::lock_php_worker;
use bstr::{BStr, BString};
use shirabe_php_shim::os_str_from_bytes;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e-buffer-io")
}

struct Run {
    exit_code: i32,
    trace: BString,
    formatter: BString,
}

/// Runs `install` in a fresh copy of the fixture and returns the exit code with the plugin's
/// trace and the outcome of its formatter construction.
fn install(program: &BStr, prefix_args: &[&OsStr]) -> Run {
    let work = TempDir::new().unwrap();
    copy_dir(&fixture_dir(), work.path());
    let project = work.path().join("project");
    let output = std::process::Command::new(os_str_from_bytes(&program))
        .args(prefix_args)
        .arg("install")
        .current_dir(&project)
        .env("COMPOSER_HOME", work.path().join("home"))
        .env("COMPOSER_CACHE_DIR", work.path().join("cache"))
        .env("COMPOSER_NO_INTERACTION", "1")
        .env("COLUMNS", "120")
        .env("LINES", "30")
        .output()
        .unwrap();
    Run {
        exit_code: output.status.code().unwrap_or(-1),
        trace: BString::from(
            std::fs::read(project.join("buffer-io-trace.txt")).unwrap_or_default(),
        ),
        formatter: BString::from(
            std::fs::read(project.join("buffer-io-formatter.txt")).unwrap_or_default(),
        ),
    }
}

#[test]
fn test_plugin_owned_buffer_io_matches_upstream_composer() {
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();

    let upstream = install(BStr::new("php"), &[composer_bin.as_os_str()]);
    let shirabe = install(BStr::new(env!("CARGO_BIN_EXE_shirabe")), &[]);

    assert_eq!(0, upstream.exit_code, "upstream install must succeed");
    assert_eq!(upstream.exit_code, shirabe.exit_code);
    assert_eq!(upstream.trace, shirabe.trace);

    // Pinned as well as compared, so a run where neither side wrote a trace cannot pass.
    assert_eq!(
        "\
event=post-update-cmd
default output=\"plain\\nfirst\\nsecond\\nno newline tagged\\n10%20%\\n\" verbose=false veryVerbose=false debug=false interactive=false decorated=false
very-verbose output=\"very verbose\\n\" verbose=true veryVerbose=true debug=false interactive=false decorated=false
user-inputs verbose=true veryVerbose=true debug=false interactive=true decorated=false
process code=0 output=\"captured\\n\" io=\"Executing command (CWD): echo captured\\n\" verbose=true veryVerbose=true debug=true interactive=false decorated=false
",
        upstream.trace
    );

    // TODO(plugin): an output formatter is a Symfony object built in the plugin process, and no
    // proxy carries one to the Rust side, so Shirabe answers its construction with an explicit
    // error where Composer builds the BufferIO.
    assert_eq!("constructed\n", upstream.formatter);
    assert_eq!(
        "caught=RuntimeException message=\"Shirabe does not support passing an output formatter \
         to Composer\\\\IO\\\\BufferIO inside the plugin process yet\"\n",
        shirabe.formatter
    );
}
