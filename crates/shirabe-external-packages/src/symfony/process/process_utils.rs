//! ref: composer/vendor/symfony/process/ProcessUtils.php

use shirabe_php_shim::{self as php, PhpMixed};

use crate::symfony::process::exception::invalid_argument_exception::InvalidArgumentException;

/// ProcessUtils is a bunch of utility methods.
#[derive(Debug)]
pub struct ProcessUtils;

impl ProcessUtils {
    /// Validates and normalizes a Process input.
    pub fn validate_input(caller: &str, input: PhpMixed) -> anyhow::Result<PhpMixed> {
        if !input.is_null() {
            if php::is_string(&input) {
                return Ok(input);
            }
            if php::is_scalar(&input) {
                let s = match &input {
                    PhpMixed::Bool(b) => {
                        if *b {
                            "1".to_string()
                        } else {
                            String::new()
                        }
                    }
                    PhpMixed::Int(i) => i.to_string(),
                    PhpMixed::Float(f) => f.to_string(),
                    other => other.as_string().map(|s| s.to_string()).unwrap_or_default(),
                };
                return Ok(PhpMixed::String(s));
            }
            // TODO(plugin): Process / \Iterator / \Traversable inputs are not modeled by PhpMixed.

            return Err(InvalidArgumentException::new(format!(
                "\"{}\" only accepts strings, Traversable objects or stream resources.",
                caller
            ))
            .into());
        }

        Ok(input)
    }
}
