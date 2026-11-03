//! ref: composer/vendor/symfony/console/Completion/Suggestion.php

use bstr::BString;
/// Represents a single suggested value.
#[derive(Debug)]
pub struct Suggestion {
    value: BString,
}

impl Suggestion {
    pub fn new(value: impl Into<BString>) -> Self {
        Self {
            value: value.into(),
        }
    }

    pub fn get_value(&self) -> BString {
        self.value.clone()
    }
}
