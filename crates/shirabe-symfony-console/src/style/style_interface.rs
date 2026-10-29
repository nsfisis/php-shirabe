//! ref: composer/vendor/symfony/console/Style/StyleInterface.php

use crate::helper::{Cell, Row};
use bstr::BString;

/// Output style helpers.
pub trait StyleInterface {
    /// Formats an error result bar.
    fn error(&mut self, message: &[BString]);

    /// Formats a table.
    fn table(&mut self, headers: Vec<Cell>, rows: Vec<Row>);

    /// Asks for confirmation.
    fn confirm(&mut self, question: &str, default: bool) -> bool;

    /// Add newline(s).
    fn new_line(&mut self, count: i64);
}
