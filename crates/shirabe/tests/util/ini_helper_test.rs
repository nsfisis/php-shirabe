//! ref: composer/tests/Composer/Test/Util/IniHelperTest.php

use shirabe::util::ini_helper::IniHelper;
use shirabe_php_shim::{PATH_SEPARATOR, putenv};

fn set_env(paths: &[&str]) {
    putenv(&format!(
        "COMPOSER_ORIGINAL_INIS={}",
        paths.join(PATH_SEPARATOR)
    ));
}

#[test]
#[ignore = "IniHelper::get_all reaches XdebugHandler::get_all_ini_files, which is todo!()"]
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
#[ignore = "IniHelper::get_all reaches XdebugHandler::get_all_ini_files, which is todo!()"]
fn test_with_loaded_ini_only() {
    let paths = ["loaded.ini"];

    set_env(&paths);
    assert!(IniHelper::get_message().contains("loaded.ini"));
}

#[test]
#[ignore = "IniHelper::get_all reaches XdebugHandler::get_all_ini_files, which is todo!()"]
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
#[ignore = "IniHelper::get_all reaches XdebugHandler::get_all_ini_files, which is todo!()"]
fn test_without_loaded_ini_and_additional() {
    let paths = ["", "one.ini", "two.ini"];

    set_env(&paths);
    assert!(IniHelper::get_message().contains("multiple ini files"));
    assert_eq!(
        paths.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        IniHelper::get_all()
    );
}
