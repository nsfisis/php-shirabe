//! ref: composer/vendor/symfony/console/Formatter/WrappableOutputFormatterInterface.php sha1:403bb7f7c8f0907e1a6f68e1ce1b248227fb3759

use crate::formatter::OutputFormatterInterface;
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
