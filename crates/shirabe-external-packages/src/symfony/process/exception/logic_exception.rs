//! ref: composer/vendor/symfony/process/Exception/LogicException.php

#[derive(Debug)]
pub struct LogicException {
    pub message: String,
    pub code: i64,
}

impl LogicException {
    pub fn new(message: String) -> Self {
        Self { message, code: 0 }
    }
}

impl std::fmt::Display for LogicException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for LogicException {}
