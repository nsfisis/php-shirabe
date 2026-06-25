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

pub fn serialize(value: &PhpMixed) -> String {
    let mut out = String::new();
    serialize_into(&mut out, value);
    out
}

fn serialize_into(out: &mut String, value: &PhpMixed) {
    match value {
        PhpMixed::Null => out.push_str("N;"),
        PhpMixed::Bool(b) => {
            out.push_str("b:");
            out.push(if *b { '1' } else { '0' });
            out.push(';');
        }
        PhpMixed::Int(i) => out.push_str(&format!("i:{};", i)),
        PhpMixed::Float(f) => out.push_str(&format!("d:{};", serialize_float(*f))),
        // PHP measures the string length in bytes.
        PhpMixed::String(s) => out.push_str(&format!("s:{}:\"{}\";", s.len(), s)),
        PhpMixed::List(items) => {
            out.push_str(&format!("a:{}:{{", items.len()));
            for (i, item) in items.iter().enumerate() {
                out.push_str(&format!("i:{};", i));
                serialize_into(out, item);
            }
            out.push('}');
        }
        PhpMixed::Array(entries) => {
            out.push_str(&format!("a:{}:{{", entries.len()));
            for (k, v) in entries {
                // PHP normalizes canonical integer string keys to integer keys.
                match canonical_int_key(k) {
                    Some(i) => out.push_str(&format!("i:{};", i)),
                    None => out.push_str(&format!("s:{}:\"{}\";", k.len(), k)),
                }
                serialize_into(out, v);
            }
            out.push('}');
        }
        // TODO(phase-d): object serialization needs the PHP class name and the property
        // visibility name-mangling ("O:len:\"Class\":n:{...}"), which PhpMixed::Object does not
        // carry.
        PhpMixed::Object(_) => todo!(),
    }
}

// TODO(phase-d): PHP's serialize uses serialize_precision (-1 => shortest round-trip), which Rust's
// default float formatting also produces, but the two differ on scientific-notation spelling (PHP
// "1.0E+20" vs Rust "1e20") for very large/small magnitudes.
fn serialize_float(f: f64) -> String {
    if f.is_nan() {
        "NAN".to_string()
    } else if f.is_infinite() {
        if f < 0.0 {
            "-INF".to_string()
        } else {
            "INF".to_string()
        }
    } else {
        format!("{}", f)
    }
}

/// Returns the integer a PHP array key string normalizes to, or None if the key stays a string.
/// PHP treats a key as an integer only when it is a canonical decimal integer: no leading `+`, no
/// redundant leading zeros, and within the platform integer range ("-0" is not canonical).
fn canonical_int_key(key: &str) -> Option<i64> {
    if key == "0" {
        return Some(0);
    }
    let digits = key.strip_prefix('-').unwrap_or(key);
    if digits.is_empty() || digits.starts_with('0') {
        return None;
    }
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    key.parse::<i64>().ok()
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

pub fn is_callable(value: &PhpMixed) -> bool {
    match value {
        // Scalars and null are never callable in PHP.
        PhpMixed::Null | PhpMixed::Bool(_) | PhpMixed::Int(_) | PhpMixed::Float(_) => false,
        // TODO(phase-d): PHP is_callable() checks whether a string names an existing function, or an
        // array/object resolves to a method/__invoke. PhpMixed has no callable variant and the shim
        // has no function/method registry, so callability of these cannot be determined.
        _ => todo!(),
    }
}

pub fn is_object(_value: &PhpMixed) -> bool {
    matches!(_value, PhpMixed::Object(_))
}

pub fn is_a(_object_or_class: &PhpMixed, _class: &str, _allow_string: bool) -> bool {
    // TODO(phase-d): requires runtime class information (the object's class and its ancestry), which
    // PhpMixed::Object does not carry.
    todo!()
}

pub fn is_array(_value: &PhpMixed) -> bool {
    matches!(_value, PhpMixed::List(_) | PhpMixed::Array(_))
}

pub fn is_null(_value: &PhpMixed) -> bool {
    matches!(_value, PhpMixed::Null)
}

pub fn is_iterable(value: &PhpMixed) -> bool {
    // PHP is_iterable() is true for arrays and Traversable objects.
    // TODO(phase-d): PhpMixed::Object cannot report whether it implements Traversable, so an
    // iterable object is conservatively treated as non-iterable here.
    matches!(value, PhpMixed::List(_) | PhpMixed::Array(_))
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
    // TODO(phase-d): PHP `instanceof` needs the runtime class of the value, which PhpMixed::Object
    // does not carry.
    todo!()
}

pub fn is_subclass_of(_object_or_class: &PhpMixed, _class_name: &str, _allow_string: bool) -> bool {
    // TODO(phase-d): requires runtime class ancestry, which PhpMixed::Object does not carry.
    todo!()
}

pub fn get_class(_object: &PhpMixed) -> String {
    // TODO(phase-d): PhpMixed::Object carries no class name; there is no runtime class to report.
    todo!()
}

// Overload accepting an `anyhow::Error` (PHP's `get_class($e)` is commonly used on exceptions).
pub fn get_class_err(_e: &anyhow::Error) -> String {
    // TODO(phase-d): PHP returns the exception's class name. anyhow::Error carries the concrete
    // exception type, but mapping each ported exception struct to its PHP class name is not yet
    // wired up (cf. php_exception_get_code which downcasts case by case).
    todo!()
}

/// Overload accepting any object reference. PHP's `get_class($obj)` returns the
/// class name; in Rust we don't have a runtime class name, so this stub is left
/// as `todo!()`.
pub fn get_class_obj<T: ?Sized>(_object: &T) -> String {
    // TODO(phase-d): PHP returns the object's class name; Rust has no runtime class name for an
    // arbitrary `T` (the static type path is not the PHP class name).
    todo!()
}

pub fn get_debug_type(value: &PhpMixed) -> String {
    match value {
        PhpMixed::Null => "null".to_string(),
        PhpMixed::Bool(_) => "bool".to_string(),
        PhpMixed::Int(_) => "int".to_string(),
        PhpMixed::Float(_) => "float".to_string(),
        PhpMixed::String(_) => "string".to_string(),
        PhpMixed::List(_) | PhpMixed::Array(_) => "array".to_string(),
        // TODO(phase-d): PHP returns the object's class name; PhpMixed::Object carries none.
        PhpMixed::Object(_) => todo!(),
    }
}

pub fn get_debug_type_obj<T>(_value: &T) -> String {
    // PHP get_debug_type() returns the class name for an object. Rust has no runtime class names;
    // the static type name is the closest faithful diagnostic available here.
    std::any::type_name::<T>().to_string()
}

pub fn instantiate_class(_class: &str, _args: Vec<PhpMixed>) -> PhpMixed {
    // TODO(phase-d): instantiating a class by name needs a runtime class registry (reflection),
    // which the shim does not provide.
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
        // TODO(phase-d): PHP casts an object to string via its __toString() method; PhpMixed::Object
        // carries no class/method information to dispatch to.
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

pub fn to_array(value: PhpMixed) -> IndexMap<String, PhpMixed> {
    // PHP `(array)` cast: null => empty array; scalar => [0 => value]; list keeps its integer keys;
    // array/object map directly to their entries.
    match value {
        PhpMixed::Null => IndexMap::new(),
        PhpMixed::Array(m) | PhpMixed::Object(m) => m,
        PhpMixed::List(items) => items
            .into_iter()
            .enumerate()
            .map(|(i, v)| (i.to_string(), v))
            .collect(),
        scalar => {
            let mut m = IndexMap::new();
            m.insert("0".to_string(), scalar);
            m
        }
    }
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

pub fn var_export(value: &PhpMixed, r#return: bool) -> String {
    let mut out = String::new();
    var_export_into(&mut out, value, 0);
    if r#return {
        out
    } else {
        // PHP echoes the representation and returns null when $return is false.
        print!("{}", out);
        String::new()
    }
}

pub fn var_export_str(value: &str, r#return: bool) -> String {
    let out = var_export_string(value);
    if r#return {
        out
    } else {
        print!("{}", out);
        String::new()
    }
}

fn var_export_into(out: &mut String, value: &PhpMixed, level: usize) {
    match value {
        PhpMixed::Null => out.push_str("NULL"),
        PhpMixed::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        PhpMixed::Int(i) => out.push_str(&i.to_string()),
        PhpMixed::Float(f) => out.push_str(&var_export_float(*f)),
        PhpMixed::String(s) => out.push_str(&var_export_string(s)),
        PhpMixed::List(items) => {
            out.push_str("array (\n");
            for (i, item) in items.iter().enumerate() {
                var_export_indent(out, level + 1);
                out.push_str(&format!("{} => ", i));
                if matches!(item, PhpMixed::List(_) | PhpMixed::Array(_)) {
                    out.push('\n');
                    var_export_indent(out, level + 1);
                }
                var_export_into(out, item, level + 1);
                out.push_str(",\n");
            }
            var_export_indent(out, level);
            out.push(')');
        }
        PhpMixed::Array(entries) => {
            out.push_str("array (\n");
            for (k, v) in entries {
                var_export_indent(out, level + 1);
                match canonical_int_key(k) {
                    Some(i) => out.push_str(&i.to_string()),
                    None => out.push_str(&var_export_string(k)),
                }
                out.push_str(" => ");
                if matches!(v, PhpMixed::List(_) | PhpMixed::Array(_)) {
                    out.push('\n');
                    var_export_indent(out, level + 1);
                }
                var_export_into(out, v, level + 1);
                out.push_str(",\n");
            }
            var_export_indent(out, level);
            out.push(')');
        }
        // TODO(phase-d): PHP renders objects as "\Class::__set_state(array(...))"; PhpMixed::Object
        // carries no class name.
        PhpMixed::Object(_) => todo!(),
    }
}

fn var_export_indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("  ");
    }
}

// PHP var_export() escapes only the backslash and single-quote inside the single-quoted literal.
fn var_export_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('\'');
    out
}

fn var_export_float(f: f64) -> String {
    if f.is_nan() {
        return "NAN".to_string();
    }
    if f.is_infinite() {
        return if f < 0.0 {
            "-INF".to_string()
        } else {
            "INF".to_string()
        };
    }
    // PHP always renders a float with a fractional/exponent marker, so a whole-valued float gets a
    // trailing ".0".
    let s = format!("{}", f);
    if s.contains('.') || s.contains('e') || s.contains('E') {
        s
    } else {
        format!("{}.0", s)
    }
}
