//! ref: composer/vendor/symfony/console/Input/InputOption.php

use crate::symfony::console::exception::invalid_argument_exception::InvalidArgumentException;
use crate::symfony::console::exception::logic_exception::LogicException;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct InputOption {
    name: String,
    shortcut: Option<String>,
    mode: i64,
    default: PhpMixed,
    description: String,
}

impl InputOption {
    pub const VALUE_NONE: i64 = 1;
    pub const VALUE_REQUIRED: i64 = 2;
    pub const VALUE_OPTIONAL: i64 = 4;
    pub const VALUE_IS_ARRAY: i64 = 8;
    pub const VALUE_NEGATABLE: i64 = 16;

    pub fn new(
        name: &str,
        shortcut: PhpMixed,
        mode: Option<i64>,
        description: String,
        default: PhpMixed,
    ) -> anyhow::Result<Self> {
        let name = if name.starts_with("--") {
            name[2..].to_string()
        } else {
            name.to_string()
        };

        if name.is_empty() {
            return Err(
                InvalidArgumentException(shirabe_php_shim::InvalidArgumentException {
                    message: "An option name cannot be empty.".to_string(),
                    code: 0,
                })
                .into(),
            );
        }

        let shortcut = match shortcut {
            PhpMixed::String(ref s) if s.is_empty() => None,
            PhpMixed::List(ref v) if v.is_empty() => None,
            PhpMixed::Bool(false) => None,
            PhpMixed::Null => None,
            PhpMixed::List(ref arr) => {
                let parts: Vec<String> = arr
                    .iter()
                    .filter_map(|v| {
                        if let PhpMixed::String(s) = v.as_ref() {
                            Some(s.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                let joined = shirabe_php_shim::implode("|", &parts);
                Self::normalize_shortcut(joined)?
            }
            PhpMixed::String(s) => Self::normalize_shortcut(s)?,
            _ => None,
        };

        let mode = match mode {
            None => Self::VALUE_NONE,
            Some(m) if !(1..(Self::VALUE_NEGATABLE << 1)).contains(&m) => {
                return Err(
                    InvalidArgumentException(shirabe_php_shim::InvalidArgumentException {
                        message: format!("Option mode \"{}\" is not valid.", m),
                        code: 0,
                    })
                    .into(),
                );
            }
            Some(m) => m,
        };

        let mut option = InputOption {
            name,
            shortcut,
            mode,
            description,
            default: PhpMixed::Null,
        };

        if option.is_array() && !option.accept_value() {
            return Err(InvalidArgumentException(shirabe_php_shim::InvalidArgumentException {
                message: "Impossible to have an option mode VALUE_IS_ARRAY if the option does not accept a value.".to_string(),
                code: 0,
            })
            .into());
        }
        if option.is_negatable() && option.accept_value() {
            return Err(InvalidArgumentException(shirabe_php_shim::InvalidArgumentException {
                message: "Impossible to have an option mode VALUE_NEGATABLE if the option also accepts a value.".to_string(),
                code: 0,
            })
            .into());
        }

        option.set_default(default)?;

        Ok(option)
    }

    fn normalize_shortcut(s: String) -> anyhow::Result<Option<String>> {
        let stripped = shirabe_php_shim::ltrim(&s, Some("-"));
        let parts = shirabe_php_shim::preg_split(r"\|(-?)", &stripped);
        let filtered: Vec<String> =
            shirabe_php_shim::array_filter(&parts, |s: &String| !s.is_empty());
        let result = shirabe_php_shim::implode("|", &filtered);
        if result.is_empty() {
            return Err(
                InvalidArgumentException(shirabe_php_shim::InvalidArgumentException {
                    message: "An option shortcut cannot be empty.".to_string(),
                    code: 0,
                })
                .into(),
            );
        }
        Ok(Some(result))
    }

    pub fn get_shortcut(&self) -> Option<&str> {
        self.shortcut.as_deref()
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn accept_value(&self) -> bool {
        self.is_value_required() || self.is_value_optional()
    }

    pub fn is_value_required(&self) -> bool {
        Self::VALUE_REQUIRED == (Self::VALUE_REQUIRED & self.mode)
    }

    pub fn is_value_optional(&self) -> bool {
        Self::VALUE_OPTIONAL == (Self::VALUE_OPTIONAL & self.mode)
    }

    pub fn is_array(&self) -> bool {
        Self::VALUE_IS_ARRAY == (Self::VALUE_IS_ARRAY & self.mode)
    }

    pub fn is_negatable(&self) -> bool {
        Self::VALUE_NEGATABLE == (Self::VALUE_NEGATABLE & self.mode)
    }

    pub fn set_default(&mut self, default: PhpMixed) -> anyhow::Result<()> {
        if Self::VALUE_NONE == (Self::VALUE_NONE & self.mode) && !matches!(default, PhpMixed::Null)
        {
            return Err(LogicException(shirabe_php_shim::LogicException {
                message: "Cannot set a default value when using InputOption::VALUE_NONE mode."
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
                        message: "A default value for an array option must be an array."
                            .to_string(),
                        code: 0,
                    })
                    .into());
                }
            }
        } else {
            default
        };

        self.default = if self.accept_value() || self.is_negatable() {
            default
        } else {
            PhpMixed::Bool(false)
        };
        Ok(())
    }

    pub fn get_default(&self) -> &PhpMixed {
        &self.default
    }

    pub fn get_description(&self) -> &str {
        &self.description
    }

    pub fn equals(&self, option: &InputOption) -> bool {
        option.get_name() == self.get_name()
            && option.get_shortcut() == self.get_shortcut()
            && option.get_default() == self.get_default()
            && option.is_negatable() == self.is_negatable()
            && option.is_array() == self.is_array()
            && option.is_value_required() == self.is_value_required()
            && option.is_value_optional() == self.is_value_optional()
    }
}
