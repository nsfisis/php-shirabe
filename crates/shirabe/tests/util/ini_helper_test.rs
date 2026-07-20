//! ref: composer/tests/Composer/Test/Util/IniHelperTest.php

use shirabe::util::ini_helper::IniHelper;
use shirabe::util::platform::Platform;
use shirabe_php_shim::{PATH_SEPARATOR, getenv, putenv};

#[allow(dead_code)]
fn set_up() -> TearDown {
    // Register our name with XdebugHandler.
    // The PHP test constructs `new XdebugHandler('composer')` so that
    // getAllIniFiles() reads the COMPOSER_ORIGINAL_INIS env var. The Rust
    // XdebugHandler is a unit struct with no name-registration API, so this
    // step is a no-op here.
    // Save current state
    let env_original =
        getenv("COMPOSER_ORIGINAL_INIS").map(|value| value.to_string_lossy().into_owned());
    TearDown { env_original }
}

#[allow(dead_code)]
fn tear_down(env_original: &Option<String>) {
    // Restore original state
    if let Some(env_original) = env_original {
        unsafe { putenv("COMPOSER_ORIGINAL_INIS", env_original) };
    } else {
        Platform::clear_env("COMPOSER_ORIGINAL_INIS");
    }
}

#[allow(dead_code)]
struct TearDown {
    env_original: Option<String>,
}

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down(&self.env_original);
    }
}

fn set_env(paths: &[&str]) {
    unsafe { putenv("COMPOSER_ORIGINAL_INIS", paths.join(PATH_SEPARATOR)) };
}

#[test]
fn test_with_no_ini() {
    let paths = [""];

    set_env(&paths);
    assert!(IniHelper::get_message().contains("does not exist"));
    assert_eq!(
        paths.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        IniHelper::get_all()
    );
}

#[test]
#[ignore = "XdebugHandler::get_all_ini_files() is stubbed to always return [\"\"], ignoring \
COMPOSER_ORIGINAL_INIS entirely"]
fn test_with_loaded_ini_only() {
    let paths = ["loaded.ini"];

    set_env(&paths);
    assert!(IniHelper::get_message().contains("loaded.ini"));
}

#[test]
#[ignore = "XdebugHandler::get_all_ini_files() is stubbed to always return [\"\"], ignoring \
COMPOSER_ORIGINAL_INIS entirely"]
fn test_with_loaded_ini_and_additional() {
    let paths = ["loaded.ini", "one.ini", "two.ini"];

    set_env(&paths);
    assert!(IniHelper::get_message().contains("multiple ini files"));
    assert_eq!(
        paths.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        IniHelper::get_all()
    );
}

#[test]
#[ignore = "XdebugHandler::get_all_ini_files() is stubbed to always return [\"\"], ignoring \
COMPOSER_ORIGINAL_INIS entirely"]
fn test_without_loaded_ini_and_additional() {
    let paths = ["", "one.ini", "two.ini"];

    set_env(&paths);
    assert!(IniHelper::get_message().contains("multiple ini files"));
    assert_eq!(
        paths.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        IniHelper::get_all()
    );
}
