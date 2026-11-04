const LOWALPHA: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
const HIALPHA: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGIT: &[u8] = b"0123456789";

const SAFE: &[u8] = b"$-_.+";
const EXTRA: &[u8] = b"!*'(),";
const NATIONAL: &[u8] = b"{}|\\^~[]`";
const PUNCTUATION: &[u8] = b"<>#%\"";
const RESERVED: &[u8] = b";/?:@&=";

/// php-src: ext/filter/sanitizing_filters.c `php_filter_url` (PHP 8.5.5)
///
/// Returns the filtered string instead of replacing the zval in place.
pub fn php_filter_url(value: &[u8]) -> Vec<u8> {
    // Strip all chars not part of section 5 of
    // http://www.faqs.org/rfcs/rfc1738.html
    let allowed_list = [
        LOWALPHA,
        HIALPHA,
        DIGIT,
        SAFE,
        EXTRA,
        NATIONAL,
        PUNCTUATION,
        RESERVED,
    ];
    value
        .iter()
        .copied()
        .filter(|c| allowed_list.iter().any(|list| list.contains(c)))
        .collect()
}
