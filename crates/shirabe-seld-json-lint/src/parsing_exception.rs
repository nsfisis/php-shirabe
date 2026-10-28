//! ref: composer/vendor/seld/jsonlint/src/Seld/JsonLint/ParsingException.php

use shirabe_php_shim::Exception;

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
    inner: Exception,
    details: Box<ParsingExceptionDetails>,
}

impl ParsingException {
    #[track_caller]
    pub fn new(message: String, details: ParsingExceptionDetails) -> Self {
        Self {
            inner: Exception::new(message),
            details: Box::new(details),
        }
    }

    pub fn get_details(&self) -> &ParsingExceptionDetails {
        &self.details
    }
}

shirabe_php_shim::impl_php_exception!(ParsingException, inner, r"Seld\JsonLint\ParsingException");
