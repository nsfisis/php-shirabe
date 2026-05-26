//! ref: composer/src/Composer/IO/IOInterface.php

use crate::config::Config;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;
use std::cell::RefCell;
use std::rc::Rc;

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
//
// On the Rust side the interface is split into an immutable part (`&self`
// methods, below) and a mutable part (`IOInterfaceMutable`, the `&mut self`
// methods). The shared handle `Rc<RefCell<dyn IOInterface>>` implements only
// `IOInterfaceImmutable`, so the mutating methods are reachable only via
// `io.borrow_mut()` — enforced at compile time rather than at runtime.
pub trait IOInterfaceImmutable: std::fmt::Debug {
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

    // From PHP `LoggerInterface` (which `IOInterface` extends). Only the
    // variants Composer actually calls through an IO are kept.
    fn error(&self, message: &str, context: &[(&str, &str)]);

    fn warning(&self, message: &str, context: &[(&str, &str)]);

    fn debug(&self, message: &str, context: &[(&str, &str)]);
}

// The `&mut self` part of PHP `IOInterface`. The shared handle does NOT
// implement this trait, so these methods can only be reached through
// `io.borrow_mut()`.
pub trait IOInterfaceMutable {
    fn set_authentication(
        &mut self,
        repository_name: String,
        username: String,
        password: Option<String>,
    );

    fn load_configuration(&mut self, config: &mut Config) -> anyhow::Result<()>;
}

// PHP `IOInterface`. This is the type used for the shared trait object
// `dyn IOInterface`; its vtable carries both the immutable and mutable methods.
pub trait IOInterface: IOInterfaceImmutable + IOInterfaceMutable {}

// Shared-ownership handle for a PHP IO instance (reference semantics). It
// exposes only the immutable surface; mutating methods (`set_authentication`,
// `load_configuration`) are reached via `io.borrow_mut()`. Because the handle
// does not implement `IOInterfaceMutable`, calling those directly on the handle
// is a compile error rather than a runtime panic.
impl IOInterfaceImmutable for Rc<RefCell<dyn IOInterface>> {
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

    fn write(&self, message: &str) {
        self.borrow().write(message)
    }

    fn write2(&self, message: &str, newline: bool) {
        self.borrow().write2(message, newline)
    }

    fn write_no_newline(&self, message: &str) {
        self.borrow().write_no_newline(message)
    }

    fn write3(&self, message: &str, newline: bool, verbosity: i64) {
        self.borrow().write3(message, newline, verbosity)
    }

    fn write_error(&self, message: &str) {
        self.borrow().write_error(message)
    }

    fn write_error2(&self, message: &str, newline: bool) {
        self.borrow().write_error2(message, newline)
    }

    fn write_error_no_newline(&self, message: &str) {
        self.borrow().write_error_no_newline(message)
    }

    fn write_error3(&self, message: &str, newline: bool, verbosity: i64) {
        self.borrow().write_error3(message, newline, verbosity)
    }

    fn write_raw(&self, message: &str) {
        self.borrow().write_raw(message)
    }

    fn write_raw2(&self, message: &str, newline: bool) {
        self.borrow().write_raw2(message, newline)
    }

    fn write_raw3(&self, message: &str, newline: bool, verbosity: i64) {
        self.borrow().write_raw3(message, newline, verbosity)
    }

    fn write_error_raw(&self, message: &str) {
        self.borrow().write_error_raw(message)
    }

    fn write_error_raw2(&self, message: &str, newline: bool) {
        self.borrow().write_error_raw2(message, newline)
    }

    fn write_error_raw3(&self, message: &str, newline: bool, verbosity: i64) {
        self.borrow().write_error_raw3(message, newline, verbosity)
    }

    fn overwrite(&self, message: &str) {
        self.borrow().overwrite(message)
    }

    fn overwrite2(&self, message: &str, newline: bool) {
        self.borrow().overwrite2(message, newline)
    }

    fn overwrite3(&self, message: &str, newline: bool, size: Option<i64>) {
        self.borrow().overwrite3(message, newline, size)
    }

    fn overwrite4(&self, message: &str, newline: bool, size: Option<i64>, verbosity: i64) {
        self.borrow().overwrite4(message, newline, size, verbosity)
    }

    fn overwrite_error(&self, message: &str) {
        self.borrow().overwrite_error(message)
    }

    fn overwrite_error2(&self, message: &str, newline: bool) {
        self.borrow().overwrite_error2(message, newline)
    }

    fn overwrite_error3(&self, message: &str, newline: bool, size: Option<i64>) {
        self.borrow().overwrite_error3(message, newline, size)
    }

    fn overwrite_error4(&self, message: &str, newline: bool, size: Option<i64>, verbosity: i64) {
        self.borrow()
            .overwrite_error4(message, newline, size, verbosity)
    }

    fn ask(&self, question: String, default: PhpMixed) -> PhpMixed {
        self.borrow().ask(question, default)
    }

    fn ask_confirmation(&self, question: String, default: bool) -> bool {
        self.borrow().ask_confirmation(question, default)
    }

    fn ask_and_validate(
        &self,
        question: String,
        validator: Box<dyn Fn(PhpMixed) -> PhpMixed>,
        attempts: Option<i64>,
        default: PhpMixed,
    ) -> PhpMixed {
        self.borrow()
            .ask_and_validate(question, validator, attempts, default)
    }

    fn ask_and_hide_answer(&self, question: String) -> Option<String> {
        self.borrow().ask_and_hide_answer(question)
    }

    fn select(
        &self,
        question: String,
        choices: Vec<String>,
        default: PhpMixed,
        attempts: PhpMixed,
        error_message: String,
        multiselect: bool,
    ) -> PhpMixed {
        self.borrow().select(
            question,
            choices,
            default,
            attempts,
            error_message,
            multiselect,
        )
    }

    fn get_authentications(&self) -> IndexMap<String, IndexMap<String, Option<String>>> {
        self.borrow().get_authentications()
    }

    fn has_authentication(&self, repository_name: &str) -> bool {
        self.borrow().has_authentication(repository_name)
    }

    fn get_authentication(&self, repository_name: &str) -> IndexMap<String, Option<String>> {
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
