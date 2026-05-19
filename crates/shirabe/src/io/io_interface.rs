//! ref: composer/src/Composer/IO/IOInterface.php

use crate::config::Config;
use indexmap::IndexMap;
use shirabe_external_packages::psr::log::logger_interface::LoggerInterface;
use shirabe_php_shim::PhpMixed;

pub const QUIET: i64 = 1;
pub const NORMAL: i64 = 2;
pub const VERBOSE: i64 = 4;
pub const VERY_VERBOSE: i64 = 8;
pub const DEBUG: i64 = 16;

pub trait IOInterface: LoggerInterface + std::fmt::Debug {
    fn is_interactive(&self) -> bool;

    fn is_verbose(&self) -> bool;

    fn is_very_verbose(&self) -> bool;

    fn is_debug(&self) -> bool;

    fn is_decorated(&self) -> bool;

    fn write(&self, message: &str) {
        self.write3(message, true, NORMAL)
    }
    fn write2(&self, message: &str, newline: bool) {
        self.write3(message, newline, NORMAL)
    }
    fn write_no_newline(&self, message: &str) {
        self.write3(message, false, NORMAL)
    }
    fn write3(&self, message: &str, newline: bool, verbosity: i64);

    fn write_error(&self, message: &str) {
        self.write_error3(message, true, NORMAL)
    }
    fn write_error2(&self, message: &str, newline: bool) {
        self.write_error3(message, newline, NORMAL)
    }
    fn write_error_no_newline(&self, message: &str) {
        self.write_error3(message, false, NORMAL)
    }
    fn write_error3(&self, message: &str, newline: bool, verbosity: i64);

    fn write_raw(&self, message: &str) {
        self.write_raw3(message, true, NORMAL)
    }
    fn write_raw2(&self, message: &str, newline: bool) {
        self.write_raw3(message, newline, NORMAL)
    }
    fn write_raw3(&self, message: &str, newline: bool, verbosity: i64);

    fn write_error_raw(&self, message: &str) {
        self.write_error_raw3(message, true, NORMAL)
    }
    fn write_error_raw2(&self, message: &str, newline: bool) {
        self.write_error_raw3(message, newline, NORMAL)
    }
    fn write_error_raw3(&self, message: &str, newline: bool, verbosity: i64);

    fn overwrite(&self, message: &str) {
        self.overwrite4(message, true, None, NORMAL)
    }
    fn overwrite2(&self, message: &str, newline: bool) {
        self.overwrite4(message, newline, None, NORMAL)
    }
    fn overwrite3(&self, message: &str, newline: bool, size: Option<i64>) {
        self.overwrite4(message, newline, size, NORMAL)
    }
    fn overwrite4(&self, message: &str, newline: bool, size: Option<i64>, verbosity: i64);

    fn overwrite_error(&self, message: &str) {
        self.overwrite_error4(message, true, None, NORMAL)
    }
    fn overwrite_error2(&self, message: &str, newline: bool) {
        self.overwrite_error4(message, newline, None, NORMAL)
    }
    fn overwrite_error3(&self, message: &str, newline: bool, size: Option<i64>) {
        self.overwrite_error4(message, newline, size, NORMAL)
    }
    fn overwrite_error4(&self, message: &str, newline: bool, size: Option<i64>, verbosity: i64);

    fn ask(&self, question: String, default: PhpMixed) -> PhpMixed;

    fn ask_confirmation(&self, question: String, default: bool) -> bool;

    fn ask_and_validate(
        &self,
        question: String,
        validator: Box<dyn Fn(PhpMixed) -> PhpMixed>,
        attempts: Option<i64>,
        default: PhpMixed,
    ) -> PhpMixed;

    fn ask_and_hide_answer(&self, question: String) -> Option<String>;

    fn select(
        &self,
        question: String,
        choices: Vec<String>,
        default: PhpMixed,
        attempts: PhpMixed,
        error_message: String,
        multiselect: bool,
    ) -> PhpMixed;

    fn get_authentications(&self) -> IndexMap<String, IndexMap<String, Option<String>>>;

    fn has_authentication(&self, repository_name: &str) -> bool;

    fn get_authentication(&self, repository_name: &str) -> IndexMap<String, Option<String>>;

    fn set_authentication(
        &mut self,
        repository_name: String,
        username: String,
        password: Option<String>,
    );

    fn load_configuration(&mut self, config: &mut Config) -> anyhow::Result<()>;

    fn clone_box(&self) -> Box<dyn IOInterface> {
        todo!()
    }
}
