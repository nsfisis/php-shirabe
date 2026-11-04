//! Configurable IOInterface stub, equivalent to PHPUnit's
//! `getMockBuilder(IOInterface::class)->getMock()` where individual methods are
//! configured via `->method('x')->willReturn(y)`.
//!
//! Each configurable method is backed by an `Option<ReturnValue>` field plus a
//! builder-style setter. Unset methods fall back to NullIO-equivalent defaults.
#![allow(dead_code)]

use bstr::BString;
use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::{BaseIO, IOInterface, IOInterfaceImmutable, IOInterfaceMutable};
use shirabe_symfony_console::input::InputValue;

// Records call arguments for a stubbed method, equivalent to PHPUnit's
// `->expects($this->once())->method(...)->with(...)` call-count/argument verification.
#[derive(Debug)]
struct CallRecorder<T>(std::cell::RefCell<Vec<T>>);

impl<T> Default for CallRecorder<T> {
    fn default() -> Self {
        Self(std::cell::RefCell::new(Vec::new()))
    }
}

impl<T: Clone> CallRecorder<T> {
    fn push(&self, value: T) {
        self.0.borrow_mut().push(value);
    }

    fn calls(&self) -> Vec<T> {
        self.0.borrow().clone()
    }
}

#[derive(Debug, Default)]
pub struct IOStub {
    authentications:
        indexmap::IndexMap<bstr::BString, indexmap::IndexMap<bstr::BString, Option<bstr::BString>>>,

    is_interactive: Option<bool>,
    is_decorated: Option<bool>,
    is_verbose: Option<bool>,
    is_very_verbose: Option<bool>,
    is_debug: Option<bool>,

    // `hasAuthentication` is keyed: PHPUnit tests usually configure a single bool
    // willReturn that applies to any argument, so a single Option<bool> matches.
    has_authentication: Option<bool>,
    // `getAuthentication` willReturn is a `['username' => ..., 'password' => ...]` array.
    get_authentication: Option<indexmap::IndexMap<bstr::BString, Option<bstr::BString>>>,

    ask: Option<InputValue>,
    // When set, `askAndValidate` invokes the caller's validator with this value and returns/
    // propagates its `Result`, equivalent to a PHPUnit `willReturnCallback` that calls the
    // validator directly. When unset, `askAndValidate` just returns `default` (unvalidated).
    ask_and_validate_answer: Option<InputValue>,
    ask_confirmation: Option<bool>,
    ask_and_hide_answer: Option<Option<bstr::BString>>,
    // Keyed `askAndHideAnswer` replies, mirroring tests whose willReturnCallback
    // switches on the question string. Unknown questions return `''` like PHP's
    // `switch` default.
    ask_and_hide_answer_responses: Option<indexmap::IndexMap<bstr::BString, bstr::BString>>,

    // Records `writeError` calls.
    write_error_calls: CallRecorder<(bstr::BString, bool)>,
    // Records `writeRaw` calls.
    write_raw_calls: CallRecorder<(bstr::BString, bool)>,
    // Records `setAuthentication` calls. Kept separate from `authentications` so
    // `with_has_authentication`/`with_get_authentication` stay static (matching a PHPUnit
    // `willReturn`) even while `setAuthentication` is invoked.
    set_authentication_calls: CallRecorder<(bstr::BString, bstr::BString, Option<bstr::BString>)>,
    // Records `askAndValidate` calls (question, attempts, default).
    ask_and_validate_calls: CallRecorder<(bstr::BString, Option<i64>, InputValue)>,
    // Records `hasAuthentication` calls.
    has_authentication_calls: CallRecorder<bstr::BString>,
    // Records `getAuthentication` calls.
    get_authentication_calls: CallRecorder<bstr::BString>,
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
        value: indexmap::IndexMap<bstr::BString, Option<bstr::BString>>,
    ) -> Self {
        self.get_authentication = Some(value);
        self
    }
    // Mutator counterpart of `with_get_authentication`, for tests that reconfigure the stub's
    // response between calls (equivalent to PHPUnit's `willReturnCallback` sequencing via
    // `array_shift`).
    pub fn set_get_authentication(
        &mut self,
        value: indexmap::IndexMap<bstr::BString, Option<bstr::BString>>,
    ) {
        self.get_authentication = Some(value);
    }
    pub fn with_ask(mut self, value: InputValue) -> Self {
        self.ask = Some(value);
        self
    }
    pub fn with_ask_and_validate_answer(mut self, value: InputValue) -> Self {
        self.ask_and_validate_answer = Some(value);
        self
    }
    pub fn with_ask_confirmation(mut self, value: bool) -> Self {
        self.ask_confirmation = Some(value);
        self
    }
    pub fn with_ask_and_hide_answer(mut self, value: Option<bstr::BString>) -> Self {
        self.ask_and_hide_answer = Some(value);
        self
    }
    pub fn with_ask_and_hide_answer_responses(
        mut self,
        value: indexmap::IndexMap<bstr::BString, bstr::BString>,
    ) -> Self {
        self.ask_and_hide_answer_responses = Some(value.into_iter().collect());
        self
    }

    // Pre-seeds the in-memory auth store, equivalent to a willReturnCallback that
    // reads a captured `$initial_config`. Reads delegate to `BaseIO` as long as the
    // `with_has_authentication`/`with_get_authentication` overrides are left unset.
    pub fn with_authentication(
        mut self,
        repository_name: impl Into<bstr::BString>,
        username: impl Into<bstr::BString>,
        password: Option<bstr::BString>,
    ) -> Self {
        let mut auth = indexmap::IndexMap::new();
        auth.insert(bstr::BString::from("username"), Some(username.into()));
        auth.insert(bstr::BString::from("password"), password);
        self.authentications.insert(repository_name.into(), auth);
        self
    }

    // For testing only. Returns the recorded `writeError` calls in call order.
    pub fn write_error_calls(&self) -> Vec<(bstr::BString, bool)> {
        self.write_error_calls.calls()
    }

    // For testing only. Returns the recorded `writeRaw` calls in call order.
    pub fn write_raw_calls(&self) -> Vec<(bstr::BString, bool)> {
        self.write_raw_calls.calls()
    }

    // For testing only. Returns the recorded `setAuthentication` calls in call order.
    pub fn set_authentication_calls(
        &self,
    ) -> Vec<(bstr::BString, bstr::BString, Option<bstr::BString>)> {
        self.set_authentication_calls.calls()
    }

    // For testing only. Returns the recorded `askAndValidate` calls in call order.
    pub fn ask_and_validate_calls(&self) -> Vec<(bstr::BString, Option<i64>, InputValue)> {
        self.ask_and_validate_calls.calls()
    }

    // For testing only. Returns the recorded `hasAuthentication` calls in call order.
    pub fn has_authentication_calls(&self) -> Vec<bstr::BString> {
        self.has_authentication_calls.calls()
    }

    // For testing only. Returns the recorded `getAuthentication` calls in call order.
    pub fn get_authentication_calls(&self) -> Vec<bstr::BString> {
        self.get_authentication_calls.calls()
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

    fn write_bytes(&self, _message: &bstr::BStr, _newline: bool, _verbosity: i64) {}
    fn write_error_bytes(&self, message: &bstr::BStr, newline: bool, _verbosity: i64) {
        self.write_error_calls.push((message.to_owned(), newline));
    }
    fn write_raw_bytes(&self, message: &bstr::BStr, newline: bool, _verbosity: i64) {
        self.write_raw_calls.push((message.to_owned(), newline));
    }
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
        Ok(self.ask.clone().unwrap_or(default))
    }
    fn ask_confirmation_bytes(
        &self,
        _question: &bstr::BStr,
        default: bool,
    ) -> anyhow::Result<bool> {
        Ok(self.ask_confirmation.unwrap_or(default))
    }
    fn ask_and_validate_bytes(
        &self,
        question: &bstr::BStr,
        validator: Box<dyn Fn(InputValue) -> anyhow::Result<InputValue>>,
        attempts: Option<i64>,
        default: InputValue,
    ) -> anyhow::Result<InputValue> {
        self.ask_and_validate_calls
            .push((question.to_owned(), attempts, default.clone()));
        match &self.ask_and_validate_answer {
            Some(answer) => validator(answer.clone()),
            None => Ok(default),
        }
    }
    fn ask_and_hide_answer_bytes(
        &self,
        question: &bstr::BStr,
    ) -> anyhow::Result<Option<bstr::BString>> {
        if let Some(responses) = &self.ask_and_hide_answer_responses {
            return Ok(Some(responses.get(question).cloned().unwrap_or_default()));
        }
        Ok(self.ask_and_hide_answer.clone().unwrap_or(None))
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
        self.has_authentication_calls
            .push(repository_name.to_owned());
        match self.has_authentication {
            Some(value) => value,
            None => <Self as BaseIO>::has_authentication(self, repository_name),
        }
    }
    fn get_authentication(
        &self,
        repository_name: &bstr::BStr,
    ) -> indexmap::IndexMap<bstr::BString, Option<bstr::BString>> {
        self.get_authentication_calls
            .push(repository_name.to_owned());
        match &self.get_authentication {
            Some(value) => value.clone(),
            None => <Self as BaseIO>::get_authentication(self, repository_name),
        }
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

impl IOInterfaceMutable for IOStub {
    fn set_authentication(
        &mut self,
        repository_name: bstr::BString,
        username: bstr::BString,
        password: Option<bstr::BString>,
    ) {
        self.set_authentication_calls.push((
            repository_name.clone(),
            username.clone(),
            password.clone(),
        ));
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
