//! ref: composer/vendor/symfony/console/Input/InputOption.php sha1:660d571d521d374ccff9934cd84e8c96dcf30ec5

use crate::exception::{InvalidArgumentException, LogicException};
use crate::input::InputValue;
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_shim::{bconcat, implode, ltrim2, php_regex, preg_split};

#[derive(Debug, Clone)]
pub struct InputOption {
    name: BString,
    shortcut: Option<BString>,
    mode: i64,
    default: InputValue,
    description: BString,
}

impl InputOption {
    pub const VALUE_NONE: i64 = 1;
    pub const VALUE_REQUIRED: i64 = 2;
    pub const VALUE_OPTIONAL: i64 = 4;
    pub const VALUE_IS_ARRAY: i64 = 8;
    pub const VALUE_NEGATABLE: i64 = 16;

    pub fn new(
        name: impl AsRef<[u8]>,
        shortcut: Option<&BStr>,
        mode: Option<i64>,
        description: impl AsRef<[u8]>,
        default: InputValue,
    ) -> anyhow::Result<Self> {
        let name = name.as_ref();
        let name = BString::from(name.strip_prefix(b"--").unwrap_or(name));

        if name.is_empty() {
            return Err(InvalidArgumentException::new("An option name cannot be empty.").into());
        }

        let shortcut = match shortcut {
            None => None,
            Some(shortcut) if shortcut.is_empty() => None,
            Some(shortcut) => Self::normalize_shortcut(shortcut)?,
        };

        let mode = match mode {
            None => Self::VALUE_NONE,
            Some(m) if !(1..(Self::VALUE_NEGATABLE << 1)).contains(&m) => {
                return Err(InvalidArgumentException::new(bconcat!(
                    "Option mode \"",
                    m,
                    "\" is not valid."
                ))
                .into());
            }
            Some(m) => m,
        };

        let mut option = InputOption {
            name,
            shortcut,
            mode,
            description: BString::from(description.as_ref()),
            default: InputValue::Null,
        };

        if option.is_array() && !option.accept_value() {
            return Err(InvalidArgumentException::new("Impossible to have an option mode VALUE_IS_ARRAY if the option does not accept a value.")
            .into());
        }
        if option.is_negatable() && option.accept_value() {
            return Err(InvalidArgumentException::new("Impossible to have an option mode VALUE_NEGATABLE if the option also accepts a value.")
            .into());
        }

        option.set_default(default)?;

        Ok(option)
    }

    fn normalize_shortcut(s: &BStr) -> anyhow::Result<Option<BString>> {
        let shortcuts: Vec<BString> = preg_split(php_regex!(r"{(\|)-?}"), ltrim2(s, "-"))
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect();
        let result = implode("|", &shortcuts);
        if result.is_empty() {
            return Err(
                InvalidArgumentException::new("An option shortcut cannot be empty.").into(),
            );
        }
        Ok(Some(result))
    }

    pub fn get_shortcut(&self) -> Option<&BStr> {
        self.shortcut.as_ref().map(|shortcut| shortcut.as_bstr())
    }

    pub fn get_name(&self) -> &BStr {
        self.name.as_bstr()
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

    pub fn set_default(&mut self, default: InputValue) -> anyhow::Result<()> {
        if Self::VALUE_NONE == (Self::VALUE_NONE & self.mode) && !default.is_null() {
            return Err(LogicException::new(
                "Cannot set a default value when using InputOption::VALUE_NONE mode.",
            )
            .into());
        }

        let default = if self.is_array() {
            match default {
                InputValue::Null => InputValue::Array(vec![]),
                InputValue::Array(_) => default,
                _ => {
                    return Err(LogicException::new(
                        "A default value for an array option must be an array.",
                    )
                    .into());
                }
            }
        } else {
            default
        };

        self.default = if self.accept_value() || self.is_negatable() {
            default
        } else {
            InputValue::Bool(false)
        };
        Ok(())
    }

    pub fn get_default(&self) -> &InputValue {
        &self.default
    }

    pub fn get_description(&self) -> &BStr {
        self.description.as_bstr()
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
