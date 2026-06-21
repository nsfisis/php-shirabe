// Without FILTER_NULL_ON_FAILURE, php_filter_boolean trims surrounding
// whitespace, lowercases, and yields true only for "1"/"true"/"on"/"yes";
// every other input (including the "0"/"false"/"off"/"no"/"" set) yields
// false.
pub fn filter_var_boolean(value: &str) -> bool {
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
pub fn filter_var_url(value: &str) -> bool {
    reqwest::Url::parse(value).is_ok()
}

pub fn filter_var_email(_value: &str) -> bool {
    todo!()
}

pub fn filter_var_ip(_value: &str) -> bool {
    todo!()
}

pub fn filter_var_int_with_range(_value: &str, _min: i64, _max: i64) -> bool {
    todo!()
}
