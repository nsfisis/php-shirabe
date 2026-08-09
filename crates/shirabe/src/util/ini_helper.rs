//! ref: composer/src/Composer/Util/IniHelper.php

pub struct IniHelper;

impl IniHelper {
    /// Returns an array of php.ini locations with at least one entry.
    pub fn get_all() -> Vec<String> {
        // PHP: XdebugHandler::getAllIniFiles(). shirabe_xdebug_handler::XdebugHandler's port
        // of that method is a stub that returns the PHP default, so query the real PHP
        // runtime's loaded/scanned ini files here instead.
        shirabe_php_rpc::get_all_ini_files()
    }

    /// Describes the location of the loaded php.ini file(s).
    pub fn get_message() -> String {
        let mut paths = Self::get_all();

        if paths.first().is_some_and(|s| s.is_empty()) {
            paths.remove(0);
        }

        let ini = if paths.is_empty() {
            String::new()
        } else {
            paths.remove(0)
        };

        if ini.is_empty() {
            return "A php.ini file does not exist. You will have to create one.".to_string();
        }

        if !paths.is_empty() {
            return "Your command-line PHP is using multiple ini files. Run `php --ini` to show them.".to_string();
        }

        format!("The php.ini used by your command-line PHP is: {}", ini)
    }
}
