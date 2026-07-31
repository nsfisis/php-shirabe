/// php-src: ext/standard/strnatcmp.c `strnatcmp_ex` (PHP 8.5.2)
///
/// Operating on byte slices, an out-of-range index reads as 0, reproducing the NUL terminator
/// that the C implementation relies on.
pub fn strnatcmp_ex(a: &[u8], b: &[u8], fold_case: bool) -> i64 {
    let a_len = a.len();
    let b_len = b.len();
    if a_len == 0 || b_len == 0 {
        return match a_len.cmp(&b_len) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Greater => 1,
            std::cmp::Ordering::Equal => 0,
        };
    }

    let mut ap = 0usize;
    let mut bp = 0usize;
    let mut leading = true;
    loop {
        let mut ca = natcmp_at(a, ap);
        let mut cb = natcmp_at(b, bp);

        // Skip over leading zeros.
        while leading && ca == b'0' && natcmp_at(a, ap + 1).is_ascii_digit() {
            ap += 1;
            ca = natcmp_at(a, ap);
        }
        while leading && cb == b'0' && natcmp_at(b, bp + 1).is_ascii_digit() {
            bp += 1;
            cb = natcmp_at(b, bp);
        }
        leading = false;

        // Skip consecutive whitespace.
        while natcmp_is_space(ca) {
            ap += 1;
            ca = natcmp_at(a, ap);
        }
        while natcmp_is_space(cb) {
            bp += 1;
            cb = natcmp_at(b, bp);
        }

        // Process a run of digits.
        if ca.is_ascii_digit() && cb.is_ascii_digit() {
            let fractional = ca == b'0' || cb == b'0';
            let result = if fractional {
                natcmp_compare_left(a, &mut ap, b, &mut bp)
            } else {
                natcmp_compare_right(a, &mut ap, b, &mut bp)
            };
            if result != 0 {
                return result;
            }
        }

        if ap == a_len && bp == b_len {
            return 0;
        } else if ap == a_len {
            return -1;
        } else if bp == b_len {
            return 1;
        }

        if fold_case {
            ca = natcmp_at(a, ap).to_ascii_uppercase();
            cb = natcmp_at(b, bp).to_ascii_uppercase();
        } else {
            ca = natcmp_at(a, ap);
            cb = natcmp_at(b, bp);
        }

        if ca < cb {
            return -1;
        } else if ca > cb {
            return 1;
        }

        ap += 1;
        bp += 1;
    }
}

/// php-src: ext/standard/strnatcmp.c (no direct counterpart) (PHP 8.5.2)
fn natcmp_at(s: &[u8], i: usize) -> u8 {
    if i < s.len() { s[i] } else { 0 }
}

/// php-src: ext/standard/strnatcmp.c `isspace(3)` usage (PHP 8.5.2)
fn natcmp_is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// php-src: ext/standard/strnatcmp.c `compare_right` (PHP 8.5.2)
///
/// Compare two right-aligned numbers: the longest run of digits wins; failing that, the first
/// differing digit decides, but only once magnitudes are known equal (tracked in `bias`).
fn natcmp_compare_right(a: &[u8], ap: &mut usize, b: &[u8], bp: &mut usize) -> i64 {
    let mut bias = 0i64;
    loop {
        let ca = natcmp_at(a, *ap);
        let cb = natcmp_at(b, *bp);
        let a_digit = ca.is_ascii_digit();
        let b_digit = cb.is_ascii_digit();
        if !a_digit && !b_digit {
            return bias;
        } else if !a_digit {
            return -1;
        } else if !b_digit {
            return 1;
        } else if ca < cb {
            if bias == 0 {
                bias = -1;
            }
        } else if ca > cb && bias == 0 {
            bias = 1;
        }
        *ap += 1;
        *bp += 1;
    }
}

/// php-src: ext/standard/strnatcmp.c `compare_left` (PHP 8.5.2)
///
/// Compare two left-aligned numbers: the first differing digit decides.
fn natcmp_compare_left(a: &[u8], ap: &mut usize, b: &[u8], bp: &mut usize) -> i64 {
    loop {
        let ca = natcmp_at(a, *ap);
        let cb = natcmp_at(b, *bp);
        let a_digit = ca.is_ascii_digit();
        let b_digit = cb.is_ascii_digit();
        if !a_digit && !b_digit {
            return 0;
        } else if !a_digit {
            return -1;
        } else if !b_digit {
            return 1;
        } else if ca < cb {
            return -1;
        } else if ca > cb {
            return 1;
        }
        *ap += 1;
        *bp += 1;
    }
}
