//! ref: composer/vendor/symfony/console/Terminal.php

use shirabe_php_shim::PhpMixed;
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
        let width = shirabe_php_shim::getenv("COLUMNS");
        if let Some(width) = width {
            return shirabe_php_shim::intval(&PhpMixed::String(shirabe_php_shim::trim(
                &width, None,
            )));
        }

        if WIDTH.with(|w| w.get()).is_none() {
            Self::init_dimensions();
        }

        WIDTH.with(|w| w.get()).filter(|&v| v != 0).unwrap_or(80)
    }

    /// Gets the terminal height.
    pub fn get_height(&self) -> i64 {
        let height = shirabe_php_shim::getenv("LINES");
        if let Some(height) = height {
            return shirabe_php_shim::intval(&PhpMixed::String(shirabe_php_shim::trim(
                &height, None,
            )));
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

        // skip check if shell_exec function is disabled
        if !shirabe_php_shim::function_exists("shell_exec") {
            return false;
        }

        let result = shirabe_php_shim::shell_exec(&format!(
            "stty 2> {}",
            if shirabe_php_shim::DIRECTORY_SEPARATOR == "\\" {
                "NUL"
            } else {
                "/dev/null"
            }
        ))
        .is_some();
        STTY.with(|s| s.set(Some(result)));
        result
    }

    fn init_dimensions() {
        if shirabe_php_shim::DIRECTORY_SEPARATOR == "\\" {
            let ansicon = shirabe_php_shim::getenv("ANSICON");
            let mut matches: Vec<Option<String>> = Vec::new();
            if let Some(ansicon) = &ansicon
                && shirabe_php_shim::preg_match(
                    "/^(\\d+)x(\\d+)(?: \\((\\d+)x(\\d+)\\))?$/",
                    &shirabe_php_shim::trim(ansicon, None),
                    &mut matches,
                )
            {
                // extract [w, H] from "wxh (WxH)"
                // or [w, h] from "wxh"
                WIDTH.with(|w| {
                    w.set(Some(shirabe_php_shim::intval(&PhpMixed::String(
                        matches[1].clone().unwrap_or_default(),
                    ))))
                });
                HEIGHT.with(|h| {
                    let value = if matches.get(4).map(|m| m.is_some()).unwrap_or(false) {
                        shirabe_php_shim::intval(&PhpMixed::String(
                            matches[4].clone().unwrap_or_default(),
                        ))
                    } else {
                        shirabe_php_shim::intval(&PhpMixed::String(
                            matches[2].clone().unwrap_or_default(),
                        ))
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
        shirabe_php_shim::function_exists("sapi_windows_vt100_support") && {
            let stream = shirabe_php_shim::php_fopen_resource("php://stdout", "w");
            shirabe_php_shim::sapi_windows_vt100_support(&stream)
        }
    }

    /// Initializes dimensions using the output of an stty columns line.
    fn init_dimensions_using_stty() {
        if let Some(stty_string) = Self::get_stty_columns() {
            if stty_string.is_empty() {
                return;
            }
            let mut matches: Vec<Option<String>> = Vec::new();
            if shirabe_php_shim::preg_match(
                "/rows.(\\d+);.columns.(\\d+);/i",
                &stty_string,
                &mut matches,
            ) {
                // extract [w, h] from "rows h; columns w;"
                WIDTH.with(|w| {
                    w.set(Some(shirabe_php_shim::intval(&PhpMixed::String(
                        matches[2].clone().unwrap_or_default(),
                    ))))
                });
                HEIGHT.with(|h| {
                    h.set(Some(shirabe_php_shim::intval(&PhpMixed::String(
                        matches[1].clone().unwrap_or_default(),
                    ))))
                });
            } else if shirabe_php_shim::preg_match(
                "/;.(\\d+).rows;.(\\d+).columns/i",
                &stty_string,
                &mut matches,
            ) {
                // extract [w, h] from "; h rows; w columns"
                WIDTH.with(|w| {
                    w.set(Some(shirabe_php_shim::intval(&PhpMixed::String(
                        matches[2].clone().unwrap_or_default(),
                    ))))
                });
                HEIGHT.with(|h| {
                    h.set(Some(shirabe_php_shim::intval(&PhpMixed::String(
                        matches[1].clone().unwrap_or_default(),
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
        let mut matches: Vec<Option<String>> = Vec::new();
        if !shirabe_php_shim::preg_match(
            "/--------+\\r?\\n.+?(\\d+)\\r?\\n.+?(\\d+)\\r?\\n/",
            &info,
            &mut matches,
        ) {
            return None;
        }

        Some(vec![
            shirabe_php_shim::intval(&PhpMixed::String(matches[2].clone().unwrap_or_default())),
            shirabe_php_shim::intval(&PhpMixed::String(matches[1].clone().unwrap_or_default())),
        ])
    }

    /// Runs and parses stty -a if it's available, suppressing any error output.
    fn get_stty_columns() -> Option<String> {
        Self::read_from_process("stty -a | grep columns")
    }

    fn read_from_process(command: &str) -> Option<String> {
        if !shirabe_php_shim::function_exists("proc_open") {
            return None;
        }

        // Sparse PHP descriptorspec `[1 => ['pipe', 'w'], 2 => ['pipe', 'w']]`: fd 0 is inherited.
        let descriptorspec = [
            shirabe_php_shim::Descriptor::Inherit,
            shirabe_php_shim::Descriptor::Pipe("w".to_string()),
            shirabe_php_shim::Descriptor::Pipe("w".to_string()),
        ];

        let cp = if shirabe_php_shim::function_exists("sapi_windows_cp_set") {
            shirabe_php_shim::sapi_windows_cp_get(None)
        } else {
            0
        };

        let mut pipes: indexmap::IndexMap<i64, shirabe_php_shim::PhpResource> =
            indexmap::IndexMap::new();
        let process = match shirabe_php_shim::proc_open(
            command,
            &descriptorspec,
            &mut pipes,
            None,
            None,
            None,
        ) {
            Ok(process) => process,
            Err(_) => return None,
        };

        let info = pipes
            .get(&1)
            .and_then(shirabe_php_shim::stream_get_contents);
        if let Some(pipe) = pipes.get(&1) {
            shirabe_php_shim::fclose(pipe);
        }
        if let Some(pipe) = pipes.get(&2) {
            shirabe_php_shim::fclose(pipe);
        }
        shirabe_php_shim::proc_close(&process);

        if cp != 0 {
            shirabe_php_shim::sapi_windows_cp_set(cp);
        }

        info
    }
}
