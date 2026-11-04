//! ref: composer/vendor/symfony/console/Style/StyleInterface.php sha1:5d4fad503c923a87e29b269b7f4a74147a6db114

use crate::helper::{Cell, Row};
use bstr::{BStr, BString};

/// Output style helpers.
pub trait StyleInterface {
    /// Formats an error result bar.
    fn error(&self, message: &[BString]);

    /// Formats a table.
    fn table(&self, headers: Vec<Cell>, rows: Vec<Row>) -> anyhow::Result<()>;

    /// Asks for confirmation.
    fn confirm(&self, question: &BStr, default: bool) -> bool;

    /// Add newline(s).
    fn new_line(&self, count: i64);
}
