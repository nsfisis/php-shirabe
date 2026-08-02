//! ref: composer/tests/Composer/Test/CompletionFunctionalTest.php
//!
//! Validate autocompletion for all commands.
//!
//! The PHP test runs inside the Composer dev checkout and takes its expectations from that
//! environment: the checkout's own composer.json/composer.lock (installed packages, scripts,
//! package properties) and live Packagist queries for available-package suggestions. Each
//! data-provider entry is ported as its own test so the entries that cannot run here can be
//! ignored individually (currently only `exec `, whose expectations require a fully
//! installed vendor/bin).

#[path = "common/bootstrap.rs"]
mod bootstrap;

use serial_test::serial;
use shirabe::console::application::{Application, ApplicationHandle};
use shirabe_external_packages::symfony::console::tester::command_completion_tester::CommandCompletionTester;

struct RestoreCwd(std::path::PathBuf);

impl Drop for RestoreCwd {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.0);
    }
}

/// PHP runs from the Composer dev checkout; chdir into the vendored checkout to reproduce
/// that environment (restored on drop, so the tests are `#[serial]`).
fn chdir_composer_checkout() -> RestoreCwd {
    let prev = std::env::current_dir().unwrap();
    let checkout = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../composer");
    std::env::set_current_dir(checkout).unwrap();
    RestoreCwd(prev)
}

/// ref: CompletionFunctionalTest::testComplete
///
/// `expected_suggestions` are sample expected suggestions (a subset). None if nothing is
/// expected.
fn assert_complete(input: &str, expected_suggestions: Option<&[&str]>) {
    bootstrap::bootstrap();
    let _guard = chdir_composer_checkout();

    let mut input: Vec<&str> = input.split(' ').collect();
    let command_name = input.remove(0);
    // PHP: $this->getApplication()->get($commandName)
    let application = ApplicationHandle::new("Composer".to_string(), "".to_string()).unwrap();
    let base = application.__base_application();
    let command = {
        let mut app_ref = base.borrow_mut();
        let app_dyn: &mut dyn shirabe_external_packages::symfony::console::application::Application =
            &mut *app_ref;
        let app = app_dyn
            .as_any_mut()
            .downcast_mut::<Application>()
            .expect("ApplicationHandle wraps the shirabe Application");
        app.get(command_name).unwrap()
    };

    let tester = CommandCompletionTester::new(command);
    let suggestions = tester.complete(&input).unwrap();

    let Some(expected_suggestions) = expected_suggestions else {
        assert!(
            suggestions.is_empty(),
            "Expected no suggestions. Got \"{}\".",
            suggestions.join("\", \"")
        );
        return;
    };

    let diff: Vec<&str> = expected_suggestions
        .iter()
        .copied()
        .filter(|expected| !suggestions.iter().any(|s| s == expected))
        .collect();
    assert!(
        diff.is_empty(),
        "Suggestions must contain \"{}\". Got \"{}\".",
        diff.join("\", \""),
        suggestions.join("\", \"")
    );
}

// PHP: $randomVendor = 'a/'; $installedPackages = [...]; $preferInstall = [...]
const RANDOM_VENDOR: &str = "a/";
const INSTALLED_PACKAGES: &[&str] = &["composer/semver", "psr/log"];
const PREFER_INSTALL: &[&str] = &["dist", "source", "auto"];

#[test]
#[serial]
fn test_complete_archive() {
    assert_complete("archive ", Some(&[RANDOM_VENDOR]));
}

#[test]
#[serial]
fn test_complete_archive_package_prefix() {
    assert_complete(
        "archive symfony/http-",
        Some(&["symfony/http-kernel", "symfony/http-foundation"]),
    );
}

#[test]
#[serial]
fn test_complete_archive_format() {
    assert_complete("archive --format ", Some(&["tar", "zip"]));
}

#[test]
#[serial]
fn test_complete_create_project() {
    assert_complete("create-project ", Some(&[RANDOM_VENDOR]));
}

#[test]
#[serial]
fn test_complete_create_project_prefer_install() {
    assert_complete(
        "create-project symfony/skeleton --prefer-install ",
        Some(PREFER_INSTALL),
    );
}

#[test]
#[serial]
fn test_complete_depends() {
    assert_complete("depends ", Some(INSTALLED_PACKAGES));
}

#[test]
#[serial]
fn test_complete_why() {
    assert_complete("why ", Some(INSTALLED_PACKAGES));
}

#[test]
#[serial]
#[ignore = "expects the Composer dev checkout's fully installed vendor/bin (phpstan, simple-phpunit, ...); the vendored checkout ships only jsonlint and validate-json"]
fn test_complete_exec() {
    assert_complete(
        "exec ",
        Some(&[
            "composer",
            "jsonlint",
            "phpstan",
            "phpstan.phar",
            "simple-phpunit",
            "validate-json",
        ]),
    );
}

#[test]
#[serial]
fn test_complete_browse() {
    assert_complete("browse ", Some(INSTALLED_PACKAGES));
}

#[test]
#[serial]
fn test_complete_home_h() {
    assert_complete("home -H ", Some(INSTALLED_PACKAGES));
}

#[test]
#[serial]
fn test_complete_init_require() {
    assert_complete("init --require ", Some(&[RANDOM_VENDOR]));
}

#[test]
#[serial]
fn test_complete_init_require_dev() {
    assert_complete(
        "init --require-dev foo/bar --require-dev ",
        Some(&[RANDOM_VENDOR]),
    );
}

#[test]
#[serial]
fn test_complete_install_prefer_install() {
    assert_complete("install --prefer-install ", Some(PREFER_INSTALL));
}

#[test]
#[serial]
fn test_complete_install() {
    assert_complete("install ", None);
}

#[test]
#[serial]
fn test_complete_outdated() {
    assert_complete("outdated ", Some(INSTALLED_PACKAGES));
}

#[test]
#[serial]
fn test_complete_prohibits() {
    assert_complete("prohibits ", Some(&[RANDOM_VENDOR]));
}

#[test]
#[serial]
fn test_complete_why_not() {
    assert_complete("why-not symfony/http-ker", Some(&["symfony/http-kernel"]));
}

#[test]
#[serial]
fn test_complete_reinstall_prefer_install() {
    assert_complete("reinstall --prefer-install ", Some(PREFER_INSTALL));
}

#[test]
#[serial]
fn test_complete_reinstall() {
    assert_complete("reinstall ", Some(INSTALLED_PACKAGES));
}

#[test]
#[serial]
fn test_complete_remove() {
    assert_complete("remove ", Some(INSTALLED_PACKAGES));
}

#[test]
#[serial]
fn test_complete_require_prefer_install() {
    assert_complete("require --prefer-install ", Some(PREFER_INSTALL));
}

#[test]
#[serial]
fn test_complete_require() {
    assert_complete("require ", Some(&[RANDOM_VENDOR]));
}

#[test]
#[serial]
fn test_complete_require_dev_package_prefix() {
    assert_complete(
        "require --dev symfony/http-",
        Some(&["symfony/http-kernel", "symfony/http-foundation"]),
    );
}

#[test]
#[serial]
fn test_complete_run_script() {
    assert_complete("run-script ", Some(&["compile", "test", "phpstan"]));
}

#[test]
#[serial]
fn test_complete_run_script_args() {
    assert_complete("run-script test ", None);
}

#[test]
#[serial]
fn test_complete_search_format() {
    assert_complete("search --format ", Some(&["text", "json"]));
}

#[test]
#[serial]
fn test_complete_show_format() {
    assert_complete("show --format ", Some(&["text", "json"]));
}

#[test]
#[serial]
fn test_complete_info() {
    assert_complete("info ", Some(INSTALLED_PACKAGES));
}

#[test]
#[serial]
fn test_complete_suggests() {
    assert_complete("suggests ", Some(INSTALLED_PACKAGES));
}

#[test]
#[serial]
fn test_complete_update_prefer_install() {
    assert_complete("update --prefer-install ", Some(PREFER_INSTALL));
}

#[test]
#[serial]
fn test_complete_update() {
    assert_complete("update ", Some(INSTALLED_PACKAGES));
}

#[test]
#[serial]
fn test_complete_config_list() {
    assert_complete("config --list ", None);
}

#[test]
#[serial]
fn test_complete_config_editor() {
    assert_complete("config --editor ", None);
}

#[test]
#[serial]
fn test_complete_config_auth() {
    assert_complete("config --auth ", None);
}

#[test]
#[serial]
fn test_complete_config() {
    assert_complete(
        "config ",
        Some(&[
            "bin-compat",
            "extra",
            "extra.branch-alias",
            "home",
            "name",
            "repositories",
            "repositories.packagist.org",
            "suggest",
            "suggest.ext-zip",
            "type",
            "version",
        ]),
    );
}

// global setting
#[test]
#[serial]
fn test_complete_config_bin() {
    assert_complete("config bin", Some(&["bin-dir"]));
}

// existing package-property
#[test]
#[serial]
fn test_complete_config_nam() {
    assert_complete("config nam", Some(&["name"]));
}

// non-existing package-property
#[test]
#[serial]
fn test_complete_config_ver() {
    assert_complete("config ver", Some(&["version"]));
}

#[test]
#[serial]
fn test_complete_config_repo() {
    assert_complete(
        "config repo",
        Some(&["repositories", "repositories.packagist.org"]),
    );
}

#[test]
#[serial]
fn test_complete_config_repositories_dot() {
    assert_complete(
        "config repositories.",
        Some(&["repositories.packagist.org"]),
    );
}

#[test]
#[serial]
fn test_complete_config_sug() {
    assert_complete("config sug", Some(&["suggest", "suggest.ext-zip"]));
}

#[test]
#[serial]
fn test_complete_config_suggest_ext() {
    assert_complete("config suggest.ext-", Some(&["suggest.ext-zip"]));
}

#[test]
#[serial]
fn test_complete_config_ext() {
    assert_complete(
        "config ext",
        Some(&["extra", "extra.branch-alias", "extra.branch-alias.dev-main"]),
    );
}

// as this test does not use a fixture (yet?), the completion
// of setting authentication settings can have varying results
// yield ['config http-basic.', […]];

#[test]
#[serial]
fn test_complete_config_unset() {
    assert_complete(
        "config --unset ",
        Some(&[
            "extra",
            "extra.branch-alias",
            "extra.branch-alias.dev-main",
            "name",
            "suggest",
            "suggest.ext-zip",
            "type",
        ]),
    );
}

// global setting
#[test]
#[serial]
fn test_complete_config_unset_bin_dir() {
    assert_complete("config --unset bin-dir", None);
}

// existing package-property
#[test]
#[serial]
fn test_complete_config_unset_nam() {
    assert_complete("config --unset nam", Some(&["name"]));
}

// non-existing package-property
#[test]
#[serial]
fn test_complete_config_unset_version() {
    assert_complete("config --unset version", None);
}

#[test]
#[serial]
fn test_complete_config_unset_extra_dot() {
    assert_complete(
        "config --unset extra.",
        Some(&["extra.branch-alias", "extra.branch-alias.dev-main"]),
    );
}

// as this test does not use a fixture (yet?), the completion
// of unsetting authentication settings can have varying results
// yield ['config --unset http-basic.', […]];

#[test]
#[serial]
fn test_complete_config_global() {
    assert_complete(
        "config --global ",
        Some(&[
            "bin-compat",
            "home",
            "repositories",
            "repositories.packagist.org",
        ]),
    );
}

#[test]
#[serial]
fn test_complete_config_global_repo() {
    assert_complete(
        "config --global repo",
        Some(&["repositories", "repositories.packagist.org"]),
    );
}

#[test]
#[serial]
fn test_complete_config_global_repositories_dot() {
    assert_complete(
        "config --global repositories.",
        Some(&["repositories.packagist.org"]),
    );
}

// as this test does not use a fixture (yet?), the completion
// of unsetting global settings can have varying results
// yield ['config --global --unset ', null];

// as this test does not use a fixture (yet?), the completion of
// unsetting global authentication settings can have varying results
// yield ['config --global --unset http-basic.', […]];
