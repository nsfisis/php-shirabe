//! ref: composer/vendor/symfony/console/Formatter/WrappableOutputFormatterInterface.php

use crate::formatter::output_formatter_interface::OutputFormatterInterface;
use bstr::{BStr, BString};

/// Formatter interface for console output that supports word wrapping.
pub trait WrappableOutputFormatterInterface: OutputFormatterInterface {
    /// Formats a message according to the given styles, wrapping at `width` (0 means no wrapping).
    fn format_and_wrap(
        &mut self,
        message: Option<&BStr>,
        width: i64,
    ) -> anyhow::Result<Option<BString>>;
}
