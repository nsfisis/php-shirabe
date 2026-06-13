//! ref: composer/vendor/symfony/console/Formatter/OutputFormatterInterface.php

use crate::symfony::console::formatter::output_formatter_style_interface::OutputFormatterStyleInterface;

/// Formatter interface for console output.
pub trait OutputFormatterInterface {
    /// Sets the decorated flag.
    fn set_decorated(&mut self, decorated: bool);

    /// Whether the output will decorate messages.
    fn is_decorated(&self) -> bool;

    /// Sets a new style.
    fn set_style(&mut self, name: &str, style: Box<dyn OutputFormatterStyleInterface>);

    /// Checks if output formatter has style with specified name.
    fn has_style(&self, name: &str) -> bool;

    /// Gets style options from style with specified name.
    ///
    /// Throws InvalidArgumentException when style isn't defined.
    fn get_style(&self, name: &str) -> anyhow::Result<Box<dyn OutputFormatterStyleInterface>>;

    /// Formats a message according to the given styles.
    fn format(&mut self, message: Option<&str>) -> anyhow::Result<Option<String>>;
}
