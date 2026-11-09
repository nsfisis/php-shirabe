//! ref: composer/src/Composer/Util/IniHelper.php sha1:1533d2baf9088a804193ca464c0081145da3ede4

use bstr::BString;
use shirabe_php_shim::bformat;

pub struct IniHelper;

impl IniHelper {
    /// Returns an array of php.ini locations with at least one entry.
    ///
    /// PHP asks `XdebugHandler::getAllIniFiles()`, which answers from `COMPOSER_ORIGINAL_INIS`
    /// when it is set, because a restarted process runs on a generated ini file and has to name
    /// the ones it replaced. Nothing here restarts PHP, so the worker always runs on the machine's
    /// own ini files and the variable is not consulted.
    pub fn get_all() -> anyhow::Result<Vec<std::path::PathBuf>> {
        crate::php_worker::xdebug::get_all_ini_files()
    }

    /// Describes the location of the loaded php.ini file(s).
    pub fn get_message() -> anyhow::Result<BString> {
        let mut paths = Self::get_all()?;

        if paths.first().is_some_and(|s| s.as_os_str().is_empty()) {
            paths.remove(0);
        }

        let ini = if paths.is_empty() {
            std::path::PathBuf::new()
        } else {
            paths.remove(0)
        };

        if ini.as_os_str().is_empty() {
            return Ok(BString::from(
                "A php.ini file does not exist. You will have to create one.",
            ));
        }

        if !paths.is_empty() {
            return Ok(BString::from(
                "Your command-line PHP is using multiple ini files. Run `php --ini` to show them.",
            ));
        }

        Ok(bformat!(
            "The php.ini used by your command-line PHP is: {ini}"
        ))
    }
}
