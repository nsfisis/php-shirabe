//! ref: composer/src/Composer/Util/IniHelper.php

pub struct IniHelper;

impl IniHelper {
    /// Returns an array of php.ini locations with at least one entry.
    ///
    /// PHP asks `XdebugHandler::getAllIniFiles()`, which answers from `COMPOSER_ORIGINAL_INIS`
    /// when it is set, because a restarted process runs on a generated ini file and has to name
    /// the ones it replaced. Nothing here restarts PHP, so the worker always runs on the machine's
    /// own ini files and the variable is not consulted.
    pub fn get_all() -> Vec<String> {
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
