//! ref: composer/vendor/symfony/process/ProcessUtils.php

use crate::exception::InvalidArgumentException;
use shirabe_php_shim::{PhpMixed, is_scalar, is_string, php_to_string};

/// ProcessUtils is a bunch of utility methods.
#[derive(Debug)]
pub struct ProcessUtils;

impl ProcessUtils {
    /// Validates and normalizes a Process input.
    pub fn validate_input(caller: &str, input: PhpMixed) -> anyhow::Result<PhpMixed> {
        if !input.is_null() {
            if is_string(&input) {
                return Ok(input);
            }
            if is_scalar(&input) {
                return Ok(PhpMixed::string(php_to_string(&input)));
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
