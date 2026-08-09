//! ref: composer/vendor/symfony/console/Cursor.php

use crate::output::OutputInterface;
use crate::output::output_interface;

#[derive(Debug)]
pub struct Cursor {
    output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    input: shirabe_php_shim::PhpResource,
}

impl Cursor {
    pub fn new(
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
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
}
