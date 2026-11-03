//! ref: composer/vendor/symfony/console/CommandLoader/CommandLoaderInterface.php

use crate::command::Command;
use bstr::{BStr, BString};

pub trait CommandLoaderInterface: std::fmt::Debug {
    /// Loads a command.
    fn get(&self, name: &BStr) -> std::rc::Rc<std::cell::RefCell<dyn Command>>;

    /// Checks if a command exists.
    fn has(&self, name: &BStr) -> bool;

    fn get_names(&self) -> Vec<BString>;
}
