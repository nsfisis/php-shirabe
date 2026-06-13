//! ref: composer/vendor/symfony/console/Formatter/WrappableOutputFormatterInterface.php

use crate::symfony::console::formatter::output_formatter_interface::OutputFormatterInterface;

/// Formatter interface for console output that supports word wrapping.
pub trait WrappableOutputFormatterInterface: OutputFormatterInterface {
    /// Formats a message according to the given styles, wrapping at `width` (0 means no wrapping).
    fn format_and_wrap(
        &mut self,
        message: Option<&str>,
        width: i64,
    ) -> anyhow::Result<Option<String>>;
}
