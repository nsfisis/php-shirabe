//! Configurable IOInterface stub, equivalent to PHPUnit's
//! `getMockBuilder(IOInterface::class)->getMock()` where individual methods are
//! configured via `->method('x')->willReturn(y)`.
//!
//! Each configurable method is backed by an `Option<ReturnValue>` field plus a
//! builder-style setter. Unset methods fall back to NullIO-equivalent defaults.
#![allow(dead_code)]

use shirabe::config::Config;
use shirabe::io::{BaseIO, IOInterface, IOInterfaceImmutable, IOInterfaceMutable};
use shirabe_php_shim::PhpMixed;

#[derive(Debug, Default)]
pub struct IOStub {
    authentications: indexmap::IndexMap<String, indexmap::IndexMap<String, Option<String>>>,

    is_interactive: Option<bool>,
    is_decorated: Option<bool>,
    is_verbose: Option<bool>,
    is_very_verbose: Option<bool>,
    is_debug: Option<bool>,

    // `hasAuthentication` is keyed: PHPUnit tests usually configure a single bool
    // willReturn that applies to any argument, so a single Option<bool> matches.
    has_authentication: Option<bool>,
    // `getAuthentication` willReturn is a `['username' => ..., 'password' => ...]` array.
    get_authentication: Option<indexmap::IndexMap<String, Option<String>>>,

    ask: Option<PhpMixed>,
    ask_confirmation: Option<bool>,
    ask_and_hide_answer: Option<Option<String>>,
    // Keyed `askAndHideAnswer` replies, mirroring tests whose willReturnCallback
    // switches on the question string. Unknown questions return `''` like PHP's
    // `switch` default.
    ask_and_hide_answer_responses: Option<indexmap::IndexMap<String, String>>,
}

impl IOStub {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_is_interactive(mut self, value: bool) -> Self {
        self.is_interactive = Some(value);
        self
    }
    pub fn with_is_decorated(mut self, value: bool) -> Self {
        self.is_decorated = Some(value);
        self
    }
    pub fn with_is_verbose(mut self, value: bool) -> Self {
        self.is_verbose = Some(value);
        self
    }
    pub fn with_is_very_verbose(mut self, value: bool) -> Self {
        self.is_very_verbose = Some(value);
        self
    }
    pub fn with_is_debug(mut self, value: bool) -> Self {
        self.is_debug = Some(value);
        self
    }
    pub fn with_has_authentication(mut self, value: bool) -> Self {
        self.has_authentication = Some(value);
        self
    }
    pub fn with_get_authentication(
        mut self,
        value: indexmap::IndexMap<String, Option<String>>,
    ) -> Self {
        self.get_authentication = Some(value);
        self
    }
    pub fn with_ask(mut self, value: PhpMixed) -> Self {
        self.ask = Some(value);
        self
    }
    pub fn with_ask_confirmation(mut self, value: bool) -> Self {
        self.ask_confirmation = Some(value);
        self
    }
    pub fn with_ask_and_hide_answer(mut self, value: Option<String>) -> Self {
        self.ask_and_hide_answer = Some(value);
        self
    }
    pub fn with_ask_and_hide_answer_responses(
        mut self,
        value: indexmap::IndexMap<String, String>,
    ) -> Self {
        self.ask_and_hide_answer_responses = Some(value);
        self
    }

    // Pre-seeds the in-memory auth store, equivalent to a willReturnCallback that
    // reads a captured `$initial_config`. Reads delegate to `BaseIO` as long as the
    // `with_has_authentication`/`with_get_authentication` overrides are left unset.
    pub fn with_authentication(
        mut self,
        repository_name: impl Into<String>,
        username: impl Into<String>,
        password: Option<String>,
    ) -> Self {
        let mut auth = indexmap::IndexMap::new();
        auth.insert("username".to_string(), Some(username.into()));
        auth.insert("password".to_string(), password);
        self.authentications.insert(repository_name.into(), auth);
        self
    }
}

impl IOInterfaceImmutable for IOStub {
    fn is_interactive(&self) -> bool {
        self.is_interactive.unwrap_or(false)
    }
    fn is_verbose(&self) -> bool {
        self.is_verbose.unwrap_or(false)
    }
    fn is_very_verbose(&self) -> bool {
        self.is_very_verbose.unwrap_or(false)
    }
    fn is_debug(&self) -> bool {
        self.is_debug.unwrap_or(false)
    }
    fn is_decorated(&self) -> bool {
        self.is_decorated.unwrap_or(false)
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
        self.ask.clone().unwrap_or(default)
    }
    fn ask_confirmation(&self, _question: String, default: bool) -> bool {
        self.ask_confirmation.unwrap_or(default)
    }
    fn ask_and_validate(
        &self,
        _question: String,
        _validator: Box<dyn Fn(PhpMixed) -> anyhow::Result<PhpMixed>>,
        _attempts: Option<i64>,
        default: PhpMixed,
    ) -> anyhow::Result<PhpMixed> {
        Ok(default)
    }
    fn ask_and_hide_answer(&self, question: String) -> Option<String> {
        if let Some(responses) = &self.ask_and_hide_answer_responses {
            return Some(responses.get(&question).cloned().unwrap_or_default());
        }
        self.ask_and_hide_answer.clone().unwrap_or(None)
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
        match self.has_authentication {
            Some(value) => value,
            None => <Self as BaseIO>::has_authentication(self, repository_name),
        }
    }
    fn get_authentication(
        &self,
        repository_name: &str,
    ) -> indexmap::IndexMap<String, Option<String>> {
        match &self.get_authentication {
            Some(value) => value.clone(),
            None => <Self as BaseIO>::get_authentication(self, repository_name),
        }
    }

    fn error(&self, message: &str, context: &[(&str, &str)]) {
        <Self as BaseIO>::error(self, message, context);
    }
    fn warning(&self, message: &str, context: &[(&str, &str)]) {
        <Self as BaseIO>::warning(self, message, context);
    }
    fn debug(&self, message: &str, context: &[(&str, &str)]) {
        <Self as BaseIO>::debug(self, message, context);
    }
}

impl IOInterfaceMutable for IOStub {
    fn set_authentication(
        &mut self,
        repository_name: String,
        username: String,
        password: Option<String>,
    ) {
        <Self as BaseIO>::set_authentication(self, repository_name, username, password)
    }
    fn load_configuration(&mut self, config: &mut Config) -> anyhow::Result<()> {
        <Self as BaseIO>::load_configuration(self, config)
    }
}

impl IOInterface for IOStub {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_base_io_mut(&mut self) -> Option<&mut dyn BaseIO> {
        Some(self)
    }
}

impl BaseIO for IOStub {
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
