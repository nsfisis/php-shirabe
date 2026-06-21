use crate::PhpMixed;
use indexmap::IndexMap;

pub const FILTER_VALIDATE_EMAIL: i64 = 274;

pub const FILTER_VALIDATE_BOOLEAN: i64 = 258;
pub const FILTER_VALIDATE_URL: i64 = 273;
pub const FILTER_VALIDATE_IP: i64 = 275;
pub const FILTER_VALIDATE_INT: i64 = 257;

pub fn filter_var(value: &str, filter: i64) -> bool {
    match filter {
        // Without FILTER_NULL_ON_FAILURE, php_filter_boolean trims surrounding
        // whitespace, lowercases, and yields true only for "1"/"true"/"on"/"yes";
        // every other input (including the "0"/"false"/"off"/"no"/"" set) yields
        // false.
        FILTER_VALIDATE_BOOLEAN => {
            let trimmed = value.trim_matches([' ', '\t', '\n', '\r', '\0', '\x0B']);
            matches!(
                trimmed.to_ascii_lowercase().as_str(),
                "1" | "true" | "on" | "yes"
            )
        }
        // TODO(phase-c): PHP's FILTER_VALIDATE_URL parses with php_url_parse_ex and
        // additionally validates the host as a domain/IPv6 literal. reqwest::Url
        // (WHATWG/RFC 3986) is stricter on some inputs and more lenient on others,
        // so this is not a byte-for-byte compatible validator.
        FILTER_VALIDATE_URL => reqwest::Url::parse(value).is_ok(),
        _ => todo!(),
    }
}

pub fn filter_var_with_options(
    _value: &str,
    _filter: i64,
    _options: &IndexMap<String, PhpMixed>,
) -> PhpMixed {
    todo!()
}
