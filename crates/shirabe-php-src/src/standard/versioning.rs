/// php-src: ext/standard/versioning.c `php_version_compare` (PHP 8.5.2)
///
/// Returns -1, 0 or 1. The original walks the canonicalized strings with destructive `.` splits
/// and moving pointers; this walks a `split('.')` iterator over each instead.
pub fn php_version_compare(v1: impl AsRef<[u8]>, v2: impl AsRef<[u8]>) -> i32 {
    let v1 = v1.as_ref();
    let v2 = v2.as_ref();
    if v1.is_empty() || v2.is_empty() {
        return match (v1.is_empty(), v2.is_empty()) {
            (true, true) => 0,
            (false, _) => 1,
            (_, false) => -1,
        };
    }
    let c1 = canonicalize_version(v1);
    let c2 = canonicalize_version(v2);
    let mut t1 = c1.split(|&b| b == b'.').filter(|s| !s.is_empty());
    let mut t2 = c2.split(|&b| b == b'.').filter(|s| !s.is_empty());

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
            compare = if p[0].is_ascii_digit() {
                1
            } else {
                special_form_order(p).cmp(&4) as i32
            };
        } else if let Some(p) = p2 {
            compare = if p[0].is_ascii_digit() {
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
fn canonicalize_version(version: &[u8]) -> Vec<u8> {
    let bytes = version;
    if bytes.is_empty() {
        return Vec::new();
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
    q
}

/// php-src: ext/standard/versioning.c `php_version_compare` loop body (PHP 8.5.2)
fn version_token_compare(t1: &[u8], t2: &[u8]) -> i32 {
    let d1 = t1[0].is_ascii_digit();
    let d2 = t2[0].is_ascii_digit();
    if d1 && d2 {
        let l1 = strtol(t1);
        let l2 = strtol(t2);
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
fn special_form_order(form: &[u8]) -> i32 {
    const FORMS: &[(&[u8], i32)] = &[
        (b"dev", 0),
        (b"alpha", 1),
        (b"a", 1),
        (b"beta", 2),
        (b"b", 2),
        (b"RC", 3),
        (b"rc", 3),
        (b"#", 4),
        (b"pl", 5),
        (b"p", 5),
    ];
    for (name, order) in FORMS {
        if form.starts_with(name) {
            return *order;
        }
    }
    -1
}

/// `strtol(token, NULL, 10)` for a token of ASCII digits, which saturates at `LONG_MAX`.
fn strtol(digits: &[u8]) -> i64 {
    digits.iter().fold(0i64, |acc, &d| {
        acc.saturating_mul(10).saturating_add(i64::from(d - b'0'))
    })
}
