//! ref: composer/src/Composer/Util/ErrorHandler.php

use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use shirabe_php_shim::{
    E_ALL, E_DEPRECATED, E_USER_DEPRECATED, E_USER_WARNING, E_WARNING, ErrorException, PHP_EOL,
    PhpMixed, STDERR, debug_backtrace, error_reporting, filter_var_boolean, ini_get, is_resource,
    set_error_handler,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

// PHP keeps `$io` / `$hasShownDeprecationNotice` as process-global statics. Composer runs
// single-threaded, so thread-locals on the main thread reproduce that faithfully while letting
// us hold the same shared (`Rc<RefCell<dyn IOInterface>>`) IO instance the application uses.
thread_local! {
    static IO: RefCell<Option<Rc<RefCell<dyn IOInterface>>>> = const { RefCell::new(None) };
    static HAS_SHOWN_DEPRECATION_NOTICE: Cell<i64> = const { Cell::new(0) };
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
        if filter_var_boolean(&xdebug_scream) {
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

        let io = IO.with(|cell| cell.borrow().clone());
        if let Some(io) = io {
            let has_shown = HAS_SHOWN_DEPRECATION_NOTICE.with(|c| c.get());
            if has_shown > 0 && !io.is_verbose() {
                if has_shown == 1 {
                    io.write_error("<warning>More deprecation notices were hidden, run again with `-v` to show them.</warning>");
                    HAS_SHOWN_DEPRECATION_NOTICE.with(|c| c.set(2));
                }
                return Ok(true);
            }
            HAS_SHOWN_DEPRECATION_NOTICE.with(|c| c.set(1));
            Self::output_warning(
                &format!("Deprecation Notice: {} in {}:{}", message, file, line),
                false,
            );
        }

        Ok(true)
    }

    pub fn register(io: Option<Rc<RefCell<dyn IOInterface>>>) {
        set_error_handler(|level, message, file, line| {
            Self::handle(level, message.to_string(), file.to_string(), line).unwrap_or(true)
        });
        error_reporting(Some(E_ALL));
        IO.with(|cell| *cell.borrow_mut() = io);
    }

    fn output_warning(message: &str, output_even_without_io: bool) {
        let io = IO.with(|cell| cell.borrow().clone());
        if let Some(io) = io {
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
