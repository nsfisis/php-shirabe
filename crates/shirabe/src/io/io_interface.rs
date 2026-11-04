//! ref: composer/src/Composer/IO/IOInterface.php

use crate::config::Config;
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_symfony_console::input::InputValue;

pub const QUIET: i64 = 1;
pub const NORMAL: i64 = 2;
pub const VERBOSE: i64 = 4;
pub const VERY_VERBOSE: i64 = 8;
pub const DEBUG: i64 = 16;

// In PHP this is `IOInterface extends LoggerInterface`. Shirabe does not
// integrate with the PHP runtime, so there is no need for a separate
// `LoggerInterface` entity on the Rust side: the LoggerInterface methods that
// Composer actually invokes through an IO are folded directly into this trait
// (each is annotated as originating from LoggerInterface).
pub trait IOInterfaceImmutable: std::fmt::Debug {
    fn is_interactive(&self) -> bool;

    fn is_verbose(&self) -> bool;

    fn is_very_verbose(&self) -> bool;

    fn is_debug(&self) -> bool;

    fn is_decorated(&self) -> bool;

    fn write_bytes(&self, message: &BStr, newline: bool, verbosity: i64);

    fn write_error_bytes(&self, message: &BStr, newline: bool, verbosity: i64);

    fn write_raw_bytes(&self, message: &BStr, newline: bool, verbosity: i64);

    fn write_error_raw_bytes(&self, message: &BStr, newline: bool, verbosity: i64);

    fn overwrite_bytes(&self, message: &BStr, newline: bool, size: Option<i64>, verbosity: i64);

    fn overwrite_error_bytes(
        &self,
        message: &BStr,
        newline: bool,
        size: Option<i64>,
        verbosity: i64,
    );

    /// PHP declares a default as `string|bool|int|float|null` and an answer as `mixed`; what
    /// Composer passes and gets back is a string, a bool, a list of strings or null, which is the
    /// domain of `InputValue`. The same goes for `ask_and_validate()` and `select()`.
    ///
    /// PHP: `@throws \RuntimeException If there is no data to read in the input stream`.
    /// `QuestionHelper::ask` exceptions (validator errors, `MissingInputException`) propagate
    /// to the caller, hence the `anyhow::Result` return type.
    fn ask_bytes(&self, question: &BStr, default: InputValue) -> anyhow::Result<InputValue>;

    fn ask_confirmation_bytes(&self, question: &BStr, default: bool) -> anyhow::Result<bool>;

    fn ask_and_validate_bytes(
        &self,
        question: &BStr,
        validator: Box<dyn Fn(InputValue) -> anyhow::Result<InputValue>>,
        attempts: Option<i64>,
        default: InputValue,
    ) -> anyhow::Result<InputValue>;

    fn ask_and_hide_answer_bytes(&self, question: &BStr) -> anyhow::Result<Option<BString>>;

    /// PHP `array $choices` may be a list or an associative array; list choices resolve to their
    /// index, while associative choices resolve to their key. A list is passed with its indexes
    /// as keys. `attempts` is `None` for PHP's `false` (infinite attempts).
    ///
    /// PHP: `@throws \InvalidArgumentException` — the `ChoiceQuestion` validator error
    /// propagates to the caller once `$attempts` is exhausted, hence the `anyhow::Result`
    /// return type.
    fn select_bytes(
        &self,
        question: &BStr,
        choices: IndexMap<BString, BString>,
        default: InputValue,
        attempts: Option<i64>,
        error_message: &BStr,
        multiselect: bool,
    ) -> anyhow::Result<InputValue>;

    fn get_authentications(&self) -> IndexMap<BString, IndexMap<BString, Option<BString>>>;

    fn has_authentication(&self, repository_name: &BStr) -> bool;

    fn get_authentication(&self, repository_name: &BStr) -> IndexMap<BString, Option<BString>>;

    // From PHP `LoggerInterface` (which `IOInterface` extends). Only the
    // variants Composer actually calls through an IO are kept.
    fn error_bytes(&self, message: &BStr, context: &[(&BStr, &BStr)]);

    fn warning_bytes(&self, message: &BStr, context: &[(&BStr, &BStr)]);

    fn debug_bytes(&self, message: &BStr, context: &[(&BStr, &BStr)]);
}

// The overloads of the PHP methods taking a string (`write()`, `writeError()`, `ask()`,
// `select()`, `error()`, ...), which accept any byte string. They are generic, so they live outside
// the `dyn IOInterface` vtable.
pub trait IOInterfaceWrite: IOInterfaceImmutable {
    fn write(&self, message: impl AsRef<[u8]>) {
        self.write_bytes(BStr::new(message.as_ref()), true, NORMAL)
    }
    fn write2(&self, message: impl AsRef<[u8]>, newline: bool) {
        self.write_bytes(BStr::new(message.as_ref()), newline, NORMAL)
    }
    fn write3(&self, message: impl AsRef<[u8]>, newline: bool, verbosity: i64) {
        self.write_bytes(BStr::new(message.as_ref()), newline, verbosity)
    }
    fn write_no_newline(&self, message: impl AsRef<[u8]>) {
        self.write_bytes(BStr::new(message.as_ref()), false, NORMAL)
    }
    fn write_error(&self, message: impl AsRef<[u8]>) {
        self.write_error_bytes(BStr::new(message.as_ref()), true, NORMAL)
    }
    fn write_error2(&self, message: impl AsRef<[u8]>, newline: bool) {
        self.write_error_bytes(BStr::new(message.as_ref()), newline, NORMAL)
    }
    fn write_error3(&self, message: impl AsRef<[u8]>, newline: bool, verbosity: i64) {
        self.write_error_bytes(BStr::new(message.as_ref()), newline, verbosity)
    }
    fn write_error_no_newline(&self, message: impl AsRef<[u8]>) {
        self.write_error_bytes(BStr::new(message.as_ref()), false, NORMAL)
    }
    fn write_raw(&self, message: impl AsRef<[u8]>) {
        self.write_raw_bytes(BStr::new(message.as_ref()), true, NORMAL)
    }
    fn write_raw2(&self, message: impl AsRef<[u8]>, newline: bool) {
        self.write_raw_bytes(BStr::new(message.as_ref()), newline, NORMAL)
    }
    fn write_raw3(&self, message: impl AsRef<[u8]>, newline: bool, verbosity: i64) {
        self.write_raw_bytes(BStr::new(message.as_ref()), newline, verbosity)
    }
    fn write_error_raw(&self, message: impl AsRef<[u8]>) {
        self.write_error_raw_bytes(BStr::new(message.as_ref()), true, NORMAL)
    }
    fn write_error_raw2(&self, message: impl AsRef<[u8]>, newline: bool) {
        self.write_error_raw_bytes(BStr::new(message.as_ref()), newline, NORMAL)
    }
    fn write_error_raw3(&self, message: impl AsRef<[u8]>, newline: bool, verbosity: i64) {
        self.write_error_raw_bytes(BStr::new(message.as_ref()), newline, verbosity)
    }
    fn overwrite(&self, message: impl AsRef<[u8]>) {
        self.overwrite_bytes(BStr::new(message.as_ref()), true, None, NORMAL)
    }
    fn overwrite2(&self, message: impl AsRef<[u8]>, newline: bool) {
        self.overwrite_bytes(BStr::new(message.as_ref()), newline, None, NORMAL)
    }
    fn overwrite3(&self, message: impl AsRef<[u8]>, newline: bool, size: Option<i64>) {
        self.overwrite_bytes(BStr::new(message.as_ref()), newline, size, NORMAL)
    }
    fn overwrite4(
        &self,
        message: impl AsRef<[u8]>,
        newline: bool,
        size: Option<i64>,
        verbosity: i64,
    ) {
        self.overwrite_bytes(BStr::new(message.as_ref()), newline, size, verbosity)
    }
    fn overwrite_error(&self, message: impl AsRef<[u8]>) {
        self.overwrite_error_bytes(BStr::new(message.as_ref()), true, None, NORMAL)
    }
    fn overwrite_error2(&self, message: impl AsRef<[u8]>, newline: bool) {
        self.overwrite_error_bytes(BStr::new(message.as_ref()), newline, None, NORMAL)
    }
    fn overwrite_error3(&self, message: impl AsRef<[u8]>, newline: bool, size: Option<i64>) {
        self.overwrite_error_bytes(BStr::new(message.as_ref()), newline, size, NORMAL)
    }
    fn overwrite_error4(
        &self,
        message: impl AsRef<[u8]>,
        newline: bool,
        size: Option<i64>,
        verbosity: i64,
    ) {
        self.overwrite_error_bytes(BStr::new(message.as_ref()), newline, size, verbosity)
    }
    fn ask(&self, question: impl AsRef<[u8]>, default: InputValue) -> anyhow::Result<InputValue> {
        self.ask_bytes(BStr::new(question.as_ref()), default)
    }
    fn ask_confirmation(&self, question: impl AsRef<[u8]>, default: bool) -> anyhow::Result<bool> {
        self.ask_confirmation_bytes(BStr::new(question.as_ref()), default)
    }
    fn ask_and_validate(
        &self,
        question: impl AsRef<[u8]>,
        validator: Box<dyn Fn(InputValue) -> anyhow::Result<InputValue>>,
        attempts: Option<i64>,
        default: InputValue,
    ) -> anyhow::Result<InputValue> {
        self.ask_and_validate_bytes(BStr::new(question.as_ref()), validator, attempts, default)
    }
    fn ask_and_hide_answer(&self, question: impl AsRef<[u8]>) -> anyhow::Result<Option<BString>> {
        self.ask_and_hide_answer_bytes(BStr::new(question.as_ref()))
    }
    fn select(
        &self,
        question: impl AsRef<[u8]>,
        choices: IndexMap<BString, BString>,
        default: InputValue,
        attempts: Option<i64>,
        error_message: impl AsRef<[u8]>,
        multiselect: bool,
    ) -> anyhow::Result<InputValue> {
        self.select_bytes(
            BStr::new(question.as_ref()),
            choices,
            default,
            attempts,
            BStr::new(error_message.as_ref()),
            multiselect,
        )
    }
    fn error(&self, message: impl AsRef<[u8]>, context: &[(&BStr, &BStr)]) {
        self.error_bytes(BStr::new(message.as_ref()), context)
    }
    fn warning(&self, message: impl AsRef<[u8]>, context: &[(&BStr, &BStr)]) {
        self.warning_bytes(BStr::new(message.as_ref()), context)
    }
    fn debug(&self, message: impl AsRef<[u8]>, context: &[(&BStr, &BStr)]) {
        self.debug_bytes(BStr::new(message.as_ref()), context)
    }
}

impl<T: IOInterfaceImmutable + ?Sized> IOInterfaceWrite for T {}

// The `&mut self` part of PHP `IOInterface`.
pub trait IOInterfaceMutable {
    fn set_authentication(
        &mut self,
        repository_name: BString,
        username: BString,
        password: Option<BString>,
    );

    fn load_configuration(&mut self, config: &mut Config) -> anyhow::Result<()>;
}

// PHP `IOInterface`. This is the type used for the shared trait object
// `dyn IOInterface`; its vtable carries both the immutable and mutable methods.
pub trait IOInterface: IOInterfaceImmutable + IOInterfaceMutable {
    fn as_any(&self) -> &dyn std::any::Any;

    fn as_base_io_mut(&mut self) -> Option<&mut dyn crate::io::BaseIO> {
        None
    }

    // PHP: only `ConsoleIO::enableDebugging` exists; the interface itself does not declare it.
    fn enable_debugging(&mut self, _start_time: f64) {
        panic!("enable_debugging is only supported by ConsoleIO/BufferIO");
    }
}

impl IOInterfaceImmutable for std::rc::Rc<std::cell::RefCell<dyn IOInterface>> {
    fn is_interactive(&self) -> bool {
        self.borrow().is_interactive()
    }

    fn is_verbose(&self) -> bool {
        self.borrow().is_verbose()
    }

    fn is_very_verbose(&self) -> bool {
        self.borrow().is_very_verbose()
    }

    fn is_debug(&self) -> bool {
        self.borrow().is_debug()
    }

    fn is_decorated(&self) -> bool {
        self.borrow().is_decorated()
    }

    fn write_bytes(&self, message: &BStr, newline: bool, verbosity: i64) {
        self.borrow().write_bytes(message, newline, verbosity)
    }

    fn write_error_bytes(&self, message: &BStr, newline: bool, verbosity: i64) {
        self.borrow().write_error_bytes(message, newline, verbosity)
    }

    fn write_raw_bytes(&self, message: &BStr, newline: bool, verbosity: i64) {
        self.borrow().write_raw_bytes(message, newline, verbosity)
    }

    fn write_error_raw_bytes(&self, message: &BStr, newline: bool, verbosity: i64) {
        self.borrow()
            .write_error_raw_bytes(message, newline, verbosity)
    }

    fn overwrite_bytes(&self, message: &BStr, newline: bool, size: Option<i64>, verbosity: i64) {
        self.borrow()
            .overwrite_bytes(message, newline, size, verbosity)
    }

    fn overwrite_error_bytes(
        &self,
        message: &BStr,
        newline: bool,
        size: Option<i64>,
        verbosity: i64,
    ) {
        self.borrow()
            .overwrite_error_bytes(message, newline, size, verbosity)
    }

    fn ask_bytes(&self, question: &BStr, default: InputValue) -> anyhow::Result<InputValue> {
        self.borrow().ask_bytes(question, default)
    }

    fn ask_confirmation_bytes(&self, question: &BStr, default: bool) -> anyhow::Result<bool> {
        self.borrow().ask_confirmation_bytes(question, default)
    }

    fn ask_and_validate_bytes(
        &self,
        question: &BStr,
        validator: Box<dyn Fn(InputValue) -> anyhow::Result<InputValue>>,
        attempts: Option<i64>,
        default: InputValue,
    ) -> anyhow::Result<InputValue> {
        self.borrow()
            .ask_and_validate_bytes(question, validator, attempts, default)
    }

    fn ask_and_hide_answer_bytes(&self, question: &BStr) -> anyhow::Result<Option<BString>> {
        self.borrow().ask_and_hide_answer_bytes(question)
    }

    fn select_bytes(
        &self,
        question: &BStr,
        choices: IndexMap<BString, BString>,
        default: InputValue,
        attempts: Option<i64>,
        error_message: &BStr,
        multiselect: bool,
    ) -> anyhow::Result<InputValue> {
        self.borrow().select_bytes(
            question,
            choices,
            default,
            attempts,
            error_message,
            multiselect,
        )
    }

    fn get_authentications(&self) -> IndexMap<BString, IndexMap<BString, Option<BString>>> {
        self.borrow().get_authentications()
    }

    fn has_authentication(&self, repository_name: &BStr) -> bool {
        self.borrow().has_authentication(repository_name)
    }

    fn get_authentication(&self, repository_name: &BStr) -> IndexMap<BString, Option<BString>> {
        self.borrow().get_authentication(repository_name)
    }

    fn error_bytes(&self, message: &BStr, context: &[(&BStr, &BStr)]) {
        self.borrow().error_bytes(message, context)
    }

    fn warning_bytes(&self, message: &BStr, context: &[(&BStr, &BStr)]) {
        self.borrow().warning_bytes(message, context)
    }

    fn debug_bytes(&self, message: &BStr, context: &[(&BStr, &BStr)]) {
        self.borrow().debug_bytes(message, context)
    }
}
