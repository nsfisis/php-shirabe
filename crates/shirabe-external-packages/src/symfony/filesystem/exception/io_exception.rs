//! ref: composer/vendor/symfony/filesystem/Exception/IOException.php

#[derive(Debug)]
pub struct IOException {
    pub message: String,
    pub code: i64,
    pub path: Option<String>,
}

impl IOException {
    pub fn new(
        message: String,
        code: i64,
        _previous: Option<Box<dyn std::error::Error + Send + Sync>>,
        path: Option<String>,
    ) -> Self {
        Self {
            message,
            code,
            path,
        }
    }
}

impl std::fmt::Display for IOException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for IOException {}
