//! ref: composer/src/Composer/IO/IOInterface.php

use crate::config::Config;
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

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

    /// PHP: `@throws \RuntimeException If there is no data to read in the input stream`.
    /// `QuestionHelper::ask` exceptions (validator errors, `MissingInputException`) propagate
    /// to the caller, hence the `anyhow::Result` return type.
    fn ask(&self, question: String, default: PhpMixed) -> anyhow::Result<PhpMixed>;

    fn ask_confirmation(&self, question: String, default: bool) -> anyhow::Result<bool>;

    fn ask_and_validate(
        &self,
        question: String,
        validator: Box<dyn Fn(PhpMixed) -> anyhow::Result<PhpMixed>>,
        attempts: Option<i64>,
        default: PhpMixed,
    ) -> anyhow::Result<PhpMixed>;

    fn ask_and_hide_answer(&self, question: String) -> anyhow::Result<Option<String>>;

    /// PHP `array $choices` may be a list (`PhpMixed::List`) or an associative
    /// array (`PhpMixed::Array`); list choices resolve to their index, while
    /// associative choices resolve to their key.
    ///
    /// PHP: `@throws \InvalidArgumentException` — the `ChoiceQuestion` validator error
    /// propagates to the caller once `$attempts` is exhausted, hence the `anyhow::Result`
    /// return type.
    fn select(
        &self,
        question: String,
        choices: PhpMixed,
        default: PhpMixed,
        attempts: PhpMixed,
        error_message: String,
        multiselect: bool,
    ) -> anyhow::Result<PhpMixed>;

    fn get_authentications(&self) -> IndexMap<BString, IndexMap<String, Option<BString>>>;

    fn has_authentication(&self, repository_name: &BStr) -> bool;

    fn get_authentication(&self, repository_name: &BStr) -> IndexMap<String, Option<BString>>;

    // From PHP `LoggerInterface` (which `IOInterface` extends). Only the
    // variants Composer actually calls through an IO are kept.
    fn error(&self, message: &str, context: &[(&str, &str)]);

    fn warning(&self, message: &str, context: &[(&str, &str)]);

    fn debug(&self, message: &str, context: &[(&str, &str)]);
}

// The overloads of PHP's `write()`, `writeError()`, `writeRaw()`, `writeErrorRaw()`, `overwrite()`
// and `overwriteError()`, which take any byte string. They are generic, so they live outside the
// `dyn IOInterface` vtable.
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

    fn ask(&self, question: String, default: PhpMixed) -> anyhow::Result<PhpMixed> {
        self.borrow().ask(question, default)
    }

    fn ask_confirmation(&self, question: String, default: bool) -> anyhow::Result<bool> {
        self.borrow().ask_confirmation(question, default)
    }

    fn ask_and_validate(
        &self,
        question: String,
        validator: Box<dyn Fn(PhpMixed) -> anyhow::Result<PhpMixed>>,
        attempts: Option<i64>,
        default: PhpMixed,
    ) -> anyhow::Result<PhpMixed> {
        self.borrow()
            .ask_and_validate(question, validator, attempts, default)
    }

    fn ask_and_hide_answer(&self, question: String) -> anyhow::Result<Option<String>> {
        self.borrow().ask_and_hide_answer(question)
    }

    fn select(
        &self,
        question: String,
        choices: PhpMixed,
        default: PhpMixed,
        attempts: PhpMixed,
        error_message: String,
        multiselect: bool,
    ) -> anyhow::Result<PhpMixed> {
        self.borrow().select(
            question,
            choices,
            default,
            attempts,
            error_message,
            multiselect,
        )
    }

    fn get_authentications(&self) -> IndexMap<BString, IndexMap<String, Option<BString>>> {
        self.borrow().get_authentications()
    }

    fn has_authentication(&self, repository_name: &BStr) -> bool {
        self.borrow().has_authentication(repository_name)
    }

    fn get_authentication(&self, repository_name: &BStr) -> IndexMap<String, Option<BString>> {
        self.borrow().get_authentication(repository_name)
    }

    fn error(&self, message: &str, context: &[(&str, &str)]) {
        self.borrow().error(message, context)
    }

    fn warning(&self, message: &str, context: &[(&str, &str)]) {
        self.borrow().warning(message, context)
    }

    fn debug(&self, message: &str, context: &[(&str, &str)]) {
        self.borrow().debug(message, context)
    }
}
