//! ref: composer/tests/Composer/Test/CompletionFunctionalTest.php sha1:138f5457c86af85b29e484a86d24b8a9f9046805
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
#[path = "common/isolated_cwd.rs"]
mod isolated_cwd;

use bstr::{BStr, BString, ByteSlice};
#[cfg(not(target_os = "linux"))]
use serial_test::serial;
use shirabe::console::application::{Application, ApplicationHandle};
use shirabe_php_shim::{implode, php_str};
use shirabe_symfony_console::tester::CommandCompletionTester;

/// PHP runs from the Composer dev checkout; chdir into the vendored checkout to reproduce
/// that environment.
fn chdir_composer_checkout() -> isolated_cwd::RestoreCwd {
    isolated_cwd::set_current_dir(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../composer"),
    )
}

/// ref: CompletionFunctionalTest::testComplete
///
/// `expected_suggestions` are sample expected suggestions (a subset). None if nothing is
/// expected.
fn assert_complete(input: &BStr, expected_suggestions: Option<&[&BStr]>) {
    bootstrap::bootstrap();
    let _guard = chdir_composer_checkout();

    let mut input: Vec<&BStr> = input.split_str(" ").map(BStr::new).collect();
    let command_name = input.remove(0);
    // PHP: $this->getApplication()->get($commandName)
    let application = ApplicationHandle::new(BString::from("Shirabe"), BString::from("")).unwrap();
    let base = application.__base_application();
    let command = {
        let mut app_ref = base.borrow_mut();
        let app_dyn: &mut dyn shirabe_symfony_console::application::Application = &mut *app_ref;
        let app = app_dyn
            .as_any_mut()
            .downcast_mut::<Application>()
            .expect("ApplicationHandle wraps the shirabe Application");
        app.get(bstr::BStr::new(command_name)).unwrap()
    };

    let tester = CommandCompletionTester::new(command);
    let suggestions = tester.complete(&input).unwrap();

    let Some(expected_suggestions) = expected_suggestions else {
        assert!(
            suggestions.is_empty(),
            "Expected no suggestions. Got \"{}\".",
            implode("\", \"", &suggestions).display_lossy()
        );
        return;
    };

    let diff: Vec<&BStr> = expected_suggestions
        .iter()
        .copied()
        .filter(|expected| !suggestions.iter().any(|s| s == expected))
        .collect();
    assert!(
        diff.is_empty(),
        "Suggestions must contain \"{}\". Got \"{}\".",
        implode("\", \"", &diff).display_lossy(),
        implode("\", \"", &suggestions).display_lossy()
    );
}

// PHP: $randomVendor = 'a/'; $installedPackages = [...]; $preferInstall = [...]
const RANDOM_VENDOR: &BStr = php_str(b"a/");
const INSTALLED_PACKAGES: &[&BStr] = &[php_str(b"composer/semver"), php_str(b"psr/log")];
const PREFER_INSTALL: &[&BStr] = &[php_str(b"dist"), php_str(b"source"), php_str(b"auto")];

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_archive() {
    assert_complete(BStr::new("archive "), Some(&[RANDOM_VENDOR]));
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_archive_package_prefix() {
    assert_complete(
        BStr::new("archive symfony/http-"),
        Some(&[
            BStr::new("symfony/http-kernel"),
            BStr::new("symfony/http-foundation"),
        ]),
    );
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_archive_format() {
    assert_complete(
        BStr::new("archive --format "),
        Some(&[BStr::new("tar"), BStr::new("zip")]),
    );
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_create_project() {
    assert_complete(BStr::new("create-project "), Some(&[RANDOM_VENDOR]));
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_create_project_prefer_install() {
    assert_complete(
        BStr::new("create-project symfony/skeleton --prefer-install "),
        Some(PREFER_INSTALL),
    );
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_depends() {
    assert_complete(BStr::new("depends "), Some(INSTALLED_PACKAGES));
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_why() {
    assert_complete(BStr::new("why "), Some(INSTALLED_PACKAGES));
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
#[ignore = "expects the Composer dev checkout's fully installed vendor/bin (phpstan, simple-phpunit, ...); the vendored checkout ships only jsonlint and validate-json"]
fn test_complete_exec() {
    assert_complete(
        BStr::new("exec "),
        Some(&[
            BStr::new("composer"),
            BStr::new("jsonlint"),
            BStr::new("phpstan"),
            BStr::new("phpstan.phar"),
            BStr::new("simple-phpunit"),
            BStr::new("validate-json"),
        ]),
    );
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_browse() {
    assert_complete(BStr::new("browse "), Some(INSTALLED_PACKAGES));
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_home_h() {
    assert_complete(BStr::new("home -H "), Some(INSTALLED_PACKAGES));
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_init_require() {
    assert_complete(BStr::new("init --require "), Some(&[RANDOM_VENDOR]));
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_init_require_dev() {
    assert_complete(
        BStr::new("init --require-dev foo/bar --require-dev "),
        Some(&[RANDOM_VENDOR]),
    );
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_install_prefer_install() {
    assert_complete(BStr::new("install --prefer-install "), Some(PREFER_INSTALL));
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_install() {
    assert_complete(BStr::new("install "), None);
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_outdated() {
    assert_complete(BStr::new("outdated "), Some(INSTALLED_PACKAGES));
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_prohibits() {
    assert_complete(BStr::new("prohibits "), Some(&[RANDOM_VENDOR]));
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_why_not() {
    assert_complete(
        BStr::new("why-not symfony/http-ker"),
        Some(&[BStr::new("symfony/http-kernel")]),
    );
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_reinstall_prefer_install() {
    assert_complete(
        BStr::new("reinstall --prefer-install "),
        Some(PREFER_INSTALL),
    );
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_reinstall() {
    assert_complete(BStr::new("reinstall "), Some(INSTALLED_PACKAGES));
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_remove() {
    assert_complete(BStr::new("remove "), Some(INSTALLED_PACKAGES));
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_require_prefer_install() {
    assert_complete(BStr::new("require --prefer-install "), Some(PREFER_INSTALL));
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_require() {
    assert_complete(BStr::new("require "), Some(&[RANDOM_VENDOR]));
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_require_dev_package_prefix() {
    assert_complete(
        BStr::new("require --dev symfony/http-"),
        Some(&[
            BStr::new("symfony/http-kernel"),
            BStr::new("symfony/http-foundation"),
        ]),
    );
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_run_script() {
    assert_complete(
        BStr::new("run-script "),
        Some(&[
            BStr::new("compile"),
            BStr::new("test"),
            BStr::new("phpstan"),
        ]),
    );
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_run_script_args() {
    assert_complete(BStr::new("run-script test "), None);
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_search_format() {
    assert_complete(
        BStr::new("search --format "),
        Some(&[BStr::new("text"), BStr::new("json")]),
    );
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_show_format() {
    assert_complete(
        BStr::new("show --format "),
        Some(&[BStr::new("text"), BStr::new("json")]),
    );
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_info() {
    assert_complete(BStr::new("info "), Some(INSTALLED_PACKAGES));
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_suggests() {
    assert_complete(BStr::new("suggests "), Some(INSTALLED_PACKAGES));
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_update_prefer_install() {
    assert_complete(BStr::new("update --prefer-install "), Some(PREFER_INSTALL));
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_update() {
    assert_complete(BStr::new("update "), Some(INSTALLED_PACKAGES));
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_config_list() {
    assert_complete(BStr::new("config --list "), None);
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_config_editor() {
    assert_complete(BStr::new("config --editor "), None);
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_config_auth() {
    assert_complete(BStr::new("config --auth "), None);
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_config() {
    assert_complete(
        BStr::new("config "),
        Some(&[
            BStr::new("bin-compat"),
            BStr::new("extra"),
            BStr::new("extra.branch-alias"),
            BStr::new("home"),
            BStr::new("name"),
            BStr::new("repositories"),
            BStr::new("repositories.packagist.org"),
            BStr::new("suggest"),
            BStr::new("suggest.ext-zip"),
            BStr::new("type"),
            BStr::new("version"),
        ]),
    );
}

// global setting
#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_config_bin() {
    assert_complete(BStr::new("config bin"), Some(&[BStr::new("bin-dir")]));
}

// existing package-property
#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_config_nam() {
    assert_complete(BStr::new("config nam"), Some(&[BStr::new("name")]));
}

// non-existing package-property
#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_config_ver() {
    assert_complete(BStr::new("config ver"), Some(&[BStr::new("version")]));
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_config_repo() {
    assert_complete(
        BStr::new("config repo"),
        Some(&[
            BStr::new("repositories"),
            BStr::new("repositories.packagist.org"),
        ]),
    );
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_config_repositories_dot() {
    assert_complete(
        BStr::new("config repositories."),
        Some(&[BStr::new("repositories.packagist.org")]),
    );
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_config_sug() {
    assert_complete(
        BStr::new("config sug"),
        Some(&[BStr::new("suggest"), BStr::new("suggest.ext-zip")]),
    );
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_config_suggest_ext() {
    assert_complete(
        BStr::new("config suggest.ext-"),
        Some(&[BStr::new("suggest.ext-zip")]),
    );
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_config_ext() {
    assert_complete(
        BStr::new("config ext"),
        Some(&[
            BStr::new("extra"),
            BStr::new("extra.branch-alias"),
            BStr::new("extra.branch-alias.dev-main"),
        ]),
    );
}

// as this test does not use a fixture (yet?), the completion
// of setting authentication settings can have varying results
// yield ['config http-basic.', […]];

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_config_unset() {
    assert_complete(
        BStr::new("config --unset "),
        Some(&[
            BStr::new("extra"),
            BStr::new("extra.branch-alias"),
            BStr::new("extra.branch-alias.dev-main"),
            BStr::new("name"),
            BStr::new("suggest"),
            BStr::new("suggest.ext-zip"),
            BStr::new("type"),
        ]),
    );
}

// global setting
#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_config_unset_bin_dir() {
    assert_complete(BStr::new("config --unset bin-dir"), None);
}

// existing package-property
#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_config_unset_nam() {
    assert_complete(BStr::new("config --unset nam"), Some(&[BStr::new("name")]));
}

// non-existing package-property
#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_config_unset_version() {
    assert_complete(BStr::new("config --unset version"), None);
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_config_unset_extra_dot() {
    assert_complete(
        BStr::new("config --unset extra."),
        Some(&[
            BStr::new("extra.branch-alias"),
            BStr::new("extra.branch-alias.dev-main"),
        ]),
    );
}

// as this test does not use a fixture (yet?), the completion
// of unsetting authentication settings can have varying results
// yield ['config --unset http-basic.', […]];

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_config_global() {
    assert_complete(
        BStr::new("config --global "),
        Some(&[
            BStr::new("bin-compat"),
            BStr::new("home"),
            BStr::new("repositories"),
            BStr::new("repositories.packagist.org"),
        ]),
    );
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_config_global_repo() {
    assert_complete(
        BStr::new("config --global repo"),
        Some(&[
            BStr::new("repositories"),
            BStr::new("repositories.packagist.org"),
        ]),
    );
}

#[test]
#[cfg_attr(not(target_os = "linux"), serial)]
fn test_complete_config_global_repositories_dot() {
    assert_complete(
        BStr::new("config --global repositories."),
        Some(&[BStr::new("repositories.packagist.org")]),
    );
}

// as this test does not use a fixture (yet?), the completion
// of unsetting global settings can have varying results
// yield ['config --global --unset ', null];

// as this test does not use a fixture (yet?), the completion of
// unsetting global authentication settings can have varying results
// yield ['config --global --unset http-basic.', […]];
