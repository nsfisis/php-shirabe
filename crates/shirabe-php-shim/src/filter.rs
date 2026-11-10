use bstr::ByteSlice as _;
use shirabe_php_src::filter::logical_filters::{
    php_filter_boolean, php_filter_int, php_filter_validate_ip, php_filter_validate_url,
};

/// `filter_var($value, FILTER_VALIDATE_BOOLEAN)`: without `FILTER_NULL_ON_FAILURE`, a value that
/// fails validation yields `false`.
pub fn filter_var_boolean(value: impl AsRef<[u8]>) -> bool {
    php_filter_boolean(value.as_ref()).unwrap_or(false)
}

/// `filter_var($value, FILTER_VALIDATE_URL) !== false`.
pub fn filter_var_url(value: impl AsRef<[u8]>) -> bool {
    php_filter_validate_url(value.as_ref(), 0)
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

/// `filter_var($value, FILTER_VALIDATE_IP) !== false`.
pub fn filter_var_ip(value: impl AsRef<[u8]>) -> bool {
    php_filter_validate_ip(value.as_ref(), 0)
}

/// `filter_var($value, FILTER_VALIDATE_INT, ['options' => ['min_range' => $min, 'max_range' => $max]])
/// !== false`.
pub fn filter_var_int_with_range(value: impl AsRef<[u8]>, min: i64, max: i64) -> bool {
    php_filter_int(value.as_ref(), 0, Some(min), Some(max)).is_some()
}
