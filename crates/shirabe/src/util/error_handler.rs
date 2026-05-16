//! ref: composer/src/Composer/Util/ErrorHandler.php

use crate::io::io_interface::IOInterface;
use shirabe_php_shim::{
    E_ALL, E_DEPRECATED, E_USER_DEPRECATED, E_USER_WARNING, E_WARNING, ErrorException,
    FILTER_VALIDATE_BOOLEAN, PHP_EOL, PhpMixed, STDERR, debug_backtrace, error_reporting,
    filter_var, ini_get, is_resource, set_error_handler,
};
use std::sync::{Mutex, OnceLock};

static IO: OnceLock<Mutex<Option<Box<dyn IOInterface + Send>>>> = OnceLock::new();
static HAS_SHOWN_DEPRECATION_NOTICE: Mutex<i64> = Mutex::new(0);

fn io() -> &'static Mutex<Option<Box<dyn IOInterface + Send>>> {
    IO.get_or_init(|| Mutex::new(None))
}

pub struct ErrorHandler;

impl ErrorHandler {
    pub fn handle(
        level: i64,
        message: String,
        file: String,
        line: i64,
    ) -> Result<bool, ErrorException> {
        let is_deprecation_notice = level == E_DEPRECATED || level == E_USER_DEPRECATED;

        // error code is not included in error_reporting
        if !is_deprecation_notice && 0 == (error_reporting(None) & level) {
            return Ok(true);
        }

        let mut message = message;

        let xdebug_scream = ini_get("xdebug.scream").unwrap_or_default();
        if filter_var(&xdebug_scream, FILTER_VALIDATE_BOOLEAN) {
            message += "\n\nWarning: You have xdebug.scream enabled, the warning above may be\na legitimately suppressed error that you were not supposed to see.";
        }

        if !is_deprecation_notice {
            // ignore some newly introduced warnings in new php versions until dependencies
            // can be fixed as we do not want to abort execution for those
            if (level == E_WARNING || level == E_USER_WARNING)
                && message.contains(
                    "should either be used or intentionally ignored by casting it as (void)",
                )
            {
                Self::output_warning(
                    &format!(
                        "Ignored new PHP warning but it should be reported and fixed: {} in {}:{}",
                        message, file, line
                    ),
                    true,
                );
                return Ok(true);
            }

            return Err(ErrorException {
                message,
                code: 0,
                severity: level,
                filename: file,
                lineno: line,
            });
        }

        let io_guard = io().lock().unwrap();
        if io_guard.is_some() {
            let has_shown = *HAS_SHOWN_DEPRECATION_NOTICE.lock().unwrap();
            if has_shown > 0 && !io_guard.as_ref().unwrap().is_verbose() {
                if has_shown == 1 {
                    io_guard.as_ref().unwrap().write_error("<warning>More deprecation notices were hidden, run again with `-v` to show them.</warning>");
                    *HAS_SHOWN_DEPRECATION_NOTICE.lock().unwrap() = 2;
                }
                return Ok(true);
            }
            *HAS_SHOWN_DEPRECATION_NOTICE.lock().unwrap() = 1;
            drop(io_guard);
            Self::output_warning(
                &format!("Deprecation Notice: {} in {}:{}", message, file, line),
                false,
            );
        }

        Ok(true)
    }

    pub fn register(io: Option<Box<dyn IOInterface + Send>>) {
        set_error_handler(|level, message, file, line| {
            Self::handle(level, message.to_string(), file.to_string(), line).unwrap_or(true)
        });
        error_reporting(Some(E_ALL));
        *self::io().lock().unwrap() = io;
    }

    fn output_warning(message: &str, output_even_without_io: bool) {
        let io_guard = io().lock().unwrap();
        if let Some(ref io) = *io_guard {
            io.write_error(&format!("<warning>{}</warning>", message));
            if io.is_verbose() {
                io.write_error("<warning>Stack trace:</warning>");
                let frames: Vec<String> = debug_backtrace()
                    .into_iter()
                    .skip(2)
                    .filter_map(|frame| {
                        let line = frame.get("line").and_then(|v| v.as_int());
                        let file = frame
                            .get("file")
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_string());
                        if let (Some(line), Some(file)) = (line, file) {
                            Some(format!("<warning> {}:{}</warning>", file, line))
                        } else {
                            None
                        }
                    })
                    .collect();
                for frame_str in frames {
                    io.write_error(&frame_str);
                }
            }
            return;
        }
        drop(io_guard);

        if output_even_without_io {
            if is_resource(&PhpMixed::Int(STDERR)) {
                shirabe_php_shim::fwrite(
                    PhpMixed::Int(STDERR),
                    &format!("Warning: {}{}", message, PHP_EOL),
                    -1,
                );
            } else {
                print!("Warning: {}{}", message, PHP_EOL);
            }
        }
    }
}
