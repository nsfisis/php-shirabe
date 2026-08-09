//! ref: composer/vendor/symfony/console/Output/ConsoleOutputInterface.php

use crate::output::ConsoleSectionOutput;
use crate::output::OutputInterface;

/// ConsoleOutputInterface is the interface implemented by ConsoleOutput class.
/// This adds information about stderr and section output stream.
pub trait ConsoleOutputInterface: OutputInterface {
    /// Gets the OutputInterface for errors.
    fn get_error_output(&self) -> std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>;

    fn set_error_output(&self, error: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>);

    fn section(&self) -> std::rc::Rc<std::cell::RefCell<ConsoleSectionOutput>>;
}
