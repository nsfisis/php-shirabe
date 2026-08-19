//! ref: composer/vendor/symfony/console/Input/InputInterface.php

/// The value an option or an argument can hold.
///
/// Symfony declares the domain as `string|bool|int|float|array|null` and only ever narrows it in
/// PHPDoc; the arrays it stores are lists of strings.
#[derive(Debug, Clone, PartialEq)]
pub enum InputValue {
    Null,
    Bool(bool),
    String(String),
    Array(Vec<String>),
}

impl InputValue {
    /// Narrows a PHP value to this domain.
    ///
    /// TODO(type-model): the question helpers and the plugin bridge still hand back a
    /// `PhpMixed`, so a value outside this domain can only be rejected here.
    pub fn from_php_mixed(value: &shirabe_php_shim::PhpMixed) -> Self {
        use shirabe_php_shim::PhpMixed;
        match value {
            PhpMixed::Null => Self::Null,
            PhpMixed::Bool(b) => Self::Bool(*b),
            PhpMixed::String(s) => Self::String(s.clone()),
            PhpMixed::List(_) | PhpMixed::Array(_) => Self::Array(
                value
                    .values()
                    .into_iter()
                    .map(|item| match item {
                        PhpMixed::String(s) => s.clone(),
                        other => panic!("an input array holds {:?}, not a string", other),
                    })
                    .collect(),
            ),
            other => panic!(
                "an input value holds {:?}, not a bool, string, array or null",
                other
            ),
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    pub fn is_array(&self) -> bool {
        matches!(self, Self::Array(_))
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[String]> {
        match self {
            Self::Array(items) => Some(items),
            _ => None,
        }
    }

    /// PHP's `(bool) $value`.
    pub fn to_bool(&self) -> bool {
        match self {
            Self::Null => false,
            Self::Bool(b) => *b,
            Self::String(s) => !s.is_empty() && s != "0",
            Self::Array(items) => !items.is_empty(),
        }
    }

    /// The value as a PHP `mixed`, for the shim functions that take one.
    pub fn to_php_mixed(&self) -> shirabe_php_shim::PhpMixed {
        self.clone().into()
    }

    /// PHP's `(string) $value`, which is a fatal error for an array.
    pub fn to_php_string(&self) -> String {
        match self {
            Self::Null => String::new(),
            Self::Bool(b) => {
                if *b {
                    "1".to_string()
                } else {
                    String::new()
                }
            }
            Self::String(s) => s.clone(),
            Self::Array(_) => panic!("array to string conversion"),
        }
    }
}

impl From<InputValue> for shirabe_php_shim::PhpMixed {
    fn from(value: InputValue) -> Self {
        match value {
            InputValue::Null => Self::Null,
            InputValue::Bool(b) => Self::Bool(b),
            InputValue::String(s) => Self::String(s),
            InputValue::Array(items) => Self::List(items.into_iter().map(Self::String).collect()),
        }
    }
}

impl From<&str> for InputValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_string())
    }
}

impl From<String> for InputValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<bool> for InputValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<Option<String>> for InputValue {
    fn from(value: Option<String>) -> Self {
        match value {
            Some(value) => Self::String(value),
            None => Self::Null,
        }
    }
}

impl From<Vec<String>> for InputValue {
    fn from(value: Vec<String>) -> Self {
        Self::Array(value)
    }
}
