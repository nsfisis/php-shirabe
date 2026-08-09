//! ref: composer/vendor/symfony/console/CommandLoader/CommandLoaderInterface.php

use crate::command::Command;

pub trait CommandLoaderInterface: std::fmt::Debug {
    /// Loads a command.
    ///
    /// @throws CommandNotFoundException
    fn get(&self, name: &str) -> std::rc::Rc<std::cell::RefCell<dyn Command>>;

    /// Checks if a command exists.
    fn has(&self, name: &str) -> bool;

    fn get_names(&self) -> Vec<String>;
}
