//! AutoloadGenerator E2E compatibility check: upstream Composer and Shirabe each install a fixture
//! project whose plugin drives the generator `Composer::getAutoloadGenerator()` hands out and writes
//! what every call on it reports to a trace file. Upstream has no test that drives the generator
//! from plugin code, so the whole fixture is Shirabe-authored (`fixtures/e2e-autoload-generator/`)
//! and nothing has to be fetched; the test skips only while the PHP runtime or the Composer
//! checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::php_worker::{lock_php_worker, php_runtime_available};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e-autoload-generator")
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
        trace: std::fs::read_to_string(project.join("autoload-generator-trace.txt"))
            .unwrap_or_default(),
    }
}

#[test]
fn test_plugin_driven_autoload_generator_matches_upstream_composer() {
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

    // Pinned as well as compared, so a run where neither side wrote a trace cannot pass.
    assert_eq!(
        "\
event=post-update-cmd
class=Composer\\Autoload\\AutoloadGenerator same=true
packageMap shirabe/e2e-autoload-generator=\"\" shirabe-test/autoload-generator-probe=\"<cwd>\\/vendor\\/shirabe-test\\/autoload-generator-probe\" acme/dev-tool=\"<cwd>\\/vendor\\/acme\\/dev-tool\" acme/lib-b=\"<cwd>\\/vendor\\/acme\\/lib-b\" acme/lib-a=\"<cwd>\\/vendor\\/acme\\/lib-a\"
packageMap rootIdentity=true
invalid caught=InvalidArgumentException message=\"psr-4 namespaces must end with a namespace separator, 'Acme\\\\Invalid' does not, use 'Acme\\\\Invalid\\\\'.\"
autoloads {\"psr-0\":{\"AcmeB_\":[\"<cwd>\\/vendor\\/acme\\/lib-b\\/lib\\/\"]},\"psr-4\":{\"ShirabeTest\\\\AutoloadGenerator\\\\\":[\"<cwd>\\/vendor\\/shirabe-test\\/autoload-generator-probe\\/src\\/\"],\"App\\\\\":[\"app\\/\"],\"Acme\\\\Dev\\\\\":[\"<cwd>\\/vendor\\/acme\\/dev-tool\\/src\\/\"],\"Acme\\\\A\\\\\":[\"<cwd>\\/vendor\\/acme\\/lib-a\\/src\\/\"]},\"classmap\":[\"classes\\/\",\"<cwd>\\/vendor\\/acme\\/lib-a\\/lib\\/\"],\"files\":{\"086e64d3836f837a0d310c79fdd64822\":\"<cwd>\\/vendor\\/acme\\/lib-a\\/helpers.php\"},\"exclude-from-classmap\":[]}
autoloads no-dev {\"psr-0\":{\"AcmeB_\":[\"<cwd>\\/vendor\\/acme\\/lib-b\\/lib\\/\"]},\"psr-4\":{\"ShirabeTest\\\\AutoloadGenerator\\\\\":[\"<cwd>\\/vendor\\/shirabe-test\\/autoload-generator-probe\\/src\\/\"],\"App\\\\\":[\"app\\/\"],\"Acme\\\\A\\\\\":[\"<cwd>\\/vendor\\/acme\\/lib-a\\/src\\/\"]},\"classmap\":[\"classes\\/\",\"<cwd>\\/vendor\\/acme\\/lib-a\\/lib\\/\"],\"files\":{\"086e64d3836f837a0d310c79fdd64822\":\"<cwd>\\/vendor\\/acme\\/lib-a\\/helpers.php\"},\"exclude-from-classmap\":[]}
autoloads filtered {\"psr-0\":[],\"psr-4\":{\"ShirabeTest\\\\AutoloadGenerator\\\\\":[\"<cwd>\\/vendor\\/shirabe-test\\/autoload-generator-probe\\/src\\/\"],\"App\\\\\":[\"app\\/\"],\"Acme\\\\Dev\\\\\":[\"<cwd>\\/vendor\\/acme\\/dev-tool\\/src\\/\"],\"Acme\\\\A\\\\\":[\"<cwd>\\/vendor\\/acme\\/lib-a\\/src\\/\"]},\"classmap\":[\"classes\\/\",\"<cwd>\\/vendor\\/acme\\/lib-a\\/lib\\/\"],\"files\":{\"086e64d3836f837a0d310c79fdd64822\":\"<cwd>\\/vendor\\/acme\\/lib-a\\/helpers.php\"},\"exclude-from-classmap\":[]}
loader class=Composer\\Autoload\\ClassLoader registered=false
loader psr-0 {\"AcmeB_\":[\"<cwd>\\/vendor\\/acme\\/lib-b\\/lib\\/\"]}
loader psr-4 {\"ShirabeTest\\\\AutoloadGenerator\\\\\":[\"<cwd>\\/vendor\\/shirabe-test\\/autoload-generator-probe\\/src\\/\"],\"App\\\\\":[\"app\\/\"],\"Acme\\\\Dev\\\\\":[\"<cwd>\\/vendor\\/acme\\/dev-tool\\/src\\/\"],\"Acme\\\\A\\\\\":[\"<cwd>\\/vendor\\/acme\\/lib-a\\/src\\/\"]}
loader classmap {\"Duplicate\":\"<cwd>\\/classes\\/Duplicate.php\"}
loader findFile \"<cwd>\\/vendor\\/acme\\/lib-a\\/src\\/\\/Widget.php\"
invalid loader caught=InvalidArgumentException message=\"A non-empty PSR-4 prefix must end with a namespace separator.\"
setters [null,null,null,null,null]
dump class=Composer\\ClassMapGenerator\\ClassMap count=2
dump map {\"Composer\\\\InstalledVersions\":\"<cwd>\\/vendor\\/composer\\/InstalledVersions.php\",\"Duplicate\":\"<cwd>\\/classes\\/Duplicate.php\"}
dump ambiguous {\"Duplicate\":[\"<cwd>\\/vendor\\/acme\\/lib-a\\/lib\\/Duplicate.php\"]}
dump psrViolations []
dump scanPsr map {\"AcmeB_Thing\":\"<cwd>\\/vendor\\/acme\\/lib-b\\/lib\\/AcmeB\\/Thing.php\",\"Acme\\\\A\\\\Widget\":\"<cwd>\\/vendor\\/acme\\/lib-a\\/src\\/Widget.php\",\"Acme\\\\Dev\\\\Tool\":\"<cwd>\\/vendor\\/acme\\/dev-tool\\/src\\/Tool.php\",\"App\\\\Kernel\":\"<cwd>\\/app\\/Kernel.php\",\"Composer\\\\InstalledVersions\":\"<cwd>\\/vendor\\/composer\\/InstalledVersions.php\",\"Duplicate\":\"<cwd>\\/classes\\/Duplicate.php\",\"ShirabeTest\\\\AutoloadGenerator\\\\Plugin\":\"<cwd>\\/vendor\\/shirabe-test\\/autoload-generator-probe\\/src\\/Plugin.php\"}
dump scanPsr psrViolations [\"Class App\\\\Elsewhere\\\\Misplaced located in .\\/app\\/Misplaced.php does not comply with psr-4 autoloading standard (rule: App\\\\ => .\\/app). Skipping.\"]
dump authoritative count=7
dump no-dev map [\"AcmeB_Thing\",\"Acme\\\\A\\\\Widget\",\"App\\\\Kernel\",\"Composer\\\\InstalledVersions\",\"Duplicate\",\"ShirabeTest\\\\AutoloadGenerator\\\\Plugin\"]
",
        upstream.trace
    );
}
