//! ref: composer/src/Composer/IO/NullIO.php

use crate::io::{BaseIO, IOInterface, IOInterfaceImmutable, IOInterfaceMutable};
use bstr::BString;
use indexmap::IndexMap;
use shirabe_symfony_console::input::InputValue;

#[derive(Debug)]
pub struct NullIO {
    authentications:
        indexmap::IndexMap<bstr::BString, indexmap::IndexMap<bstr::BString, Option<bstr::BString>>>,
}

impl Default for NullIO {
    fn default() -> Self {
        Self::new()
    }
}

impl NullIO {
    pub fn new() -> Self {
        Self {
            authentications: indexmap::IndexMap::new(),
        }
    }
}

impl IOInterfaceImmutable for NullIO {
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

    fn write_bytes(&self, _message: &bstr::BStr, _newline: bool, _verbosity: i64) {}

    fn write_error_bytes(&self, _message: &bstr::BStr, _newline: bool, _verbosity: i64) {}

    fn write_raw_bytes(&self, _message: &bstr::BStr, _newline: bool, _verbosity: i64) {}

    fn write_error_raw_bytes(&self, _message: &bstr::BStr, _newline: bool, _verbosity: i64) {}

    fn overwrite_bytes(
        &self,
        _message: &bstr::BStr,
        _newline: bool,
        _size: Option<i64>,
        _verbosity: i64,
    ) {
    }

    fn overwrite_error_bytes(
        &self,
        _message: &bstr::BStr,
        _newline: bool,
        _size: Option<i64>,
        _verbosity: i64,
    ) {
    }

    fn ask_bytes(&self, _question: &bstr::BStr, default: InputValue) -> anyhow::Result<InputValue> {
        Ok(default)
    }

    fn ask_confirmation_bytes(
        &self,
        _question: &bstr::BStr,
        default: bool,
    ) -> anyhow::Result<bool> {
        Ok(default)
    }

    fn ask_and_validate_bytes(
        &self,
        _question: &bstr::BStr,
        _validator: Box<dyn Fn(InputValue) -> anyhow::Result<InputValue>>,
        _attempts: Option<i64>,
        default: InputValue,
    ) -> anyhow::Result<InputValue> {
        Ok(default)
    }

    fn ask_and_hide_answer_bytes(
        &self,
        _question: &bstr::BStr,
    ) -> anyhow::Result<Option<bstr::BString>> {
        Ok(None)
    }

    fn select_bytes(
        &self,
        _question: &bstr::BStr,
        _choices: IndexMap<BString, BString>,
        default: InputValue,
        _attempts: Option<i64>,
        _error_message: &bstr::BStr,
        _multiselect: bool,
    ) -> anyhow::Result<InputValue> {
        Ok(default)
    }

    fn get_authentications(
        &self,
    ) -> indexmap::IndexMap<bstr::BString, indexmap::IndexMap<bstr::BString, Option<bstr::BString>>>
    {
        <Self as BaseIO>::get_authentications(self)
    }

    fn has_authentication(&self, repository_name: &bstr::BStr) -> bool {
        <Self as BaseIO>::has_authentication(self, repository_name)
    }

    fn get_authentication(
        &self,
        repository_name: &bstr::BStr,
    ) -> indexmap::IndexMap<bstr::BString, Option<bstr::BString>> {
        <Self as BaseIO>::get_authentication(self, repository_name)
    }

    fn error_bytes(&self, message: &bstr::BStr, context: &[(&bstr::BStr, &bstr::BStr)]) {
        <Self as BaseIO>::error(self, message, context);
    }

    fn warning_bytes(&self, message: &bstr::BStr, context: &[(&bstr::BStr, &bstr::BStr)]) {
        <Self as BaseIO>::warning(self, message, context);
    }

    fn debug_bytes(&self, message: &bstr::BStr, context: &[(&bstr::BStr, &bstr::BStr)]) {
        <Self as BaseIO>::debug(self, message, context);
    }
}

impl IOInterfaceMutable for NullIO {
    fn set_authentication(
        &mut self,
        repository_name: bstr::BString,
        username: bstr::BString,
        password: Option<bstr::BString>,
    ) {
        <Self as BaseIO>::set_authentication(self, repository_name, username, password)
    }

    fn load_configuration(&mut self, config: &mut crate::config::Config) -> anyhow::Result<()> {
        <Self as BaseIO>::load_configuration(self, config)
    }
}

impl IOInterface for NullIO {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_base_io_mut(&mut self) -> Option<&mut dyn BaseIO> {
        Some(self)
    }
}

impl BaseIO for NullIO {
    fn authentications(
        &self,
    ) -> &indexmap::IndexMap<bstr::BString, indexmap::IndexMap<bstr::BString, Option<bstr::BString>>>
    {
        &self.authentications
    }

    fn authentications_mut(
        &mut self,
    ) -> &mut indexmap::IndexMap<
        bstr::BString,
        indexmap::IndexMap<bstr::BString, Option<bstr::BString>>,
    > {
        &mut self.authentications
    }
}
