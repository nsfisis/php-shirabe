//! ref: composer/vendor/symfony/console/Cursor.php

use crate::symfony::console::output::OutputInterface;
use crate::symfony::console::output::output_interface;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug)]
pub struct Cursor {
    output: Rc<RefCell<dyn OutputInterface>>,
    input: shirabe_php_shim::PhpResource,
}

impl Cursor {
    pub fn new(
        output: Rc<RefCell<dyn OutputInterface>>,
        input: Option<shirabe_php_shim::PhpResource>,
    ) -> Self {
        let input = input.unwrap_or(shirabe_php_shim::STDIN);

        Self { output, input }
    }

    pub fn move_up(&self, lines: i64) -> &Self {
        self.output.borrow().write(
            &[format!("\x1b[{}A", lines)],
            false,
            output_interface::OUTPUT_NORMAL,
        );

        self
    }

    pub fn move_down(&self, lines: i64) -> &Self {
        self.output.borrow().write(
            &[format!("\x1b[{}B", lines)],
            false,
            output_interface::OUTPUT_NORMAL,
        );

        self
    }

    pub fn move_right(&self, columns: i64) -> &Self {
        self.output.borrow().write(
            &[format!("\x1b[{}C", columns)],
            false,
            output_interface::OUTPUT_NORMAL,
        );

        self
    }

    pub fn move_left(&self, columns: i64) -> &Self {
        self.output.borrow().write(
            &[format!("\x1b[{}D", columns)],
            false,
            output_interface::OUTPUT_NORMAL,
        );

        self
    }

    pub fn move_to_column(&self, column: i64) -> &Self {
        self.output.borrow().write(
            &[format!("\x1b[{}G", column)],
            false,
            output_interface::OUTPUT_NORMAL,
        );

        self
    }

    pub fn move_to_position(&self, column: i64, row: i64) -> &Self {
        self.output.borrow().write(
            &[format!("\x1b[{};{}H", row + 1, column)],
            false,
            output_interface::OUTPUT_NORMAL,
        );

        self
    }

    pub fn save_position(&self) -> &Self {
        self.output.borrow().write(
            &["\x1b7".to_string()],
            false,
            output_interface::OUTPUT_NORMAL,
        );

        self
    }

    pub fn restore_position(&self) -> &Self {
        self.output.borrow().write(
            &["\x1b8".to_string()],
            false,
            output_interface::OUTPUT_NORMAL,
        );

        self
    }

    pub fn hide(&self) -> &Self {
        self.output.borrow().write(
            &["\x1b[?25l".to_string()],
            false,
            output_interface::OUTPUT_NORMAL,
        );

        self
    }

    pub fn show(&self) -> &Self {
        self.output.borrow().write(
            &["\x1b[?25h\x1b[?0c".to_string()],
            false,
            output_interface::OUTPUT_NORMAL,
        );

        self
    }

    /// Clears all the output from the current line.
    pub fn clear_line(&self) -> &Self {
        self.output.borrow().write(
            &["\x1b[2K".to_string()],
            false,
            output_interface::OUTPUT_NORMAL,
        );

        self
    }

    /// Clears all the output from the current line after the current position.
    pub fn clear_line_after(&self) -> &Self {
        self.output.borrow().write(
            &["\x1b[K".to_string()],
            false,
            output_interface::OUTPUT_NORMAL,
        );

        self
    }

    /// Clears all the output from the cursors' current position to the end of the screen.
    pub fn clear_output(&self) -> &Self {
        self.output.borrow().write(
            &["\x1b[0J".to_string()],
            false,
            output_interface::OUTPUT_NORMAL,
        );

        self
    }

    /// Clears the entire screen.
    pub fn clear_screen(&self) -> &Self {
        self.output.borrow().write(
            &["\x1b[2J".to_string()],
            false,
            output_interface::OUTPUT_NORMAL,
        );

        self
    }

    /// Returns the current cursor position as x,y coordinates.
    pub fn get_current_position(&self) -> Vec<i64> {
        static IS_TTY_SUPPORTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

        let is_tty_supported = if shirabe_php_shim::function_exists("proc_open") {
            *IS_TTY_SUPPORTED.get_or_init(|| {
                let mut pipes = indexmap::IndexMap::new();
                shirabe_php_shim::proc_open(
                    "echo 1 >/dev/null",
                    &[
                        shirabe_php_shim::Descriptor::File("/dev/tty".to_string(), "r".to_string()),
                        shirabe_php_shim::Descriptor::File("/dev/tty".to_string(), "w".to_string()),
                        shirabe_php_shim::Descriptor::File("/dev/tty".to_string(), "w".to_string()),
                    ],
                    &mut pipes,
                    None,
                    None,
                    None,
                )
                .is_ok()
            })
        } else {
            false
        };

        if !is_tty_supported {
            return vec![1, 1];
        }

        let stty_mode = shirabe_php_shim::shell_exec("stty -g");
        shirabe_php_shim::shell_exec("stty -icanon -echo");

        shirabe_php_shim::fwrite(&self.input, "\x1b[6n", None);

        let code = shirabe_php_shim::trim(
            shirabe_php_shim::fread(&self.input, 1024)
                .as_deref()
                .unwrap_or(""),
            None,
        );

        shirabe_php_shim::shell_exec(&format!("stty {}", stty_mode.unwrap_or_default()));

        let mut row: i64 = 0;
        let mut col: i64 = 0;
        shirabe_php_shim::sscanf(&code, "\x1b[%d;%dR", &mut row, &mut col);

        vec![col, row]
    }
}
