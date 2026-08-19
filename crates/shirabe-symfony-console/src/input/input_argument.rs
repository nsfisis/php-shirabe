//! ref: composer/vendor/symfony/console/Input/InputArgument.php

use crate::exception::InvalidArgumentException;
use crate::exception::LogicException;
use crate::input::InputValue;

#[derive(Debug, Clone)]
pub struct InputArgument {
    name: String,
    mode: i64,
    default: InputValue,
    description: String,
}

impl InputArgument {
    pub const REQUIRED: i64 = 1;
    pub const OPTIONAL: i64 = 2;
    pub const IS_ARRAY: i64 = 4;

    pub fn new(
        name: String,
        mode: Option<i64>,
        description: String,
        default: InputValue,
    ) -> anyhow::Result<Self> {
        let mode = match mode {
            None => Self::OPTIONAL,
            Some(m) if !(1..=7).contains(&m) => {
                return Err(InvalidArgumentException::new(format!(
                    "Argument mode \"{}\" is not valid.",
                    m
                ))
                .into());
            }
            Some(m) => m,
        };

        let mut argument = InputArgument {
            name,
            mode,
            description,
            default: InputValue::Null,
        };

        argument.set_default(default)?;

        Ok(argument)
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn is_required(&self) -> bool {
        Self::REQUIRED == (Self::REQUIRED & self.mode)
    }

    pub fn is_array(&self) -> bool {
        Self::IS_ARRAY == (Self::IS_ARRAY & self.mode)
    }

    pub fn set_default(&mut self, default: InputValue) -> anyhow::Result<()> {
        if self.is_required() && !default.is_null() {
            return Err(LogicException::new(
                "Cannot set a default value except for InputArgument::OPTIONAL mode.".to_string(),
            )
            .into());
        }

        let default = if self.is_array() {
            match default {
                InputValue::Null => InputValue::Array(vec![]),
                InputValue::Array(_) => default,
                _ => {
                    return Err(LogicException::new(
                        "A default value for an array argument must be an array.".to_string(),
                    )
                    .into());
                }
            }
        } else {
            default
        };

        self.default = default;
        Ok(())
    }

    pub fn get_default(&self) -> &InputValue {
        &self.default
    }

    pub fn get_description(&self) -> &str {
        &self.description
    }
}
