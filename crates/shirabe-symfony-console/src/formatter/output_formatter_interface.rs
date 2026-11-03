//! ref: composer/vendor/symfony/console/Formatter/OutputFormatterInterface.php

use crate::formatter::output_formatter_style_interface::OutputFormatterStyleInterface;
use bstr::{BStr, BString};
use shirabe_php_shim::AsAny;

/// Formatter interface for console output.
pub trait OutputFormatterInterface: AsAny {
    /// Sets the decorated flag.
    fn set_decorated(&mut self, decorated: bool);

    /// Whether the output will decorate messages.
    fn is_decorated(&self) -> bool;

    /// Sets a new style.
    fn set_style(&mut self, name: &BStr, style: Box<dyn OutputFormatterStyleInterface>);

    /// Checks if output formatter has style with specified name.
    fn has_style(&self, name: &BStr) -> bool;

    /// Gets style options from style with specified name.
    ///
    /// Throws InvalidArgumentException when style isn't defined.
    fn get_style(
        &self,
        name: &BStr,
    ) -> anyhow::Result<std::rc::Rc<std::cell::RefCell<Box<dyn OutputFormatterStyleInterface>>>>;

    /// Formats a message according to the given styles.
    fn format(&mut self, message: Option<&BStr>) -> anyhow::Result<Option<BString>>;
}
