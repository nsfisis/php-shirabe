/// php-src: ext/standard/versioning.c `php_version_compare` (PHP 8.5.2)
///
/// Returns -1, 0 or 1. The original walks the canonicalized strings with destructive `.` splits
/// and moving pointers; this walks a `split('.')` iterator over each instead.
pub fn php_version_compare(v1: &str, v2: &str) -> i32 {
    if v1.is_empty() || v2.is_empty() {
        return match (v1.is_empty(), v2.is_empty()) {
            (true, true) => 0,
            (false, _) => 1,
            (_, false) => -1,
        };
    }
    let c1 = canonicalize_version(v1);
    let c2 = canonicalize_version(v2);
    let mut t1 = c1.split('.').filter(|s| !s.is_empty());
    let mut t2 = c2.split('.').filter(|s| !s.is_empty());

    let mut compare = 0;
    let mut p1 = t1.next();
    let mut p2 = t2.next();
    while compare == 0
        && let (Some(a), Some(b)) = (p1, p2)
    {
        compare = version_token_compare(a, b);
        p1 = t1.next();
        p2 = t2.next();
    }
    if compare == 0 {
        // A leftover numeric token wins; a leftover special form is compared against the implicit
        // release baseline ("#", order 4).
        if let Some(p) = p1 {
            compare = if p.as_bytes()[0].is_ascii_digit() {
                1
            } else {
                special_form_order(p).cmp(&4) as i32
            };
        } else if let Some(p) = p2 {
            compare = if p.as_bytes()[0].is_ascii_digit() {
                -1
            } else {
                4.cmp(&special_form_order(p)) as i32
            };
        }
    }
    compare
}

/// php-src: ext/standard/versioning.c `php_canonicalize_version` (PHP 8.5.2)
///
/// Separators (-, _, +, .) collapse to a single '.', and a '.' is inserted at every digit <->
/// non-digit boundary. The original's `!isalnum(*p)` branch is not ported.
fn canonicalize_version(version: &str) -> String {
    let bytes = version.as_bytes();
    if bytes.is_empty() {
        return String::new();
    }
    let mut q: Vec<u8> = Vec::with_capacity(bytes.len() * 2);
    q.push(bytes[0]);
    for &raw in &bytes[1..] {
        let ch = if matches!(raw, b'-' | b'_' | b'+') {
            b'.'
        } else {
            raw
        };
        let last = *q.last().unwrap();
        if ch == b'.' {
            if last != b'.' {
                q.push(b'.');
            }
        } else if last.is_ascii_digit() != ch.is_ascii_digit() {
            q.push(b'.');
            q.push(ch);
        } else {
            q.push(ch);
        }
    }
    // Splits are only ever inserted at ASCII digit <-> non-digit boundaries, which are always
    // char boundaries, so a valid UTF-8 input stays valid.
    String::from_utf8(q).expect("canonicalized version is valid UTF-8")
}

/// php-src: ext/standard/versioning.c `php_version_compare` loop body (PHP 8.5.2)
fn version_token_compare(t1: &str, t2: &str) -> i32 {
    let d1 = t1.as_bytes()[0].is_ascii_digit();
    let d2 = t2.as_bytes()[0].is_ascii_digit();
    if d1 && d2 {
        let l1 = t1.parse::<i64>().unwrap_or(0);
        let l2 = t2.parse::<i64>().unwrap_or(0);
        l1.cmp(&l2) as i32
    } else if !d1 && !d2 {
        special_form_order(t1).cmp(&special_form_order(t2)) as i32
    } else if d1 {
        // A numeric token is treated as the "#" form (order 4).
        4.cmp(&special_form_order(t2)) as i32
    } else {
        special_form_order(t1).cmp(&4) as i32
    }
}

/// php-src: ext/standard/versioning.c `compare_special_version_forms` (PHP 8.5.2)
fn special_form_order(form: &str) -> i32 {
    const FORMS: &[(&str, i32)] = &[
        ("dev", 0),
        ("alpha", 1),
        ("a", 1),
        ("beta", 2),
        ("b", 2),
        ("RC", 3),
        ("rc", 3),
        ("#", 4),
        ("pl", 5),
        ("p", 5),
    ];
    for (name, order) in FORMS {
        if form.starts_with(name) {
            return *order;
        }
    }
    -1
}
