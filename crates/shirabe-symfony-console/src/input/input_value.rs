//! ref: composer/vendor/symfony/console/Input/InputInterface.php sha1:7c0149e375832cad7933bbe51f11975bf856effb

use bstr::{BStr, BString};
use shirabe_php_shim::{PhpDisplay, PhpMixed, boolval};

/// The value an option or an argument can hold, which is also the value a question takes as its
/// default and gives as its answer.
///
/// Symfony declares the domain as `string|bool|int|float|array|null` and only ever narrows it in
/// PHPDoc; the arrays it stores are lists of strings. A question's default is declared as
/// `string|bool|int|float|null` and its answer as `mixed`, but what reaches them is a string read
/// from the input, a default of the same domain as an option's, a bool from a confirmation, or a
/// list of strings from a multiselect choice; the answers also flow back into options and
/// arguments.
#[derive(Debug, Clone, PartialEq)]
pub enum InputValue {
    Null,
    Bool(bool),
    String(BString),
    Array(Vec<BString>),
}

impl InputValue {
    /// Narrows a PHP value to this domain.
    ///
    /// TODO(type-model): the plugin bridge still hands back a `PhpMixed`, so a value outside this
    /// domain can only be rejected here.
    pub fn from_php_mixed(value: &PhpMixed) -> Self {
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

    pub fn as_string(&self) -> Option<&BStr> {
        match self {
            Self::String(s) => Some(s.as_ref()),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[BString]> {
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
            Self::String(s) => boolval(s),
            Self::Array(items) => !items.is_empty(),
        }
    }

    /// The key PHP uses when the value indexes an array: `null` is `""` and a bool is `0` or `1`.
    pub fn array_key(&self) -> BString {
        match self {
            Self::Null => BString::default(),
            Self::Bool(b) => BString::from(if *b { "1" } else { "0" }),
            Self::String(s) => s.clone(),
            Self::Array(_) => panic!("an array is not a valid array key"),
        }
    }

    /// The value as a PHP `mixed`, for the shim functions that take one.
    pub fn to_php_mixed(&self) -> PhpMixed {
        self.clone().into()
    }
}

impl PhpDisplay for InputValue {
    fn fmt(&self, out: &mut BString) {
        match self {
            Self::Null | Self::Bool(false) => {}
            Self::Bool(true) => out.push(b'1'),
            Self::String(s) => out.extend_from_slice(s),
            Self::Array(_) => panic!("array to string conversion"),
        }
    }
}

impl From<InputValue> for PhpMixed {
    fn from(value: InputValue) -> Self {
        match value {
            InputValue::Null => Self::Null,
            InputValue::Bool(b) => Self::Bool(b),
            InputValue::String(s) => Self::string(s),
            InputValue::Array(items) => Self::List(items.into_iter().map(Self::string).collect()),
        }
    }
}

#[expect(clippy::disallowed_types, reason = "a string literal")]
impl From<&str> for InputValue {
    fn from(value: &str) -> Self {
        Self::String(BString::from(value))
    }
}

impl From<&BStr> for InputValue {
    fn from(value: &BStr) -> Self {
        Self::String(BString::from(value))
    }
}

impl From<BString> for InputValue {
    fn from(value: BString) -> Self {
        Self::String(value)
    }
}

impl From<bool> for InputValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<Option<BString>> for InputValue {
    fn from(value: Option<BString>) -> Self {
        match value {
            Some(value) => Self::String(value),
            None => Self::Null,
        }
    }
}

impl From<Vec<BString>> for InputValue {
    fn from(value: Vec<BString>) -> Self {
        Self::Array(value)
    }
}
