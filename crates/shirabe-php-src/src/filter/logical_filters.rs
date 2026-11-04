use super::filter_private::{
    FILTER_FLAG_ALLOW_HEX, FILTER_FLAG_ALLOW_OCTAL, FILTER_FLAG_GLOBAL_RANGE, FILTER_FLAG_HOSTNAME,
    FILTER_FLAG_IPV4, FILTER_FLAG_IPV6, FILTER_FLAG_NO_PRIV_RANGE, FILTER_FLAG_NO_RES_RANGE,
    FILTER_FLAG_PATH_REQUIRED, FILTER_FLAG_QUERY_REQUIRED,
};
use super::sanitizing_filters::php_filter_url;
use crate::standard::url::php_url_parse_ex2;

/// php-src: Zend/zend_long.h `MAX_LENGTH_OF_LONG` (PHP 8.5.5), for a 64-bit `zend_long`.
const MAX_LENGTH_OF_LONG: isize = 20;

#[derive(Debug, PartialEq, Eq)]
enum Format {
    Ipv4,
    Ipv6,
}

/// The byte at `i`, or NUL past the end, as the NUL-terminated `zend_string` reads.
fn at(str: &[u8], i: usize) -> u8 {
    str.get(i).copied().unwrap_or(0)
}

/// php-src: ext/filter/filter_private.h `PHP_FILTER_TRIM_DEFAULT_EX` (PHP 8.5.5), without the
/// `return_if_empty` early return, which callers check on the result.
fn php_filter_trim_default(str: &[u8]) -> &[u8] {
    let is_space = |c: &u8| matches!(c, b' ' | b'\t' | b'\r' | b'\x0b' | b'\n');
    let start = str.iter().position(|c| !is_space(c)).unwrap_or(str.len());
    let end = str
        .iter()
        .rposition(|c| !is_space(c))
        .map_or(start, |i| i + 1);
    &str[start..end]
}

/// php-src: ext/filter/logical_filters.c `php_filter_parse_int` (PHP 8.5.5)
///
/// `zend_long` is 64-bit, so the `SIZEOF_LONG == 4` overflow check never applies.
fn php_filter_parse_int(str: &[u8]) -> Option<i64> {
    let end = str.len();
    let mut s = 0;
    let mut is_negative = false;

    match at(str, s) {
        b'-' => {
            is_negative = true;
            s += 1;
        }
        b'+' => s += 1,
        _ => {}
    }

    if at(str, s) == b'0' && s + 1 == end {
        // Special cases: +0 and -0
        return Some(0);
    }

    // must start with 1..9
    let mut ctx_value: i64 = if s < end && (b'1'..=b'9').contains(&str[s]) {
        let digit = i64::from(str[s] - b'0');
        s += 1;
        if is_negative { -digit } else { digit }
    } else {
        return None;
    };

    if (end - s) as isize > MAX_LENGTH_OF_LONG - 1 {
        // number too long
        return None;
    }

    while s < end {
        if str[s].is_ascii_digit() {
            let digit = i64::from(str[s] - b'0');
            s += 1;
            if !is_negative && ctx_value <= (i64::MAX - digit) / 10 {
                ctx_value = (ctx_value * 10) + digit;
            } else if is_negative && ctx_value >= (i64::MIN + digit) / 10 {
                ctx_value = (ctx_value * 10) - digit;
            } else {
                return None;
            }
        } else {
            return None;
        }
    }

    Some(ctx_value)
}

/// php-src: ext/filter/logical_filters.c `php_filter_parse_octal` (PHP 8.5.5)
fn php_filter_parse_octal(str: &[u8]) -> Option<i64> {
    let mut ctx_value: u64 = 0;

    for &c in str {
        if (b'0'..=b'7').contains(&c) {
            let n = u64::from(c - b'0');
            if ctx_value > u64::MAX / 8 {
                return None;
            }
            ctx_value *= 8;
            if ctx_value > u64::MAX - n {
                return None;
            }
            ctx_value += n;
        } else {
            return None;
        }
    }

    Some(ctx_value as i64)
}

/// php-src: ext/filter/logical_filters.c `php_filter_parse_hex` (PHP 8.5.5)
fn php_filter_parse_hex(str: &[u8]) -> Option<i64> {
    let mut ctx_value: u64 = 0;

    for &c in str {
        let n = match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - (b'a' - 10),
            b'A'..=b'F' => c - (b'A' - 10),
            _ => return None,
        };
        let n = u64::from(n);
        if ctx_value > u64::MAX / 16 {
            return None;
        }
        ctx_value *= 16;
        if ctx_value > u64::MAX - n {
            return None;
        }
        ctx_value += n;
    }

    Some(ctx_value as i64)
}

/// php-src: ext/filter/logical_filters.c `php_filter_int` (PHP 8.5.5)
///
/// Returns the validated integer, or `None` where the original fails validation. `min_range` and
/// `max_range` are `Some` when the corresponding option is set.
pub fn php_filter_int(
    value: &[u8],
    flags: i64,
    min_range: Option<i64>,
    max_range: Option<i64>,
) -> Option<i64> {
    if value.is_empty() {
        return None;
    }

    let allow_octal = flags & FILTER_FLAG_ALLOW_OCTAL != 0;
    let allow_hex = flags & FILTER_FLAG_ALLOW_HEX != 0;

    let p = php_filter_trim_default(value);
    if p.is_empty() {
        return None;
    }

    let ctx_value = if p[0] == b'0' {
        let p = &p[1..];
        if allow_hex && matches!(at(p, 0), b'x' | b'X') {
            let p = &p[1..];
            if p.is_empty() {
                return None;
            }
            php_filter_parse_hex(p)
        } else if allow_octal {
            // Support explicit octal prefix notation
            let p = if matches!(at(p, 0), b'o' | b'O') {
                let p = &p[1..];
                if p.is_empty() {
                    return None;
                }
                p
            } else {
                p
            };
            php_filter_parse_octal(p)
        } else if !p.is_empty() {
            None
        } else {
            Some(0)
        }
    } else {
        php_filter_parse_int(p)
    }?;

    if min_range.is_some_and(|min_range| ctx_value < min_range)
        || max_range.is_some_and(|max_range| ctx_value > max_range)
    {
        return None;
    }
    Some(ctx_value)
}

/// php-src: ext/filter/logical_filters.c `php_filter_boolean` (PHP 8.5.5)
///
/// Returns the validated boolean, or `None` where the original fails validation.
pub fn php_filter_boolean(value: &[u8]) -> Option<bool> {
    let str = php_filter_trim_default(value);

    // returns true for "1", "true", "on" and "yes"
    // returns false for "0", "false", "off", "no", and ""
    // null otherwise.
    match str.len() {
        0 => Some(false),
        1 => match str[0] {
            b'1' => Some(true),
            b'0' => Some(false),
            _ => None,
        },
        2 if str.eq_ignore_ascii_case(b"on") => Some(true),
        2 if str.eq_ignore_ascii_case(b"no") => Some(false),
        3 if str.eq_ignore_ascii_case(b"yes") => Some(true),
        3 if str.eq_ignore_ascii_case(b"off") => Some(false),
        4 if str.eq_ignore_ascii_case(b"true") => Some(true),
        5 if str.eq_ignore_ascii_case(b"false") => Some(false),
        _ => None,
    }
}

/// php-src: ext/filter/logical_filters.c `php_filter_validate_domain_ex` (PHP 8.5.5)
fn php_filter_validate_domain_ex(domain: &[u8], flags: i64) -> bool {
    let hostname = flags & FILTER_FLAG_HOSTNAME != 0;
    let mut i: u8 = 1;
    let mut l = domain.len();
    let mut e = l;
    let mut s = 0;

    // Ignore trailing dot
    if l > 0 && domain[l - 1] == b'.' {
        e -= 1;
        l -= 1;
    }

    // The total length cannot exceed 253 characters (final dot not included)
    if l > 253 {
        return false;
    }

    // First char must be alphanumeric
    if at(domain, s) == b'.' || (hostname && !at(domain, s).is_ascii_alphanumeric()) {
        return false;
    }

    // `s + 1` may read the ignored trailing dot or the terminating NUL, so `domain` is indexed
    // past `e`.
    while s < e {
        if domain[s] == b'.' {
            // The first and the last character of a label must be alphanumeric
            if at(domain, s + 1) == b'.'
                || (hostname
                    && (!domain[s - 1].is_ascii_alphanumeric()
                        || !at(domain, s + 1).is_ascii_alphanumeric()))
            {
                return false;
            }

            // Reset label length counter
            i = 1;
        } else {
            if i > 63
                || (hostname
                    && (domain[s] != b'-' || at(domain, s + 1) == 0)
                    && !domain[s].is_ascii_alphanumeric())
            {
                return false;
            }

            i += 1;
        }

        s += 1;
    }

    true
}

/// php-src: ext/filter/logical_filters.c `is_userinfo_valid` (PHP 8.5.5)
fn is_userinfo_valid(str: &[u8]) -> bool {
    const VALID: &[u8] = b"-._~!$&'()*+,;=:";
    let len = str.len();
    let mut p = 0;
    while p < len {
        let c = str[p];
        // strchr() also finds the terminating NUL, so an embedded NUL is accepted.
        if c.is_ascii_alphabetic() || c.is_ascii_digit() || c == 0 || VALID.contains(&c) {
            p += 1;
        } else if c == b'%'
            && p <= len.wrapping_sub(3)
            && at(str, p + 1).is_ascii_digit()
            && at(str, p + 2).is_ascii_hexdigit()
        {
            p += 3;
        } else {
            return false;
        }
    }
    true
}

/// php-src: ext/filter/logical_filters.c `php_filter_is_valid_ipv6_hostname` (PHP 8.5.5)
fn php_filter_is_valid_ipv6_hostname(s: &[u8]) -> bool {
    s.len() >= 2
        && s[0] == b'['
        && s[s.len() - 1] == b']'
        && _php_filter_validate_ipv6(&s[1..s.len() - 1], None)
}

/// php-src: ext/filter/logical_filters.c `php_filter_validate_url` (PHP 8.5.5)
///
/// Returns whether `value` passes validation. The `uri_parser_class` option is not taken, so the
/// URI is always parsed by the default `parse_url` parser (`php_url_parse_ex2`).
pub fn php_filter_validate_url(value: &[u8], flags: i64) -> bool {
    if php_filter_url(value).len() != value.len() {
        return false;
    }

    // Parse the URI - if it fails, we return NULL
    let Some(uri) = php_url_parse_ex2(value) else {
        return false;
    };

    if let Some(scheme) = &uri.scheme
        && (scheme.eq_ignore_ascii_case(b"http") || scheme.eq_ignore_ascii_case(b"https"))
    {
        let Some(host) = &uri.host else {
            return false;
        };

        if
        // An IPv6 enclosed by square brackets is a valid hostname.
        !php_filter_is_valid_ipv6_hostname(host)
            // Validate domain.
            // This includes a loose check for an IPv4 address.
            && !php_filter_validate_domain_ex(host, FILTER_FLAG_HOSTNAME)
        {
            return false;
        }
    }

    let Some(scheme) = &uri.scheme else {
        return false;
    };
    // some schemes allow the host to be empty
    if (uri.host.is_none() && scheme != "mailto" && scheme != "news" && scheme != "file")
        || (flags & FILTER_FLAG_PATH_REQUIRED != 0 && uri.path.is_none())
        || (flags & FILTER_FLAG_QUERY_REQUIRED != 0 && uri.query.is_none())
    {
        return false;
    }

    if uri
        .user
        .as_ref()
        .is_some_and(|user| !is_userinfo_valid(user))
        || uri
            .pass
            .as_ref()
            .is_some_and(|pass| !is_userinfo_valid(pass))
    {
        return false;
    }

    true
}

/// php-src: ext/filter/logical_filters.c `_php_filter_validate_ipv4` (PHP 8.5.5)
fn _php_filter_validate_ipv4(str: &[u8], ip: &mut [i32]) -> bool {
    let end = str.len();
    let mut s = 0;
    let mut n = 0;

    while s < end {
        if !str[s].is_ascii_digit() {
            return false;
        }
        let leading_zero = str[s] == b'0';
        let mut m = 1;
        let mut num = i32::from(str[s] - b'0');
        s += 1;
        while s < end && str[s].is_ascii_digit() {
            num = num * 10 + i32::from(str[s] - b'0');
            s += 1;
            m += 1;
            if num > 255 || m > 3 {
                return false;
            }
        }
        // don't allow a leading 0; that introduces octal numbers,
        // which we don't support
        if leading_zero && (num != 0 || m > 1) {
            return false;
        }
        ip[n] = num;
        n += 1;
        if n == 4 {
            return s == end;
        } else if s >= end || str[s] != b'.' {
            return false;
        }
        s += 1;
    }
    false
}

/// php-src: ext/filter/logical_filters.c `_php_filter_validate_ipv6` (PHP 8.5.5)
///
/// `ip` is `None` where the original is passed `NULL`.
fn _php_filter_validate_ipv6(str: &[u8], mut ip: Option<&mut [i32; 8]>) -> bool {
    let mut compressed_pos: i32 = -1;
    let mut blocks: i32 = 0;
    let mut ip4elm = [0i32; 4];
    let mut str_len = str.len();
    let mut p = 0;

    if !str.contains(&b':') {
        return false;
    }

    // check for bundled IPv4
    let ipv4 = str.iter().position(|&c| c == b'.');
    if let Some(mut ipv4) = ipv4 {
        while ipv4 > 0 && str[ipv4 - 1] != b':' {
            ipv4 -= 1;
        }

        if !_php_filter_validate_ipv4(&str[ipv4..], &mut ip4elm) {
            return false;
        }

        // length excluding ipv4
        str_len = ipv4;
        if str_len < 2 {
            return false;
        }

        if str[ipv4 - 2] != b':' {
            // don't include : before ipv4 unless it's a ::
            str_len -= 1;
        }

        blocks = 2;
    }

    let end = str_len;

    'parse: {
        while p < end {
            if str[p] == b':' {
                p += 1;
                if p >= end {
                    // cannot end in : without previous :
                    return false;
                }
                if str[p] == b':' {
                    if compressed_pos >= 0 {
                        return false;
                    }
                    if let Some(ip) = ip.as_deref_mut()
                        && blocks < 8
                    {
                        ip[blocks as usize] = -1;
                    }
                    // :: means 1 or more 16-bit 0 blocks
                    compressed_pos = blocks;
                    blocks += 1;
                    p += 1;
                    if p == end {
                        if blocks > 8 {
                            return false;
                        }
                        break 'parse;
                    }
                } else if p - 1 == 0 {
                    // don't allow leading : without another : following
                    return false;
                }
            }
            let mut num: u32 = 0;
            let mut n = 0;
            while p < end {
                let digit = match str[p] {
                    c @ b'0'..=b'9' => c - b'0',
                    c @ b'a'..=b'f' => c - b'a' + 10,
                    c @ b'A'..=b'F' => c - b'A' + 10,
                    _ => break,
                };
                num = num.wrapping_mul(16).wrapping_add(u32::from(digit));
                n += 1;
                p += 1;
            }
            if let Some(ip) = ip.as_deref_mut()
                && blocks < 8
            {
                ip[blocks as usize] = num as i32;
            }
            if !(1..=4).contains(&n) {
                return false;
            }
            blocks += 1;
            if blocks > 8 {
                return false;
            }
        }
    }

    // fixup_ip:
    if let Some(ip) = ip {
        if ipv4.is_some() {
            ip[..5].fill(0);
            ip[5] = 0xffff;
            ip[6] = 256 * ip4elm[0] + ip4elm[1];
            ip[7] = 256 * ip4elm[2] + ip4elm[3];
        } else if compressed_pos >= 0 && blocks <= 8 {
            let offset = 8 - blocks;
            let mut i = 7;
            while i > compressed_pos + offset {
                ip[i as usize] = ip[(i - offset) as usize];
                i -= 1;
            }
            let mut i = compressed_pos + offset;
            while i >= compressed_pos {
                ip[i as usize] = 0;
                i -= 1;
            }
        }
    }

    (compressed_pos >= 0 && blocks <= 8) || blocks == 8
}

/// The status flags of `ipv4_get_status_flags` and `ipv6_get_status_flags`.
#[derive(Debug, Default)]
struct StatusFlags {
    global: bool,
    reserved: bool,
    private: bool,
}

/// php-src: ext/filter/logical_filters.c `ipv4_get_status_flags` (PHP 8.5.5)
///
/// `None` where the original returns `false`.
fn ipv4_get_status_flags(ip: &[i32; 8]) -> Option<StatusFlags> {
    let mut flags = StatusFlags::default();

    // From the tables in RFC 6890 - Special-Purpose IP Address Registriesi
    // Including errata: https://www.rfc-editor.org/errata_search.php?rfc=6890&rec_status=1
    if ip[0] == 0 {
        // RFC 0791 - This network
        flags.reserved = true;
    } else if ip[0] == 0 && ip[1] == 0 && ip[2] == 0 && ip[3] == 0 {
        // RFC 1122 - This host on this network
        flags.reserved = true;
    } else if ip[0] == 10 {
        // RFC 1918 - Private Use
        flags.private = true;
    } else if ip[0] == 100 && ip[1] >= 64 && ip[1] <= 127 {
        // RFC 6598 - Shared Address Space
    } else if ip[0] == 127 {
        // RFC 1122 - Loopback
        flags.reserved = true;
    } else if ip[0] == 169 && ip[1] == 254 {
        // RFC 3927 - Link Local
        flags.reserved = true;
    } else if ip[0] == 172 && ip[1] >= 16 && ip[1] <= 31 {
        // RFC 1918 - Private Use
        flags.private = true;
    } else if ip[0] == 192 && ip[1] == 0 && ip[2] == 0 {
        // RFC 6890 - IETF Protocol Assignments
    } else if ip[0] == 192 && ip[1] == 0 && ip[2] == 0 && ip[3] >= 0 && ip[3] <= 7 {
        // RFC 6333 - DS-Lite
    } else if ip[0] == 192 && ip[1] == 0 && ip[2] == 2 {
        // RFC 5737 - Documentation
    } else if ip[0] == 192 && ip[1] == 88 && ip[2] == 99 {
        // RFC 3068 - 6to4 Relay Anycast
        flags.global = true;
    } else if ip[0] == 192 && ip[1] == 168 {
        // RFC 1918 - Private Use
        flags.private = true;
    } else if ip[0] == 198 && ip[1] >= 18 && ip[1] <= 19 {
        // RFC 2544 - Benchmarking
    } else if ip[0] == 198 && ip[1] == 51 && ip[2] == 100 {
        // RFC 5737 - Documentation
    } else if ip[0] == 203 && ip[1] == 0 && ip[2] == 113 {
        // RFC 5737 - Documentation
    } else if ip[0] >= 240 && ip[1] <= 255 {
        // RFC 1122 - Reserved
        flags.reserved = true;
    } else if ip[0] == 255 && ip[1] == 255 && ip[2] == 255 && ip[3] == 255 {
        // RFC 0919 - Limited Broadcast, Updated by RFC 8190, 2.2.
        flags.reserved = true;
    } else {
        return None;
    }

    Some(flags)
}

/// php-src: ext/filter/logical_filters.c `ipv6_get_status_flags` (PHP 8.5.5)
///
/// `None` where the original returns `false`.
fn ipv6_get_status_flags(ip: &[i32; 8]) -> Option<StatusFlags> {
    let mut flags = StatusFlags::default();

    // From the tables in RFC 6890 - Special-Purpose IP Address Registries
    if ip[0] == 0
        && ip[1] == 0
        && ip[2] == 0
        && ip[3] == 0
        && ip[4] == 0
        && ip[5] == 0
        && ip[6] == 0
        && ip[7] == 0
    {
        // RFC 4291 - Unspecified Address
        flags.reserved = true;
    } else if ip[0] == 0
        && ip[1] == 0
        && ip[2] == 0
        && ip[3] == 0
        && ip[4] == 0
        && ip[5] == 0
        && ip[6] == 0
        && ip[7] == 1
    {
        // RFC 4291 - Loopback Address
        flags.reserved = true;
    } else if ip[0] == 0x0064 && ip[1] == 0xff9b {
        // RFC 6052 - IPv4-IPv6 Translation
        flags.global = true;
    } else if ip[0] == 0 && ip[1] == 0 && ip[2] == 0 && ip[3] == 0 && ip[4] == 0 && ip[5] == 0xffff
    {
        // RFC 4291 - IPv4-mapped Address
        flags.reserved = true;
    } else if ip[0] == 0x0100 && ip[1] == 0 && ip[2] == 0 && ip[3] == 0 {
        // RFC 6666 - Discard-Only Address Block
    } else if ip[0] == 0x2001 && ip[1] == 0x0000 {
        // RFC 4380 - TEREDO
    } else if ip[0] == 0x2001 && ip[1] <= 0x01ff {
        // RFC 2928 - IETF Protocol Assignments
    } else if ip[0] == 0x2001 && ip[1] == 0x0002 && ip[2] == 0 {
        // RFC 5180 - Benchmarking
    } else if ip[0] == 0x2001 && ip[1] == 0x0db8 {
        // RFC 3849 - Documentation
    } else if ip[0] == 0x2001 && ip[1] >= 0x0010 && ip[1] <= 0x001f {
        // RFC 4843 - ORCHID
    } else if ip[0] == 0x2002 {
        // RFC 3056 - 6to4
    } else if ip[0] >= 0xfc00 && ip[0] <= 0xfdff {
        // RFC 4193 - Unique-Local
        flags.private = true;
    } else if ip[0] >= 0xfe80 && ip[0] <= 0xfebf {
        // RFC 4291 - Linked-Scoped Unicast
        flags.reserved = true;
    } else {
        return None;
    }

    Some(flags)
}

/// php-src: ext/filter/logical_filters.c `php_filter_validate_ip` (PHP 8.5.5)
///
/// Returns whether `value` passes validation.
pub fn php_filter_validate_ip(value: &[u8], flags: i64) -> bool {
    let mut ip = [0i32; 8];

    let mode = if value.contains(&b':') {
        Format::Ipv6
    } else if value.contains(&b'.') {
        Format::Ipv4
    } else {
        return false;
    };

    if flags & FILTER_FLAG_IPV4 != 0 && flags & FILTER_FLAG_IPV6 != 0 {
        // Both formats are cool
    } else if (flags & FILTER_FLAG_IPV4 != 0 && mode == Format::Ipv6)
        || (flags & FILTER_FLAG_IPV6 != 0 && mode == Format::Ipv4)
    {
        return false;
    }

    let status = match mode {
        Format::Ipv4 => {
            if !_php_filter_validate_ipv4(value, &mut ip) {
                return false;
            }
            ipv4_get_status_flags(&ip)
        }
        Format::Ipv6 => {
            if !_php_filter_validate_ipv6(value, Some(&mut ip)) {
                return false;
            }
            ipv6_get_status_flags(&ip)
        }
    };
    let Some(status) = status else {
        // no special block
        return true;
    };

    if flags & FILTER_FLAG_GLOBAL_RANGE != 0 && !status.global {
        return false;
    }

    if flags & FILTER_FLAG_NO_PRIV_RANGE != 0 && status.private {
        return false;
    }

    if flags & FILTER_FLAG_NO_RES_RANGE != 0 && status.reserved {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use bstr::ByteSlice;

    #[test]
    fn boolean() {
        for value in ["1", " yes ", "On", "\x0btrue\r"] {
            assert_eq!(
                Some(true),
                php_filter_boolean(value.as_bytes()),
                "{value:?}"
            );
        }
        for value in ["0", "off", "", " "] {
            assert_eq!(
                Some(false),
                php_filter_boolean(value.as_bytes()),
                "{value:?}"
            );
        }
        for value in ["1\0", "\x0ctrue", "2", "truee"] {
            assert_eq!(None, php_filter_boolean(value.as_bytes()), "{value:?}");
        }
    }

    #[test]
    fn int() {
        let cases: [(&[u8], i64, Option<i64>); 15] = [
            (b"0", 0, Some(0)),
            (b"-0", 0, Some(0)),
            (b"+0", 0, Some(0)),
            (b"00", 0, None),
            (b"01", 0, None),
            (b" 12\n", 0, Some(12)),
            (b"\x0c1", 0, None),
            (b"9223372036854775807", 0, Some(i64::MAX)),
            (b"9223372036854775808", 0, None),
            (b"-9223372036854775808", 0, Some(i64::MIN)),
            (b"0x1A", FILTER_FLAG_ALLOW_HEX, Some(26)),
            (b"0x", FILTER_FLAG_ALLOW_HEX, None),
            (b"0o17", FILTER_FLAG_ALLOW_OCTAL, Some(15)),
            (b"017", FILTER_FLAG_ALLOW_OCTAL, Some(15)),
            (b"0xFFFFFFFFFFFFFFFF", FILTER_FLAG_ALLOW_HEX, Some(-1)),
        ];
        for (value, flags, expected) in cases {
            assert_eq!(
                expected,
                php_filter_int(value, flags, None, None),
                "{:?}",
                value.as_bstr()
            );
        }
    }

    #[test]
    fn int_range() {
        assert_eq!(Some(5), php_filter_int(b"5", 0, Some(5), Some(5)));
        assert_eq!(None, php_filter_int(b"4", 0, Some(5), None));
        assert_eq!(None, php_filter_int(b"6", 0, None, Some(5)));
    }

    #[test]
    fn ip() {
        for value in [
            "127.0.0.1",
            "::1",
            "::ffff:1.2.3.4",
            "1:2:3:4:5:6:7:8",
            "1:2:3:4:5:6:1.2.3.4",
        ] {
            assert!(php_filter_validate_ip(value.as_bytes(), 0), "{value:?}");
        }
        for value in [
            "01.2.3.4",
            "256.1.1.1",
            " 1.2.3.4",
            "1:2:3:4:5:6:7:8:9",
            "1::2::3",
            ":1::2",
            "1:2:3:4:5:6:7:1.2.3.4",
            "fe80::1%eth0",
            "12345::",
        ] {
            assert!(!php_filter_validate_ip(value.as_bytes(), 0), "{value:?}");
        }
    }

    #[test]
    fn ip_flags() {
        let cases = [
            ("10.0.0.1", FILTER_FLAG_NO_PRIV_RANGE, false),
            ("127.0.0.1", FILTER_FLAG_NO_RES_RANGE, false),
            ("192.88.99.1", FILTER_FLAG_GLOBAL_RANGE, true),
            ("100.64.0.1", FILTER_FLAG_GLOBAL_RANGE, false),
            ("fc00::1", FILTER_FLAG_NO_PRIV_RANGE, false),
            ("::1", FILTER_FLAG_IPV4, false),
            ("1.2.3.4", FILTER_FLAG_IPV6, false),
        ];
        for (value, flags, expected) in cases {
            assert_eq!(
                expected,
                php_filter_validate_ip(value.as_bytes(), flags),
                "{value:?}"
            );
        }
    }

    #[test]
    fn url() {
        for value in [
            "http://example.com",
            "mailto:a@b.c",
            "file:///etc/passwd",
            "http://a-.",
            "http://[::1]:80/",
            "http://us%2er@a.com",
            "ssh://git@github.com:22/x",
            "hg://user:pass@my.satis/satis",
        ] {
            assert!(php_filter_validate_url(value.as_bytes(), 0), "{value:?}");
        }
        for value in [
            "git:Department/Repo.git",
            "x://",
            "http:///a",
            "http://ex_ample.com",
            "http://a-.com",
            "http://[zz]/",
            "http://us%e2r@a.com",
            "http://a.com\x01",
            "http://é.com",
        ] {
            assert!(!php_filter_validate_url(value.as_bytes(), 0), "{value:?}");
        }
    }

    #[test]
    fn url_flags() {
        let cases = [
            ("http://a.com", FILTER_FLAG_PATH_REQUIRED, false),
            ("http://a.com/", FILTER_FLAG_PATH_REQUIRED, true),
            ("http://a.com/", FILTER_FLAG_QUERY_REQUIRED, false),
            ("http://a.com/?q", FILTER_FLAG_QUERY_REQUIRED, true),
        ];
        for (value, flags, expected) in cases {
            assert_eq!(
                expected,
                php_filter_validate_url(value.as_bytes(), flags),
                "{value:?}"
            );
        }
    }
}
