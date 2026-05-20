//! ref: composer/src/Composer/IO/NullIO.php

use crate::io::BaseIO;
use crate::io::IOInterface;
use shirabe_external_packages::psr::log::LoggerInterface;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct NullIO {
    authentications: indexmap::IndexMap<String, indexmap::IndexMap<String, Option<String>>>,
}

impl NullIO {
    pub fn new() -> Self {
        Self {
            authentications: indexmap::IndexMap::new(),
        }
    }
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

    fn write3(&self, _message: &str, _newline: bool, _verbosity: i64) {}

    fn write_error3(&self, _message: &str, _newline: bool, _verbosity: i64) {}

    fn write_raw3(&self, _message: &str, _newline: bool, _verbosity: i64) {}

    fn write_error_raw3(&self, _message: &str, _newline: bool, _verbosity: i64) {}

    fn overwrite4(&self, _message: &str, _newline: bool, _size: Option<i64>, _verbosity: i64) {}

    fn overwrite_error4(
        &self,
        _message: &str,
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

    fn get_authentications(
        &self,
    ) -> indexmap::IndexMap<String, indexmap::IndexMap<String, Option<String>>> {
        <Self as BaseIO>::get_authentications(self)
    }

    fn has_authentication(&self, repository_name: &str) -> bool {
        <Self as BaseIO>::has_authentication(self, repository_name)
    }

    fn get_authentication(
        &self,
        repository_name: &str,
    ) -> indexmap::IndexMap<String, Option<String>> {
        <Self as BaseIO>::get_authentication(self, repository_name)
    }

    fn set_authentication(
        &mut self,
        repository_name: String,
        username: String,
        password: Option<String>,
    ) {
        <Self as BaseIO>::set_authentication(self, repository_name, username, password)
    }

    fn load_configuration(&mut self, config: &mut crate::config::Config) -> anyhow::Result<()> {
        <Self as BaseIO>::load_configuration(self, config)
    }
}

impl BaseIO for NullIO {
    fn authentications(
        &self,
    ) -> &indexmap::IndexMap<String, indexmap::IndexMap<String, Option<String>>> {
        &self.authentications
    }

    fn authentications_mut(
        &mut self,
    ) -> &mut indexmap::IndexMap<String, indexmap::IndexMap<String, Option<String>>> {
        &mut self.authentications
    }
}

impl LoggerInterface for NullIO {
    fn emergency(&self, message: &str, context: &[(&str, &str)]) {
        todo!()
    }

    fn alert(&self, message: &str, context: &[(&str, &str)]) {
        todo!()
    }

    fn critical(&self, message: &str, context: &[(&str, &str)]) {
        todo!()
    }

    fn error(&self, message: &str, context: &[(&str, &str)]) {
        todo!()
    }

    fn warning(&self, message: &str, context: &[(&str, &str)]) {
        todo!()
    }

    fn notice(&self, message: &str, context: &[(&str, &str)]) {
        todo!()
    }

    fn info(&self, message: &str, context: &[(&str, &str)]) {
        todo!()
    }

    fn debug(&self, message: &str, context: &[(&str, &str)]) {
        todo!()
    }

    fn log(&self, level: &str, message: &str, context: &[(&str, &str)]) {
        todo!()
    }
}
