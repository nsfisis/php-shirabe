use crate::PhpMixed;
use indexmap::IndexMap;

pub fn empty(value: &PhpMixed) -> bool {
    match value {
        PhpMixed::Null => true,
        PhpMixed::Bool(b) => !*b,
        PhpMixed::Int(i) => *i == 0,
        PhpMixed::Float(f) => *f == 0.0,
        PhpMixed::String(s) => s.is_empty() || s == "0",
        PhpMixed::List(v) => v.is_empty(),
        PhpMixed::Array(m) => m.is_empty(),
        PhpMixed::Object(_) => false,
    }
}

pub fn serialize(_value: &PhpMixed) -> String {
    todo!()
}

pub fn is_bool(_value: &PhpMixed) -> bool {
    matches!(_value, PhpMixed::Bool(_))
}

pub fn is_string(_value: &PhpMixed) -> bool {
    matches!(_value, PhpMixed::String(_))
}

pub fn is_int(_value: &PhpMixed) -> bool {
    matches!(_value, PhpMixed::Int(_))
}

pub fn is_scalar(_value: &PhpMixed) -> bool {
    matches!(
        _value,
        PhpMixed::Bool(_) | PhpMixed::Int(_) | PhpMixed::Float(_) | PhpMixed::String(_)
    )
}

pub fn is_numeric(value: &PhpMixed) -> bool {
    match value {
        PhpMixed::Int(_) | PhpMixed::Float(_) => true,
        PhpMixed::String(s) => is_numeric_string(s),
        _ => false,
    }
}

pub fn is_callable(_value: &PhpMixed) -> bool {
    todo!()
}

pub fn is_object(_value: &PhpMixed) -> bool {
    matches!(_value, PhpMixed::Object(_))
}

pub fn is_a(_object_or_class: &PhpMixed, _class: &str, _allow_string: bool) -> bool {
    todo!()
}

pub fn is_resource(_value: &PhpMixed) -> bool {
    // PhpMixed has no resource variant, so a PhpMixed is never a resource.
    false
}

pub fn is_array(_value: &PhpMixed) -> bool {
    matches!(_value, PhpMixed::List(_) | PhpMixed::Array(_))
}

pub fn is_null(_value: &PhpMixed) -> bool {
    matches!(_value, PhpMixed::Null)
}

pub fn is_iterable(_value: &PhpMixed) -> bool {
    todo!()
}

pub fn is_numeric_string(s: &str) -> bool {
    // PHP is_numeric() on a string: optional leading whitespace, an integer or float literal
    // (decimal/scientific). PHP does not treat "inf"/"nan" as numeric.
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("inf") || lower.contains("nan") {
        return false;
    }
    trimmed.parse::<i64>().is_ok() || trimmed.parse::<f64>().is_ok()
}

pub fn is_numeric_to_int(value: &PhpMixed) -> i64 {
    // PHP: is_numeric($value) ? (int) $value : 0.
    match value {
        PhpMixed::Int(n) => *n,
        PhpMixed::Float(f) => *f as i64,
        PhpMixed::String(s) if is_numeric_string(s) => {
            let trimmed = s.trim();
            trimmed
                .parse::<i64>()
                .unwrap_or_else(|_| trimmed.parse::<f64>().map(|f| f as i64).unwrap_or(0))
        }
        _ => 0,
    }
}

pub fn instance_of<T>(_value: &PhpMixed) -> bool {
    todo!()
}

pub fn is_subclass_of(_object_or_class: &PhpMixed, _class_name: &str, _allow_string: bool) -> bool {
    todo!()
}

pub fn get_class(_object: &PhpMixed) -> String {
    todo!()
}

// Overload accepting an `anyhow::Error` (PHP's `get_class($e)` is commonly used on exceptions).
pub fn get_class_err(_e: &anyhow::Error) -> String {
    todo!()
}

/// Overload accepting any object reference. PHP's `get_class($obj)` returns the
/// class name; in Rust we don't have a runtime class name, so this stub is left
/// as `todo!()`.
pub fn get_class_obj<T: ?Sized>(_object: &T) -> String {
    todo!()
}

pub fn get_debug_type(_value: &PhpMixed) -> String {
    todo!()
}

pub fn get_debug_type_obj<T>(_value: &T) -> String {
    // PHP get_debug_type() returns the class name for an object. Rust has no runtime class names;
    // the static type name is the closest faithful diagnostic available here.
    std::any::type_name::<T>().to_string()
}

pub fn instantiate_class(_class: &str, _args: Vec<PhpMixed>) -> PhpMixed {
    todo!()
}

pub fn php_to_string(value: &PhpMixed) -> String {
    match value {
        PhpMixed::Null => String::new(),
        PhpMixed::Bool(true) => "1".to_string(),
        PhpMixed::Bool(false) => String::new(),
        PhpMixed::Int(i) => i.to_string(),
        PhpMixed::Float(f) => f.to_string(),
        PhpMixed::String(s) => s.clone(),
        // PHP renders any array as the literal string "Array".
        PhpMixed::List(_) | PhpMixed::Array(_) => "Array".to_string(),
        PhpMixed::Object(_) => todo!(),
    }
}

pub fn strval(value: &PhpMixed) -> String {
    php_to_string(value)
}

pub fn intval(_value: &PhpMixed) -> i64 {
    // Single-argument PHP intval(), i.e. base 10.
    match _value {
        PhpMixed::Null => 0,
        PhpMixed::Bool(b) => *b as i64,
        PhpMixed::Int(i) => *i,
        PhpMixed::Float(f) => {
            if f.is_finite() {
                *f as i64
            } else {
                0
            }
        }
        PhpMixed::String(s) => {
            // Skip leading whitespace, read an optional sign and the leading run of digits,
            // stopping at the first non-digit; no leading digits yields 0. Overflow saturates.
            let bytes = s.as_bytes();
            let mut i = 0;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            let mut negative = false;
            if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
                negative = bytes[i] == b'-';
                i += 1;
            }
            let start = i;
            let mut acc: i64 = 0;
            let mut overflow = false;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                let digit = (bytes[i] - b'0') as i64;
                acc = acc
                    .checked_mul(10)
                    .and_then(|v| v.checked_add(digit))
                    .unwrap_or_else(|| {
                        overflow = true;
                        0
                    });
                i += 1;
            }
            if i == start {
                return 0;
            }
            if overflow {
                return if negative { i64::MIN } else { i64::MAX };
            }
            if negative { -acc } else { acc }
        }
        PhpMixed::List(items) => (!items.is_empty()) as i64,
        PhpMixed::Array(array) => (!array.is_empty()) as i64,
        PhpMixed::Object(_) => 1,
    }
}

pub fn to_array(_value: PhpMixed) -> IndexMap<String, PhpMixed> {
    todo!()
}

pub fn to_string(value: &PhpMixed) -> String {
    php_to_string(value)
}

pub fn to_bool(value: &PhpMixed) -> bool {
    php_truthy(value)
}

pub fn php_truthy(value: &PhpMixed) -> bool {
    match value {
        PhpMixed::Null => false,
        PhpMixed::Bool(b) => *b,
        PhpMixed::Int(i) => *i != 0,
        PhpMixed::Float(f) => *f != 0.0,
        // PHP treats only "" and "0" as falsy strings.
        PhpMixed::String(s) => !s.is_empty() && s != "0",
        PhpMixed::List(items) => !items.is_empty(),
        PhpMixed::Array(entries) => !entries.is_empty(),
        // Objects are always truthy.
        PhpMixed::Object(_) => true,
    }
}

pub fn boolval(value: &PhpMixed) -> bool {
    php_truthy(value)
}

pub fn var_export(_value: &PhpMixed, _return: bool) -> String {
    todo!()
}

pub fn var_export_str(_value: &str, _return: bool) -> String {
    todo!()
}
