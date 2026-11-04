//! ref: composer/vendor/symfony/console/CommandLoader/CommandLoaderInterface.php sha1:0705f6a2fd1673715459719c801a657069fdb0e3

use crate::command::Command;
use bstr::{BStr, BString};

pub trait CommandLoaderInterface: std::fmt::Debug {
    /// Loads a command.
    fn get(&self, name: &BStr) -> std::rc::Rc<std::cell::RefCell<dyn Command>>;

    /// Checks if a command exists.
    fn has(&self, name: &BStr) -> bool;

    fn get_names(&self) -> Vec<BString>;
}
