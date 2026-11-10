//! Plugin class loader E2E compatibility check: upstream Composer and Shirabe each install a
//! fixture project with two plugins in the same vendor directory, where a class of the plugin
//! registered first is loaded only once the second plugin's class loader has been registered,
//! appending what they observe to a trace file. Upstream has no such test, so the whole
//! fixture is Shirabe-authored (`fixtures/e2e-plugin-loaders/`) and nothing has to be fetched; the
//! test skips only while the PHP runtime or the Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::php_worker::lock_php_worker;
use bstr::{BStr, BString};
use shirabe_php_shim::os_str_from_bytes;

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/plugin/fixtures/e2e-plugin-loaders")
}

struct Run {
    exit_code: i32,
    trace: BString,
}

/// Runs `install` in a fresh copy of the fixture and returns the exit code with the plugins'
/// trace.
fn install(program: &BStr, prefix_args: &[&std::ffi::OsStr]) -> Run {
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
        .env("COLUMNS", "120")
        .env("LINES", "30")
        .output()
        .unwrap();
    Run {
        exit_code: output.status.code().unwrap_or(-1),
        trace: BString::from(
            std::fs::read(project.join("plugin-loaders-trace.txt")).unwrap_or_default(),
        ),
    }
}

#[test]
fn test_first_plugin_loads_a_class_after_a_second_plugin_loader_is_registered() {
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
activate first lazy-loaded=false
activate second
second lazy-exists=true
first lazy-loaded=true
first lazy class loaded
",
        upstream.trace
    );
}
