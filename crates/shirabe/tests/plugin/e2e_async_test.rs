//! Async surface E2E compatibility check: upstream Composer and Shirabe each install a fixture
//! project whose plugin starts child processes through `ProcessExecutor::executeAsync()`, waits
//! on them through its own `Loop` and the run's, cancels them, and installs a package through an
//! installer that returns promises which are still pending. What the plugin observes at each
//! point is written to trace files. Upstream has no test that drives the async surface from
//! plugin code, so the whole fixture is Shirabe-authored (`fixtures/e2e-async/`) and nothing has
//! to be fetched; the test skips only while the PHP runtime or the Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::php_worker::lock_php_worker;
use bstr::{BStr, BString, ByteSlice as _};
use shirabe_php_shim::{bformat, os_str_from_bytes, path_from_bytes, utf8_text};

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e-async")
}

struct Run {
    exit_code: i32,
    output: bstr::BString,
    trace: BString,
    installer_trace: BString,
    asset: BString,
}

/// PHP's built-in web server over the fixture's `server/` directory, killed when dropped. A
/// request to it is what the curl path of `HttpDownloader` needs: an `http(s)://` URL that
/// resolves without the network.
struct HttpServer {
    child: std::process::Child,
    base_url: BString,
}

impl HttpServer {
    fn start() -> Self {
        // The port is free at the moment the listener is dropped, which is as close as an
        // externally started server gets to binding port 0.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let address = bformat!("127.0.0.1:{port}");
        let child = std::process::Command::new("php")
            .arg("-S")
            .arg(os_str_from_bytes(&address))
            .arg("-t")
            .arg(fixture_dir().join("server"))
            // The two requests of a group are in flight together, and a cancelled one keeps its
            // worker busy until the script it reached returns.
            .env("PHP_CLI_SERVER_WORKERS", "4")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let server = Self {
            child,
            base_url: bformat!("http://{address}"),
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::net::TcpStream::connect(
            utf8_text(&address).expect("the fixture server prints a text address"),
        )
        .is_err()
        {
            assert!(
                std::time::Instant::now() < deadline,
                "the fixture HTTP server did not come up on {address}",
                address = address.display_lossy()
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        server
    }
}

impl Drop for HttpServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Runs `install` in a fresh copy of the fixture and returns the exit code with the plugin's
/// traces.
fn install(program: &BStr, prefix_args: &[&std::ffi::OsStr], http_base_url: &BStr) -> Run {
    let work = tempfile::TempDir::new().unwrap();
    copy_dir(&fixture_dir(), work.path());
    let project = work.path().join("project");
    let output = std::process::Command::new(os_str_from_bytes(&program))
        .args(prefix_args)
        .arg("install")
        .current_dir(&project)
        .env("COMPOSER_HOME", work.path().join("home"))
        .env("COMPOSER_CACHE_DIR", work.path().join("cache"))
        .env("COMPOSER_NO_INTERACTION", "1")
        .env("SHIRABE_E2E_ASYNC_HTTP", os_str_from_bytes(&http_base_url))
        .env("COLUMNS", "120")
        .env("LINES", "30")
        .output()
        .unwrap();
    let read = |name: &BStr| {
        BString::from(std::fs::read(project.join(path_from_bytes(name))).unwrap_or_default())
    };
    Run {
        exit_code: output.status.code().unwrap_or(-1),
        output: bformat!("{}{}", &output.stdout, &output.stderr),
        trace: read(BStr::new("async-trace.txt")),
        installer_trace: read(BStr::new("async-installer-trace.txt")),
        asset: read(BStr::new("assets/shirabe-test/async-asset-a/asset.txt")),
    }
}

#[test]
fn test_async_surface_matches_upstream_composer() {
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();

    let server = HttpServer::start();
    let upstream = install(
        BStr::new("php"),
        &[composer_bin.as_os_str()],
        server.base_url.as_bstr(),
    );
    let shirabe = install(
        BStr::new(env!("CARGO_BIN_EXE_shirabe")),
        &[],
        server.base_url.as_bstr(),
    );

    assert_eq!(
        0,
        upstream.exit_code,
        "upstream install must succeed: {}",
        upstream.output.display_lossy()
    );
    assert_eq!(
        upstream.exit_code,
        shirabe.exit_code,
        "shirabe output: {}",
        shirabe.output.display_lossy()
    );
    assert_eq!(upstream.installer_trace, shirabe.installer_trace);
    assert_eq!(upstream.asset, shirabe.asset);
    assert_eq!(upstream.trace, shirabe.trace);

    // Pinned as well as compared, so a run where neither side wrote a trace cannot pass. The
    // `settled=false` lines are the evidence that a promise is pending when it is handed over,
    // and the lines after them that the wait which follows is what settles it.
    assert_eq!(
        "\
prepare returned
install returned settled=false
install settled output=\"unpacked\\n\"
install wrote asset
",
        upstream.installer_trace
    );
    assert_eq!("shirabe-test/async-asset-a\n", upstream.asset);
    assert_eq!(
        "\
event=post-update-cmd
executeAsync-before-enable=LogicException: You must use the ProcessExecutor instance which is part of a Composer\\Loop instance to be able to run async processes
started settled=false countActiveJobs=1
waited settled=true countActiveJobs=0
process instanceof=true successful=true exitCode=0 exitCodeText=\"OK\" running=false terminated=true status=\"terminated\" pid=null
process output=\"out\\n\" error=\"err\\n\" incremental=[\"out\\n\",\"\"] commandLine=\"sleep 0.3; echo out; echo err 1>&2\" signaled=false stopped=false timeout=300
process cleared=\"\" stop=0
failing exitCode=3 successful=false rejected=null
loop before=[]
loop after=[\"fast\",\"slow\"] countActiveJobs=0
nested=[\"outer\",\"inner\",\"inner waited\"]
cancel rejected=\"RuntimeException: Aborted process\" countActiveJobs=0
queued-cancel ran=[\"occupying\"] rejected=null
budget order=[\"one\",\"two\"]
loop rejection=DomainException: from a callback
SyncHelper::await=\"awaited\"
removeDirectoryAsync result=true exists=false
removeDirectoryAsync missing=true
shared loop same=true output=\"shared\"
http started settled=false
http waited body=\"{\\\"slow\\\":true}\" countActiveJobs=0
http loop responses=[\"add 200 {\\\"probe\\\":true,\\\"n\\\":42}\",\"addCopy 200\"] file=\"{\\\"probe\\\":true,\\\"n\\\":42}\"
http missing=\"Composer\\\\Downloader\\\\TransportException\" status=404
http cancel rejected=\"Composer\\\\Exception\\\\IrrecoverableDownloadException\" canceled-message=true countActiveJobs=0
",
        upstream.trace
    );
}
