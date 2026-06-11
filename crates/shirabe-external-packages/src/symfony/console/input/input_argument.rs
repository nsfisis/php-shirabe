use crate::symfony::console::exception::invalid_argument_exception::InvalidArgumentException;
use crate::symfony::console::exception::logic_exception::LogicException;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct InputArgument {
    name: String,
    mode: i64,
    default: PhpMixed,
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
        default: PhpMixed,
    ) -> anyhow::Result<Self> {
        let mode = match mode {
            None => Self::OPTIONAL,
            Some(m) if m > 7 || m < 1 => {
                return Err(
                    InvalidArgumentException(shirabe_php_shim::InvalidArgumentException {
                        message: format!("Argument mode \"{}\" is not valid.", m),
                        code: 0,
                    })
                    .into(),
                );
            }
            Some(m) => m,
        };

        let mut argument = InputArgument {
            name,
            mode,
            description,
            default: PhpMixed::Null,
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

    pub fn set_default(&mut self, default: PhpMixed) -> anyhow::Result<()> {
        if self.is_required() && !matches!(default, PhpMixed::Null) {
            return Err(LogicException(shirabe_php_shim::LogicException {
                message: "Cannot set a default value except for InputArgument::OPTIONAL mode."
                    .to_string(),
                code: 0,
            })
            .into());
        }

        let default = if self.is_array() {
            match default {
                PhpMixed::Null => PhpMixed::List(vec![]),
                PhpMixed::List(_) => default,
                _ => {
                    return Err(LogicException(shirabe_php_shim::LogicException {
                        message: "A default value for an array argument must be an array."
                            .to_string(),
                        code: 0,
                    })
                    .into());
                }
            }
        } else {
            default
        };

        self.default = default;
        Ok(())
    }

    pub fn get_default(&self) -> &PhpMixed {
        &self.default
    }

    pub fn get_description(&self) -> &str {
        &self.description
    }
}
