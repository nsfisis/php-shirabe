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

pub trait IOInterface: LoggerInterface {
    fn is_interactive(&self) -> bool;

    fn is_verbose(&self) -> bool;

    fn is_very_verbose(&self) -> bool;

    fn is_debug(&self) -> bool;

    fn is_decorated(&self) -> bool;

    fn write(&self, messages: PhpMixed, newline: bool, verbosity: i64);

    fn write_error(&self, messages: PhpMixed, newline: bool, verbosity: i64);

    fn write_raw(&self, messages: PhpMixed, newline: bool, verbosity: i64);

    fn write_error_raw(&self, messages: PhpMixed, newline: bool, verbosity: i64);

    fn overwrite(&self, messages: PhpMixed, newline: bool, size: Option<i64>, verbosity: i64);

    fn overwrite_error(&self, messages: PhpMixed, newline: bool, size: Option<i64>, verbosity: i64);

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

    fn load_configuration(&mut self, config: &Config);
}
