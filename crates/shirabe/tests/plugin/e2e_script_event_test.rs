//! Script event E2E compatibility check: upstream Composer and Shirabe each run `dump-autoload`
//! in a fixture project whose `post-autoload-dump` script is a PHP static method, and the method
//! records what the `Composer\Script\Event` it receives exposes. Upstream has no test that reads
//! the Composer object graph out of an event from a script, so the whole fixture is
//! Shirabe-authored (`fixtures/e2e-script-event/`) and nothing has to be fetched; the test skips
//! only while the PHP runtime or the Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::php_worker::lock_php_worker;
use bstr::{BStr, BString, ByteSlice as _};
use shirabe_php_shim::os_str_from_bytes;

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e-script-event")
}

struct Run {
    exit_code: i32,
    trace: BString,
    stdout: BString,
}

/// Runs `dump-autoload` in a fresh copy of the fixture and returns the exit code with what the
/// script wrote to its trace file and through the event's IO.
fn dump_autoload(program: &BStr, prefix_args: &[&std::ffi::OsStr]) -> Run {
    let work = tempfile::TempDir::new().unwrap();
    copy_dir(&fixture_dir(), work.path());
    let project = work.path().join("project");
    let output = std::process::Command::new(os_str_from_bytes(&program))
        .args(prefix_args)
        .arg("dump-autoload")
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
            std::fs::read(project.join("script-event-trace.txt")).unwrap_or_default(),
        ),
        stdout: BString::from(output.stdout),
    }
}

#[test]
fn test_script_event_object_graph_matches_upstream_composer() {
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();

    let upstream = dump_autoload(BStr::new("php"), &[composer_bin.as_os_str()]);
    let shirabe = dump_autoload(BStr::new(env!("CARGO_BIN_EXE_shirabe")), &[]);

    assert_eq!(0, upstream.exit_code, "upstream dump-autoload must succeed");
    assert_eq!(upstream.exit_code, shirabe.exit_code);
    assert_eq!(upstream.trace, shirabe.trace);

    // Pinned as well as compared, so a run where neither side dispatches the script cannot pass.
    assert_eq!(
        "\
event=post-autoload-dump
dev=0
root=shirabe/e2e-script-event
vendor-dir=vendor
bin-dir=bin
",
        upstream.trace
    );

    #[expect(
        clippy::disallowed_methods,
        reason = "the test splits a child process's output into lines, which no PHP code does"
    )]
    let io_line = |run: &Run| -> BString {
        BString::from(
            run.stdout
                .lines()
                .find(|line| line.starts_with(b"script-event: "))
                .unwrap_or_default(),
        )
    };
    assert_eq!(io_line(&upstream), io_line(&shirabe));
    assert_eq!(
        "script-event: event=post-autoload-dump dev=0 root=shirabe/e2e-script-event \
         vendor-dir=vendor bin-dir=bin",
        io_line(&upstream)
    );
}
