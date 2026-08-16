/// php-src: main/snprintf.c `php_gcvt` (PHP 8.5.8)
///
/// The digit extraction is delegated to `dtoa_shortest` / `dtoa_fixed`, so only the digit
/// placement logic is ported here. `value` must be finite: the caller strips NAN and INF, so the
/// `decpt == 9999` branch has no counterpart here.
pub fn php_gcvt(value: f64, ndigit: i32, dec_point: char, exponent: char) -> String {
    let mode = if ndigit >= 0 { 2 } else { 0 };
    // Mode 0 asks dtoa for the shortest round-trip digits, so its digit budget is the widest a
    // double can need. Mode 2 yields a single digit when asked for fewer.
    let ndigit = if mode == 0 { 17 } else { ndigit.max(1) };

    let (sign, digits, decpt) = if mode == 0 {
        dtoa_shortest(value)
    } else {
        dtoa_fixed(value, ndigit)
    };

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
/// Rust's `{:e}` supplies the shortest round-trip digit count, but it breaks a tie between two
/// equally distant digit strings away from zero while `zend_dtoa` breaks it to even, so the digits
/// themselves come from `dtoa_fixed` at that count.
fn dtoa_shortest(value: f64) -> (bool, String, i32) {
    let shortest = format!("{:e}", value.abs());
    let shortest_mantissa = shortest
        .split_once('e')
        .expect("`{:e}` always contains an exponent")
        .0;
    let ndigits = shortest_mantissa.chars().filter(|c| *c != '.').count();

    dtoa_fixed(value, ndigits as i32)
}

/// php-src: Zend/zend_strtod.c `zend_dtoa` mode 2 equivalent: `|value|` rounded to `ndigit`
/// significant decimal digits, its sign, and the decimal point position
/// (`value = 0.digits * 10^decpt`). Rust's `{:.*e}` rounds the exact decimal value of the double
/// half to even, which is how `zend_dtoa` breaks a tie. Trailing zeros are dropped, as dtoa never
/// emits them; a value that rounds down to nothing keeps a single `0` digit.
fn dtoa_fixed(value: f64, ndigit: i32) -> (bool, String, i32) {
    let sign = value.is_sign_negative();
    let value = value.abs();

    let formatted = format!("{:.*e}", (ndigit - 1) as usize, value);
    let (mantissa, exp) = formatted
        .split_once('e')
        .expect("`{:e}` always contains an exponent");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let exp: i32 = exp.parse().expect("`{:e}` exponent is a decimal integer");

    let trimmed = digits.trim_end_matches('0');
    let digits = if trimmed.is_empty() { "0" } else { trimmed };
    (sign, digits.to_string(), exp + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gcvt(value: f64) -> String {
        php_gcvt(value, -1, '.', 'E')
    }

    fn gcvt14(value: f64) -> String {
        php_gcvt(value, 14, '.', 'E')
    }

    // Expected strings are the output of `(string) $v` under PHP 8.5.9 with precision=14, which
    // reaches php_gcvt with ndigit=14. Compared with ndigit=17 the digits are rounded to 14
    // significant places and the switch to exponential format moves down with ndigit.
    #[test]
    #[allow(clippy::excessive_precision)]
    fn fixed_precision_rounds_to_ndigit_digits() {
        assert_eq!(gcvt14(0.0), "0");
        assert_eq!(gcvt14(-0.0), "-0");
        assert_eq!(gcvt14(1.0), "1");
        assert_eq!(gcvt14(100.0), "100");
        assert_eq!(gcvt14(-1.5), "-1.5");
        assert_eq!(gcvt14(1.0 / 3.0), "0.33333333333333");
        assert_eq!(gcvt14(1.0 / 7.0), "0.14285714285714");
        assert_eq!(gcvt14(0.30000000000000004), "0.3");
        assert_eq!(gcvt14(1e-4), "0.0001");
        assert_eq!(gcvt14(1e-5), "1.0E-5");
        assert_eq!(gcvt14(9.99999999999999e-5), "0.0001");
        assert_eq!(gcvt14(1.23456789e-5), "1.23456789E-5");
        assert_eq!(gcvt14(1e13), "10000000000000");
        assert_eq!(gcvt14(1e14), "1.0E+14");
        assert_eq!(gcvt14(1e15), "1.0E+15");
        assert_eq!(gcvt14(99999999999999.99), "1.0E+14");
        assert_eq!(gcvt14(123456789012345.6), "1.2345678901235E+14");
        assert_eq!(gcvt14(1.2345678901234568e16), "1.2345678901235E+16");
        assert_eq!(gcvt14(f64::MAX), "1.7976931348623E+308");
        assert_eq!(gcvt14(f64::MIN_POSITIVE), "2.2250738585072E-308");
        assert_eq!(gcvt14(5e-324), "4.9406564584125E-324");
    }

    // Each value here is exactly representable and sits halfway between two 14-digit decimal
    // strings, so it pins down the tie break. Expected strings are the output of `(string) $v`
    // under PHP 8.5.9 with precision=14.
    #[test]
    fn fixed_precision_ties_round_to_even() {
        assert_eq!(gcvt14(12345678901234.5), "12345678901234");
        assert_eq!(gcvt14(12345678901235.5), "12345678901236");
        assert_eq!(gcvt14(12345678901236.5), "12345678901236");
        assert_eq!(gcvt14(99999999999998.5), "99999999999998");
        assert_eq!(gcvt14(1234567890123.25), "1234567890123.2");
        assert_eq!(gcvt14(1234567890123.75), "1234567890123.8");
    }

    // PHP clamps precision=0 to a single significant digit.
    #[test]
    fn fixed_precision_below_one_digit_yields_one_digit() {
        assert_eq!(php_gcvt(1.0 / 3.0, 0, '.', 'E'), "0.3");
        assert_eq!(php_gcvt(123.456, 0, '.', 'E'), "1.0E+2");
        assert_eq!(php_gcvt(123.456, 2, '.', 'E'), "1.2E+2");
    }

    // Each value here sits exactly halfway between the two shortest decimal strings that round
    // trip to it, so it pins down the tie break. Expected strings are the output of `serialize()`
    // under PHP 8.5.9 with serialize_precision=-1 (without the `d:`/`;` wrapper).
    #[test]
    #[allow(clippy::excessive_precision)]
    fn ties_round_to_even() {
        assert_eq!(gcvt(-166050639803968.125), "-166050639803968.12");
        assert_eq!(gcvt(-1093304034815995.25), "-1093304034815995.2");
        assert_eq!(gcvt(1958936080342852.25), "1958936080342852.2");
        assert_eq!(gcvt(1406236375946777.25), "1406236375946777.2");
        assert_eq!(gcvt(96483674649693.625), "96483674649693.62");
        assert_eq!(gcvt(1394865425023536.25), "1394865425023536.2");
        assert_eq!(gcvt(-167581363823776.125), "-167581363823776.12");
        assert_eq!(gcvt(1712200237658961.25), "1712200237658961.2");
        assert_eq!(gcvt(1918163363515546.25), "1918163363515546.2");
        assert_eq!(gcvt(2127524128142182.25), "2127524128142182.2");
        assert_eq!(gcvt(2067776186925270.25), "2067776186925270.2");
        assert_eq!(gcvt(-1.4074337013955528e179), "-1.4074337013955528E+179");
        assert_eq!(gcvt(-1.487618938859184e19), "-1.487618938859184E+19");
        assert_eq!(gcvt(-5.181233598032867e-173), "-5.181233598032867E-173");
        assert_eq!(gcvt(-1.1141679308961279e-114), "-1.1141679308961279E-114");
        assert_eq!(gcvt(-1.638344060600543e-227), "-1.638344060600543E-227");
        assert_eq!(gcvt(-1.2377569472211426e-164), "-1.2377569472211426E-164");
    }
}
