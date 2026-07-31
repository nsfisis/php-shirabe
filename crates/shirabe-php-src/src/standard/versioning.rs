/// php-src: ext/standard/versioning.c `php_version_compare` (PHP 8.5.2)
///
/// Returns -1, 0 or 1. The original walks the canonicalized strings with destructive `.` splits
/// and moving pointers; this splits into a `Vec<&str>` and indexes instead.
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
    let t1: Vec<&str> = c1.split('.').filter(|s| !s.is_empty()).collect();
    let t2: Vec<&str> = c2.split('.').filter(|s| !s.is_empty()).collect();

    let mut compare = 0;
    let mut i = 0;
    while i < t1.len() && i < t2.len() && compare == 0 {
        compare = version_token_compare(t1[i], t2[i]);
        i += 1;
    }
    if compare == 0 {
        // A leftover numeric token wins; a leftover special form is compared against the implicit
        // release baseline ("#", order 4).
        if i < t1.len() {
            let p = t1[i];
            compare = if p.as_bytes()[0].is_ascii_digit() {
                1
            } else {
                special_form_order(p).cmp(&4) as i32
            };
        } else if i < t2.len() {
            let p = t2[i];
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
    String::from_utf8_lossy(&q).into_owned()
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
