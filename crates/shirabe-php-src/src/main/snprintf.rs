/// php-src: main/snprintf.c `php_gcvt` (PHP 8.5.8)
///
/// Only the `ndigit < 0` path (dtoa mode 0, the shortest round-trip representation used when
/// `serialize_precision=-1`) is ported; the fixed-precision mode 2 path is not needed yet.
/// The digit extraction delegates to Rust's own shortest round-trip float formatting, which
/// produces the same digit string as `zend_dtoa` in mode 0 (both compute the unique shortest
/// decimal that round-trips), so only the digit placement logic is ported here.
pub fn php_gcvt(value: f64, ndigit: i32, dec_point: char, exponent: char) -> String {
    assert!(ndigit == -1);
    assert!(dec_point == '.');
    assert!(exponent == 'E');

    let mode = if ndigit >= 0 { 2 } else { 0 };
    if mode != 0 {
        unreachable!("php_gcvt is only ported for ndigit < 0 (serialize_precision=-1)");
    }
    let ndigit = 17i32;

    let (sign, digits, decpt) = dtoa_shortest(value);

    let mut buf = String::new();
    if sign {
        buf.push('-');
    }

    if if decpt < 0 {
        decpt < -3
    } else {
        decpt > ndigit
    } {
        // exponential format (e.g. 1.0E+17)
        let exp = decpt - 1;
        let mut chars = digits.chars();
        buf.push(chars.next().expect("dtoa always yields at least one digit"));
        buf.push(dec_point);
        let rest = chars.as_str();
        if rest.is_empty() {
            buf.push('0');
        } else {
            buf.push_str(rest);
        }
        buf.push(exponent);
        if exp < 0 {
            buf.push('-');
        } else {
            buf.push('+');
        }
        buf.push_str(&exp.abs().to_string());
    } else if decpt > 0 {
        // standard format, integer part present
        let decpt = decpt as usize;
        if digits.len() <= decpt {
            buf.push_str(&digits);
            for _ in digits.len()..decpt {
                buf.push('0');
            }
        } else {
            buf.push_str(&digits[..decpt]);
            buf.push(dec_point);
            buf.push_str(&digits[decpt..]);
        }
    } else {
        // standard format, 0.000ddd
        buf.push('0');
        buf.push(dec_point);
        for _ in decpt..0 {
            buf.push('0');
        }
        buf.push_str(&digits);
    }

    buf
}

/// php-src: Zend/zend_strtod.c `zend_dtoa` mode 0 equivalent: the shortest round-trip digit
/// string of `|value|`, its sign, and the decimal point position (`value = 0.digits * 10^decpt`).
/// Implemented on top of Rust's `{:e}` formatting, which is also shortest-round-trip.
fn dtoa_shortest(value: f64) -> (bool, String, i32) {
    let sign = value.is_sign_negative();
    let formatted = format!("{:e}", value.abs());
    let (mantissa, exp) = formatted
        .split_once('e')
        .expect("`{:e}` always contains an exponent");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let exp: i32 = exp.parse().expect("`{:e}` exponent is a decimal integer");
    (sign, digits, exp + 1)
}
