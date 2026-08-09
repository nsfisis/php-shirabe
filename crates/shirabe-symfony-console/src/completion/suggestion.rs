//! ref: composer/vendor/symfony/console/Completion/Suggestion.php

/// Represents a single suggested value.
#[derive(Debug)]
pub struct Suggestion {
    value: String,
}

impl Suggestion {
    pub fn new(value: String) -> Self {
        Self { value }
    }

    pub fn get_value(&self) -> String {
        self.value.clone()
    }
}

impl std::fmt::Display for Suggestion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.get_value())
    }
}
