//! ref: composer/vendor/symfony/console/Input/InputArgument.php sha1:21b2694f2593d73854fe762a33e1a74ec088bbfc

use crate::exception::{InvalidArgumentException, LogicException};
use crate::input::InputValue;
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_shim::bformat;

#[derive(Debug, Clone)]
pub struct InputArgument {
    name: BString,
    mode: i64,
    default: InputValue,
    description: BString,
}

impl InputArgument {
    pub const REQUIRED: i64 = 1;
    pub const OPTIONAL: i64 = 2;
    pub const IS_ARRAY: i64 = 4;

    pub fn new(
        name: impl Into<BString>,
        mode: Option<i64>,
        description: impl AsRef<[u8]>,
        default: InputValue,
    ) -> anyhow::Result<Self> {
        let mode = match mode {
            None => Self::OPTIONAL,
            Some(m) if !(1..=7).contains(&m) => {
                return Err(InvalidArgumentException::new(bformat!(
                    "Argument mode \"{m}\" is not valid."
                ))
                .into());
            }
            Some(m) => m,
        };

        let mut argument = InputArgument {
            name: name.into(),
            mode,
            description: BString::from(description.as_ref()),
            default: InputValue::Null,
        };

        argument.set_default(default)?;

        Ok(argument)
    }

    pub fn get_name(&self) -> &BStr {
        self.name.as_bstr()
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
                "Cannot set a default value except for InputArgument::OPTIONAL mode.",
            )
            .into());
        }

        let default = if self.is_array() {
            match default {
                InputValue::Null => InputValue::Array(vec![]),
                InputValue::Array(_) => default,
                _ => {
                    return Err(LogicException::new(
                        "A default value for an array argument must be an array.",
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

    pub fn get_description(&self) -> &BStr {
        self.description.as_bstr()
    }
}
