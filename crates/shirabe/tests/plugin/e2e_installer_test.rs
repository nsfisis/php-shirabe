//! Plugin-provided installer E2E compatibility check: upstream Composer and Shirabe each install
//! a fixture plugin that registers its own `InstallerInterface` through
//! `InstallationManager::addInstaller`, and the resulting project trees — including the trace the
//! installer writes on every contract call — are compared.
//!
//! The whole fixture is Shirabe-authored (`fixtures/e2e-installer/`), so nothing has to be
//! fetched; the test skips only while the PHP runtime or the Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::php_worker::lock_php_worker;
use bstr::{BStr, BString, ByteSlice as _};
use shirabe_php_shim::{os_str_from_bytes, path_to_bytes};

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e-installer")
}

struct CommandRun {
    exit_code: i32,
    stdout: BString,
}

fn run_command(
    work: &std::path::Path,
    program: &BStr,
    prefix_args: &[&std::ffi::OsStr],
    args: &[&BStr],
) -> CommandRun {
    let output = std::process::Command::new(os_str_from_bytes(&program))
        .args(prefix_args)
        .args(args.iter().map(os_str_from_bytes))
        .current_dir(work.join("project"))
        .env("COMPOSER_HOME", work.join("home"))
        .env("COMPOSER_CACHE_DIR", work.join("cache"))
        .env("COMPOSER_NO_INTERACTION", "1")
        .env("COLUMNS", "120")
        .env("LINES", "30")
        .output()
        .unwrap();
    CommandRun {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: BString::from(output.stdout),
    }
}

/// Every file under `dir` as (relative path, contents), so two project trees compare as a whole.
fn tree(dir: &std::path::Path) -> Vec<(BString, BString)> {
    let mut files = Vec::new();
    collect(dir, dir, &mut files);
    files.sort();
    files
}

fn collect(root: &std::path::Path, dir: &std::path::Path, files: &mut Vec<(BString, BString)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if entry.file_type().unwrap().is_dir() {
            collect(root, &path, files);
        } else {
            let relative = BString::from(path_to_bytes(path.strip_prefix(root).unwrap()));
            files.push((
                relative,
                BString::from(std::fs::read(&path).unwrap_or_default()),
            ));
        }
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "the test splits a child process's output into lines, which no PHP code does"
)]
fn installer_lines(stdout: &[u8]) -> Vec<&BStr> {
    stdout
        .lines()
        .filter(|line| line.starts_with(b"asset-installer:"))
        .map(bstr::ByteSlice::as_bstr)
        .collect()
}

/// Runs `install` in a fresh copy of the fixture and returns the run plus the resulting tree.
fn install(
    program: &BStr,
    prefix: &[&std::ffi::OsStr],
) -> (CommandRun, Vec<(BString, BString)>, tempfile::TempDir) {
    let work = tempfile::TempDir::new().unwrap();
    copy_dir(&fixture_dir(), work.path());
    let run = run_command(work.path(), program, prefix, &[BStr::new("install")]);
    let tree = tree(&work.path().join("project"));
    (run, tree, work)
}

#[test]
fn test_plugin_provided_installer_matches_upstream_composer() {
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();

    let (upstream, upstream_tree, _upstream_work) =
        install(BStr::new("php"), &[composer_bin.as_os_str()]);
    let (shirabe, shirabe_tree, _shirabe_work) =
        install(BStr::new(env!("CARGO_BIN_EXE_shirabe")), &[]);

    assert_eq!(0, upstream.exit_code, "upstream install must succeed");
    assert_eq!(upstream.exit_code, shirabe.exit_code);
    assert_eq!(
        installer_lines(&upstream.stdout),
        installer_lines(&shirabe.stdout)
    );
    assert_eq!(
        vec!["asset-installer: installed shirabe-test/asset-a"],
        installer_lines(&shirabe.stdout)
    );

    // The whole project tree, which covers composer.lock, installed.json/installed.php, the
    // manifest the plugin-provided installer wrote under assets/, and installer-trace.txt — the
    // ordered log of every InstallerInterface call the installer received.
    assert_eq!(
        upstream_tree.iter().map(|(p, _)| p).collect::<Vec<_>>(),
        shirabe_tree.iter().map(|(p, _)| p).collect::<Vec<_>>()
    );
    assert_eq!(upstream_tree, shirabe_tree);
}

// Re-running `install` over an already installed project, and removing the asset package
// afterwards: installer-trace.txt records every `supports` lookup, so the tree comparison also
// pins the order and count of `getInstaller` calls.
#[test]
fn test_plugin_provided_installer_call_order_matches_upstream_composer() {
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();
    let implementations: [(&BStr, Vec<&std::ffi::OsStr>); 2] = [
        (BStr::new("php"), vec![composer_bin.as_os_str()]),
        (BStr::new(env!("CARGO_BIN_EXE_shirabe")), vec![]),
    ];

    let mut results = Vec::new();
    for (program, prefix) in &implementations {
        let work = tempfile::TempDir::new().unwrap();
        copy_dir(&fixture_dir(), work.path());
        let first = run_command(work.path(), program, prefix, &[BStr::new("install")]);
        let second = run_command(work.path(), program, prefix, &[BStr::new("install")]);
        let remove = run_command(
            work.path(),
            program,
            prefix,
            &[BStr::new("remove"), BStr::new("shirabe-test/asset-a")],
        );
        let tree = tree(&work.path().join("project"));
        results.push((first, second, remove, tree));
    }

    let [upstream, shirabe] = <[_; 2]>::try_from(results).ok().unwrap();
    let (u_first, u_second, u_remove, u_tree) = upstream;
    let (s_first, s_second, s_remove, s_tree) = shirabe;

    assert_eq!(0, u_first.exit_code);
    assert_eq!(u_first.exit_code, s_first.exit_code);
    assert_eq!(u_second.exit_code, s_second.exit_code);
    assert_eq!(u_remove.exit_code, s_remove.exit_code);
    assert_eq!(u_tree, s_tree);
}
