//! HttpDownloader E2E compatibility check: upstream Composer and Shirabe each install a fixture
//! project whose plugin builds its own `HttpDownloader` and writes what every call on it reports
//! to a trace file. Upstream has no test that drives a downloader from plugin code, so the whole
//! fixture is Shirabe-authored (`fixtures/e2e-http-downloader/`) and nothing has to be fetched;
//! the test skips only while the PHP runtime or the Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::php_worker::{lock_php_worker, php_runtime_available};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e-http-downloader")
}

struct Run {
    exit_code: i32,
    trace: String,
}

/// Runs `install` in a fresh copy of the fixture and returns the exit code with the plugin's trace.
fn install(program: &str, prefix_args: &[&str]) -> Run {
    let work = TempDir::new().unwrap();
    copy_dir(&fixture_dir(), work.path());
    let project = work.path().join("project");
    let output = std::process::Command::new(program)
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
        trace: std::fs::read_to_string(project.join("http-downloader-trace.txt"))
            .unwrap_or_default(),
    }
}

#[test]
fn test_plugin_owned_http_downloader_matches_upstream_composer() {
    if !php_runtime_available() {
        return;
    }
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();
    let composer_bin = composer_bin.to_str().unwrap().to_string();

    let upstream = install("php", &[composer_bin.as_str()]);
    let shirabe = install(env!("CARGO_BIN_EXE_shirabe"), &[]);

    assert_eq!(0, upstream.exit_code, "upstream install must succeed");
    assert_eq!(upstream.exit_code, shirabe.exit_code);
    assert_eq!(upstream.trace, shirabe.trace);

    // Pinned as well as compared, so a run where neither side wrote a trace cannot pass. The
    // second options line is the evidence that both worlds merge into one value rather than each
    // holding its own copy of the map, and the response lines are the evidence that the value the
    // request answers with is a real instance of the class rather than something shaped like one.
    assert_eq!(
        "\
event=post-update-cmd
class=\"Composer\\\\Util\\\\HttpDownloader\" instanceof=true
options header=[\"X-Probe: 1\"]
options merged=[\"X-Probe: 2\"]
isCurlEnabled=true
hints other=null transport=null
outputWarnings=ok
get=ok
response class=\"Composer\\\\Util\\\\Http\\\\Response\" body=\"{\\\"probe\\\":true,\\\"n\\\":42}\" headers=[]
getHeader=null
copy=ok file=\"{\\\"probe\\\":true,\\\"n\\\":42}\"
add-before-enable=LogicException: You must use the HttpDownloader instance which is part of a Composer\\Loop instance to be able to run async http requests
add=ok body=\"{\\\"probe\\\":true,\\\"n\\\":42}\"
add-missing=ok rejected=\"Composer\\\\Downloader\\\\TransportException\"
addCopy=ok file=\"{\\\"probe\\\":true,\\\"n\\\":42}\"
countActiveJobs=0 wait=ok
collect=ok
",
        upstream.trace
    );
}
