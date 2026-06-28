//! Partial port of seld/jsonlint.
//!
//! The parser, lexer, and duplicate-key exception (`JsonParser`, `Lexer`,
//! `DuplicateKeyException`) are intentionally not ported: JSON syntax validation now relies on
//! serde_json, and duplicate-key detection is done with a hand-written serde visitor, so
//! jsonlint's own parsing machinery is no longer needed.
//!
//! `ParsingException` is kept because it is the exception type thrown for invalid JSON and is
//! matched (via downcast) across the codebase as an error-kind signal; it also carries detail
//! such as the error line. Porting it keeps both the thrown exception class and the information
//! it carries unchanged.

pub mod parsing_exception;

pub use parsing_exception::*;
