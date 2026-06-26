//! ref: composer/vendor/seld/jsonlint/src/Seld/JsonLint/ParsingException.php

#[derive(Debug, Clone, Default)]
pub struct ParsingExceptionLoc {
    pub first_line: i64,
    pub first_column: i64,
    pub last_line: i64,
    pub last_column: i64,
}

#[derive(Debug, Clone)]
pub enum ParsingExceptionToken {
    Name(String),
    Symbol(i64),
}

#[derive(Debug, Clone, Default)]
pub struct ParsingExceptionDetails {
    pub text: Option<String>,
    pub token: Option<ParsingExceptionToken>,
    pub line: Option<i64>,
    pub loc: Option<ParsingExceptionLoc>,
    pub expected: Option<Vec<String>>,
}

#[derive(Debug)]
pub struct ParsingException {
    pub message: String,
    pub code: i64,
    pub(crate) details: Box<ParsingExceptionDetails>,
}

impl ParsingException {
    pub fn new(message: String, details: ParsingExceptionDetails) -> Self {
        Self {
            message,
            code: 0,
            details: Box::new(details),
        }
    }

    pub fn get_message(&self) -> &str {
        &self.message
    }

    pub fn get_details(&self) -> &ParsingExceptionDetails {
        &self.details
    }
}

impl std::fmt::Display for ParsingException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ParsingException {}
