//! ref: composer/src/Composer/IO/NullIO.php

use crate::io::base_io::BaseIO;
use crate::io::io_interface::IOInterface;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct NullIO {
    inner: BaseIO,
}

impl IOInterface for NullIO {
    fn is_interactive(&self) -> bool {
        false
    }

    fn is_verbose(&self) -> bool {
        false
    }

    fn is_very_verbose(&self) -> bool {
        false
    }

    fn is_debug(&self) -> bool {
        false
    }

    fn is_decorated(&self) -> bool {
        false
    }

    fn write(&self, _messages: PhpMixed, _newline: bool, _verbosity: i64) {}

    fn write_error(&self, _messages: PhpMixed, _newline: bool, _verbosity: i64) {}

    fn overwrite(&self, _messages: PhpMixed, _newline: bool, _size: Option<i64>, _verbosity: i64) {}

    fn overwrite_error(
        &self,
        _messages: PhpMixed,
        _newline: bool,
        _size: Option<i64>,
        _verbosity: i64,
    ) {
    }

    fn ask(&self, _question: String, default: PhpMixed) -> PhpMixed {
        default
    }

    fn ask_confirmation(&self, _question: String, default: bool) -> bool {
        default
    }

    fn ask_and_validate(
        &self,
        _question: String,
        _validator: Box<dyn Fn(PhpMixed) -> PhpMixed>,
        _attempts: Option<i64>,
        default: PhpMixed,
    ) -> PhpMixed {
        default
    }

    fn ask_and_hide_answer(&self, _question: String) -> Option<String> {
        None
    }

    fn select(
        &self,
        _question: String,
        _choices: Vec<String>,
        default: PhpMixed,
        _attempts: PhpMixed,
        _error_message: String,
        _multiselect: bool,
    ) -> PhpMixed {
        default
    }
}
