//! ref: composer/vendor/symfony/process/ProcessUtils.php sha1:1fd6fdf6b636d32f4bc58fe71d7ab1c6a42a3fc0

use crate::exception::InvalidArgumentException;
use bstr::BStr;
use shirabe_php_shim::{PhpMixed, bconcat, is_scalar, is_string, php_to_string};

/// ProcessUtils is a bunch of utility methods.
#[derive(Debug)]
pub struct ProcessUtils;

impl ProcessUtils {
    /// Validates and normalizes a Process input.
    pub fn validate_input(caller: &BStr, input: PhpMixed) -> anyhow::Result<PhpMixed> {
        if !input.is_null() {
            if is_string(&input) {
                return Ok(input);
            }
            if is_scalar(&input) {
                return Ok(PhpMixed::string(php_to_string(&input)));
            }
            // TODO(plugin): Process / \Iterator / \Traversable inputs are not modeled by PhpMixed.

            return Err(InvalidArgumentException::new(bconcat!(
                "\"",
                caller,
                "\" only accepts strings, Traversable objects or stream resources."
            ))
            .into());
        }

        Ok(input)
    }
}
