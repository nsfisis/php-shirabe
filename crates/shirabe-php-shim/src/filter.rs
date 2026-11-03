use bstr::ByteSlice;

// TODO(php-semantics):
// Without FILTER_NULL_ON_FAILURE, php_filter_boolean trims surrounding
// whitespace, lowercases, and yields true only for "1"/"true"/"on"/"yes";
// every other input (including the "0"/"false"/"off"/"no"/"" set) yields
// false.
pub fn filter_var_boolean(value: impl AsRef<[u8]>) -> bool {
    let trimmed = crate::trim(value);
    matches!(
        trimmed.to_ascii_lowercase().as_slice(),
        b"1" | b"true" | b"on" | b"yes"
    )
}

// TODO(php-semantics): PHP's FILTER_VALIDATE_URL parses with php_url_parse_ex and
// additionally validates the host as a domain/IPv6 literal. reqwest::Url
// (WHATWG/RFC 3986) is stricter on some inputs and more lenient on others,
// so this is not a byte-for-byte compatible validator.
// A URL that is not valid UTF-8 holds a byte outside the ASCII range PHP accepts, so it is
// rejected without parsing.
pub fn filter_var_url(value: impl AsRef<[u8]>) -> bool {
    crate::utf8_text(value.as_ref()).is_some_and(|value| reqwest::Url::parse(value).is_ok())
}

// TODO(pcre):
// PHP's FILTER_VALIDATE_EMAIL applies a long PCRE with length lookaheads,
// quoted local parts, and bracketed IP-literal domains, which the `regex` crate
// cannot express. This is a simplified validator covering the common
// `local@domain` shape plus the RFC length limits PHP enforces; it diverges
// from PHP by rejecting quoted local parts (e.g. `"a b"@x.com`) and `[IPv6:...]`
// literal domains.
pub fn filter_var_email(value: impl AsRef<[u8]>) -> bool {
    let value = value.as_ref();
    let Some(at) = value.rfind_byte(b'@') else {
        return false;
    };
    let local = &value[..at];
    let domain = &value[at + 1..];

    if local.is_empty() || local.len() > 64 {
        return false;
    }
    if domain.is_empty() || domain.len() > 255 {
        return false;
    }

    is_valid_email_local(local) && is_valid_email_domain(domain)
}

// Reject leading/trailing/consecutive dots; otherwise allow printable ASCII
// except the specials PHP excludes from the local part.
fn is_valid_email_local(local: &[u8]) -> bool {
    if local.starts_with(b".") || local.ends_with(b".") || local.contains_str("..") {
        return false;
    }
    local
        .iter()
        .all(|&b| b > 0x20 && b < 0x7F && !matches!(b, b'@' | b'"' | b'[' | b']' | b'\\'))
}

// One or more dot-separated labels of [A-Za-z0-9-], each non-empty and not
// starting or ending with a hyphen.
fn is_valid_email_domain(domain: &[u8]) -> bool {
    domain.split(|&b| b == b'.').all(|label| {
        !label.is_empty()
            && !label.starts_with(b"-")
            && !label.ends_with(b"-")
            && label
                .iter()
                .all(|&b| b.is_ascii_alphanumeric() || b == b'-')
    })
}

// TODO(php-semantics):
// PHP's FILTER_VALIDATE_IP accepts both IPv4 and IPv6 literals. Rust's IpAddr
// parser is a close match (both reject leading zeros in IPv4 octets), but is not
// guaranteed byte-for-byte identical to PHP's hand-written validator on exotic
// IPv6 forms.
pub fn filter_var_ip(value: impl AsRef<[u8]>) -> bool {
    crate::utf8_text(value.as_ref()).is_some_and(|value| value.parse::<std::net::IpAddr>().is_ok())
}

// TODO(php-semantics):
// Mirrors PHP's FILTER_VALIDATE_INT with min_range/max_range: surrounding
// whitespace is trimmed, an optional sign is allowed, leading zeros are rejected
// (except a lone "0"), and the parsed value must fall within [min, max]
// inclusive.
pub fn filter_var_int_with_range(value: impl AsRef<[u8]>, min: i64, max: i64) -> bool {
    match parse_filter_int(value.as_ref()) {
        Some(n) => min <= n && n <= max,
        None => false,
    }
}

fn parse_filter_int(value: &[u8]) -> Option<i64> {
    let is_space = |b: &u8| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C);
    let start = value
        .iter()
        .position(|b| !is_space(b))
        .unwrap_or(value.len());
    let end = value
        .iter()
        .rposition(|b| !is_space(b))
        .map_or(start, |i| i + 1);
    let s = &value[start..end];
    let (sign, digits) = match s.strip_prefix(b"-") {
        Some(rest) => (-1i64, rest),
        None => (1i64, s.strip_prefix(b"+").unwrap_or(s)),
    };
    if digits.is_empty() {
        return None;
    }
    // A lone zero is valid; any other number must not have a leading zero.
    if digits != b"0" && digits.starts_with(b"0") {
        return None;
    }
    if !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let magnitude: i64 = crate::utf8_text(digits)?.parse().ok()?;
    Some(sign * magnitude)
}
