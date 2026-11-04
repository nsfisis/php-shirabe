//! ref: composer/vendor/symfony/console/Terminal.php sha1:e9ca51f4cd459635b19cafdb8a46077d50a0f1ed

use bstr::BString;
use shirabe_php_shim::{
    Descriptor, PhpMixed, PhpResource, bconcat, fclose, function_exists, getenv, intval,
    path_to_bytes, php_fopen_resource, php_regex, preg_match, proc_close, proc_open,
    sapi_windows_cp_get, sapi_windows_cp_set, sapi_windows_vt100_support, shell_exec,
    stream_get_contents, trim,
};
use std::cell::Cell;

thread_local! {
    static WIDTH: Cell<Option<i64>> = const { Cell::new(None) };
    static HEIGHT: Cell<Option<i64>> = const { Cell::new(None) };
    static STTY: Cell<Option<bool>> = const { Cell::new(None) };
}

#[derive(Debug)]
pub struct Terminal;

impl Default for Terminal {
    fn default() -> Self {
        Self::new()
    }
}

impl Terminal {
    pub fn new() -> Self {
        Terminal
    }

    /// Gets the terminal width.
    pub fn get_width(&self) -> i64 {
        let width = getenv("COLUMNS");
        if let Some(width) = width {
            return intval(&PhpMixed::string(trim(path_to_bytes(&width))));
        }

        if WIDTH.with(|w| w.get()).is_none() {
            Self::init_dimensions();
        }

        WIDTH.with(|w| w.get()).filter(|&v| v != 0).unwrap_or(80)
    }

    /// Gets the terminal height.
    pub fn get_height(&self) -> i64 {
        let height = getenv("LINES");
        if let Some(height) = height {
            return intval(&PhpMixed::string(trim(path_to_bytes(&height))));
        }

        if HEIGHT.with(|h| h.get()).is_none() {
            Self::init_dimensions();
        }

        HEIGHT.with(|h| h.get()).filter(|&v| v != 0).unwrap_or(50)
    }

    pub fn has_stty_available() -> bool {
        if let Some(stty) = STTY.with(|s| s.get()) {
            return stty;
        }

        let result = shell_exec(bconcat!(
            "stty 2> ",
            if cfg!(windows) { "NUL" } else { "/dev/null" }
        ))
        .is_some();
        STTY.with(|s| s.set(Some(result)));
        result
    }

    fn init_dimensions() {
        if cfg!(windows) {
            let ansicon = getenv("ANSICON").map(|ansicon| trim(path_to_bytes(&ansicon)));
            if let Some(ansicon) = &ansicon
                && let Some(matches) = preg_match(
                    php_regex!("/^(\\d+)x(\\d+)(?: \\((\\d+)x(\\d+)\\))?$/"),
                    ansicon,
                )
            {
                // extract [w, H] from "wxh (WxH)"
                // or [w, h] from "wxh"
                WIDTH.with(|w| {
                    w.set(Some(intval(&PhpMixed::string(
                        matches.get(1).unwrap_or_default(),
                    ))))
                });
                HEIGHT.with(|h| {
                    let value = if matches.get(4).is_some() {
                        intval(&PhpMixed::string(matches.get(4).unwrap_or_default()))
                    } else {
                        intval(&PhpMixed::string(matches.get(2).unwrap_or_default()))
                    };
                    h.set(Some(value));
                });
                return;
            }

            if !Self::has_vt100_support() && Self::has_stty_available() {
                // only use stty on Windows if the terminal does not support vt100 (e.g. Windows 7 + git-bash)
                // testing for stty in a Windows 10 vt100-enabled console will implicitly disable vt100 support on STDOUT
                Self::init_dimensions_using_stty();
            } else if let Some(dimensions) = Self::get_console_mode() {
                // extract [w, h] from "wxh"
                WIDTH.with(|w| w.set(Some(dimensions[0])));
                HEIGHT.with(|h| h.set(Some(dimensions[1])));
            }
        } else {
            Self::init_dimensions_using_stty();
        }
    }

    /// Returns whether STDOUT has vt100 support (some Windows 10+ configurations).
    fn has_vt100_support() -> bool {
        function_exists("sapi_windows_vt100_support") && {
            let stream = php_fopen_resource("php://stdout", "w");
            sapi_windows_vt100_support(&stream)
        }
    }

    /// Initializes dimensions using the output of an stty columns line.
    fn init_dimensions_using_stty() {
        if let Some(stty_string) = Self::get_stty_columns() {
            if stty_string.is_empty() {
                return;
            }
            if let Some(matches) =
                preg_match(php_regex!("/rows.(\\d+);.columns.(\\d+);/i"), &stty_string)
            {
                // extract [w, h] from "rows h; columns w;"
                WIDTH.with(|w| {
                    w.set(Some(intval(&PhpMixed::string(
                        matches.get(2).unwrap_or_default(),
                    ))))
                });
                HEIGHT.with(|h| {
                    h.set(Some(intval(&PhpMixed::string(
                        matches.get(1).unwrap_or_default(),
                    ))))
                });
            } else if let Some(matches) =
                preg_match(php_regex!("/;.(\\d+).rows;.(\\d+).columns/i"), &stty_string)
            {
                // extract [w, h] from "; h rows; w columns"
                WIDTH.with(|w| {
                    w.set(Some(intval(&PhpMixed::string(
                        matches.get(2).unwrap_or_default(),
                    ))))
                });
                HEIGHT.with(|h| {
                    h.set(Some(intval(&PhpMixed::string(
                        matches.get(1).unwrap_or_default(),
                    ))))
                });
            }
        }
    }

    /// Runs and parses mode CON if it's available, suppressing any error output.
    ///
    /// Returns an array composed of the width and the height or null if it could not be parsed.
    fn get_console_mode() -> Option<Vec<i64>> {
        let info = Self::read_from_process("mode CON");

        let info = info?;
        let matches = preg_match(
            php_regex!("/--------+\\r?\\n.+?(\\d+)\\r?\\n.+?(\\d+)\\r?\\n/"),
            &info,
        )?;

        Some(vec![
            intval(&PhpMixed::string(matches.get(2).unwrap_or_default())),
            intval(&PhpMixed::string(matches.get(1).unwrap_or_default())),
        ])
    }

    /// Runs and parses stty -a if it's available, suppressing any error output.
    fn get_stty_columns() -> Option<Vec<u8>> {
        Self::read_from_process("stty -a | grep columns")
    }

    fn read_from_process(command: impl AsRef<[u8]>) -> Option<Vec<u8>> {
        // Sparse PHP descriptorspec `[1 => ['pipe', 'w'], 2 => ['pipe', 'w']]`: fd 0 is inherited.
        let descriptorspec = [
            Descriptor::Inherit,
            Descriptor::Pipe(BString::from("w")),
            Descriptor::Pipe(BString::from("w")),
        ];

        let cp = if function_exists("sapi_windows_cp_set") {
            sapi_windows_cp_get(None)
        } else {
            0
        };

        let mut pipes: indexmap::IndexMap<i64, PhpResource> = indexmap::IndexMap::new();
        let process = match proc_open(
            command.as_ref(),
            &descriptorspec,
            &mut pipes,
            None,
            None,
            None,
        ) {
            Ok(process) => process,
            Err(_) => return None,
        };

        let info = pipes.get(&1).and_then(stream_get_contents);
        if let Some(pipe) = pipes.get(&1) {
            fclose(pipe);
        }
        if let Some(pipe) = pipes.get(&2) {
            fclose(pipe);
        }
        proc_close(&process);

        if cp != 0 {
            sapi_windows_cp_set(cp);
        }

        info
    }
}
