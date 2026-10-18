//! File download event E2E compatibility check: upstream Composer and Shirabe each install a
//! fixture project and then require a package from a Composer repository the test serves, with a
//! plugin subscribed to the file download events appending what each event exposes to a trace
//! file. The repository and the package's dist both come from PHP's built-in web server over the
//! fixture's `server/` directory, so the metadata downloads of `ComposerRepository` and the dist
//! download of `FileDownloader` all happen without the network. Upstream has no test that
//! dispatches these events through a real plugin, so the whole fixture is Shirabe-authored
//! (`fixtures/e2e-file-download/`); the test skips only while the PHP runtime or the Composer
//! checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::php_worker::{lock_php_worker, php_runtime_available};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e-file-download")
}

/// PHP's built-in web server running the fixture's router script, killed when dropped.
struct HttpServer {
    child: std::process::Child,
    base_url: String,
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
        let address = format!("127.0.0.1:{port}");
        let child = std::process::Command::new("php")
            .arg("-S")
            .arg(&address)
            .arg(fixture_dir().join("server/router.php"))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let server = Self {
            child,
            base_url: format!("http://{address}"),
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::net::TcpStream::connect(&address).is_err() {
            assert!(
                std::time::Instant::now() < deadline,
                "the fixture HTTP server did not come up on {address}"
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

struct Run {
    exit_codes: Vec<i32>,
    output: String,
    trace: String,
    installed: bool,
}

/// Runs `install` and then `require shirabe-test/remote-lib:1.0.0` in a fresh copy of the
/// fixture: the plugin is only active once the first command has installed it, so the events it
/// records are the ones the second command dispatches. The server's base URL is written into the
/// project's repository list and replaced by `{server}` in the trace.
fn install_then_require(program: &str, prefix_args: &[&str], server: &HttpServer) -> Run {
    let work = TempDir::new().unwrap();
    copy_dir(&fixture_dir(), work.path());
    let project = work.path().join("project");
    let composer_json = project.join("composer.json");
    let contents = std::fs::read_to_string(&composer_json).unwrap();
    std::fs::write(
        &composer_json,
        contents.replace("{server}", &server.base_url),
    )
    .unwrap();
    let commands: [&[&str]; 2] = [&["install"], &["require", "shirabe-test/remote-lib:1.0.0"]];
    let mut output = String::new();
    let exit_codes = commands
        .iter()
        .map(|args| {
            let result = std::process::Command::new(program)
                .args(prefix_args)
                .args(*args)
                .current_dir(&project)
                .env("COMPOSER_HOME", work.path().join("home"))
                .env("COMPOSER_CACHE_DIR", work.path().join("cache"))
                .env("COMPOSER_NO_INTERACTION", "1")
                .env("COLUMNS", "120")
                .env("LINES", "30")
                .output()
                .unwrap();
            output.push_str(&String::from_utf8_lossy(&result.stdout));
            output.push_str(&String::from_utf8_lossy(&result.stderr));
            result.status.code().unwrap_or(-1)
        })
        .collect();
    Run {
        exit_codes,
        output,
        trace: std::fs::read_to_string(project.join("file-download-trace.txt"))
            .unwrap_or_default()
            .replace(&server.base_url, "{server}"),
        installed: project
            .join("vendor/shirabe-test/remote-lib/README.md")
            .is_file(),
    }
}

#[test]
fn test_file_download_events_match_upstream_composer() {
    if !php_runtime_available() {
        return;
    }
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();
    let composer_bin = composer_bin.to_str().unwrap().to_string();

    let server = HttpServer::start();
    let upstream = install_then_require("php", &[composer_bin.as_str()], &server);
    let shirabe = install_then_require(env!("CARGO_BIN_EXE_shirabe"), &[], &server);

    assert_eq!(
        vec![0, 0],
        upstream.exit_codes,
        "upstream commands must succeed: {}",
        upstream.output
    );
    assert_eq!(
        upstream.exit_codes, shirabe.exit_codes,
        "shirabe output: {}",
        shirabe.output
    );
    assert!(upstream.installed);
    assert!(shirabe.installed);
    assert_eq!(upstream.trace, shirabe.trace);

    // Pinned as well as compared, so a run where neither side dispatches anything cannot pass.
    // The dist URL the repository serves does not resolve, so the package only installs because
    // the rewritten URL is the one downloaded.
    assert_eq!(
        "\
pre-file-download type=metadata url={server}/packages.json cacheKey=null transportOptions=[] httpDownloader=1 newTransportOptions={\"http\":{\"header\":[\"X-Shirabe-Probe: 1\"]}}
post-file-download type=metadata url={server}/packages.json fileName=null checksum=null
pre-file-download type=metadata url={server}/p2/shirabe-test/remote-lib.json cacheKey=null transportOptions=[] httpDownloader=1 newTransportOptions={\"http\":{\"header\":[\"X-Shirabe-Probe: 1\"]}}
post-file-download type=metadata url={server}/p2/shirabe-test/remote-lib.json fileName=null checksum=null
pre-file-download type=package url={server}/dists/missing.zip cacheKey=null transportOptions=[] httpDownloader=1 context=shirabe-test/remote-lib@1.0.0 rewritten={server}/dists/remote-lib.zip customCacheKey=custom-key
post-file-download type=package url={server}/dists/remote-lib.zip fileName=\"composer\\/zip\" checksum=\"0cf62c3ceab11046cdb3f6a5c60fab5a1d731905\" context=shirabe-test/remote-lib@1.0.0
",
        upstream.trace
    );
}
