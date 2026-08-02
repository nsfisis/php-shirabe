use crate::main::snprintf::php_gcvt;

/// php-src: Zend/zend_smart_str.c `smart_str_append_double` (PHP 8.5.8), folding in the `%H`
/// NAN/INF handling from main/snprintf.c `format_converter` that the original reaches through
/// `snprintf(buf, sizeof(buf), "%.*H", precision, num)`.
pub fn smart_str_append_double(dest: &mut String, num: f64, precision: i32, zero_fraction: bool) {
    if num.is_nan() {
        dest.push_str("NAN");
        return;
    }
    if num.is_infinite() {
        dest.push_str(if num > 0.0 { "INF" } else { "-INF" });
        return;
    }
    let buf = php_gcvt(num, precision, '.', 'E');
    let had_period = buf.contains('.');
    dest.push_str(&buf);
    if zero_fraction && !had_period {
        dest.push_str(".0");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn serialize_repr(num: f64) -> String {
        let mut out = String::new();
        smart_str_append_double(&mut out, num, -1, false);
        out
    }

    // Expected strings are the output of `serialize()` under PHP 8.5.8 with
    // serialize_precision=-1 (without the `d:`/`;` wrapper).
    #[test]
    fn matches_php_serialize_output() {
        assert_eq!(serialize_repr(0.0), "0");
        assert_eq!(serialize_repr(-0.0), "-0");
        assert_eq!(serialize_repr(1.5), "1.5");
        assert_eq!(serialize_repr(0.1), "0.1");
        assert_eq!(serialize_repr(2.0), "2");
        assert_eq!(serialize_repr(-2.0), "-2");
        assert_eq!(serialize_repr(100.0), "100");
        assert_eq!(serialize_repr(1e15), "1000000000000000");
        assert_eq!(serialize_repr(1e16), "10000000000000000");
        assert_eq!(serialize_repr(1e17), "1.0E+17");
        assert_eq!(serialize_repr(1e18), "1.0E+18");
        assert_eq!(serialize_repr(1e20), "1.0E+20");
        assert_eq!(serialize_repr(1.5e20), "1.5E+20");
        assert_eq!(serialize_repr(1e-4), "0.0001");
        assert_eq!(serialize_repr(1e-5), "1.0E-5");
        assert_eq!(serialize_repr(12345.6789e-9), "1.23456789E-5");
        assert_eq!(serialize_repr(1e-300), "1.0E-300");
        assert_eq!(serialize_repr(f64::MAX), "1.7976931348623157E+308");
        assert_eq!(serialize_repr(5e-324), "5.0E-324");
        assert_eq!(serialize_repr(1.0 / 3.0), "0.3333333333333333");
        assert_eq!(serialize_repr(0.30000000000000004), "0.30000000000000004");
        assert_eq!(serialize_repr(f64::NAN), "NAN");
        assert_eq!(serialize_repr(f64::INFINITY), "INF");
        assert_eq!(serialize_repr(f64::NEG_INFINITY), "-INF");
    }

    #[test]
    fn zero_fraction_appends_dot_zero_to_integral_values() {
        let mut out = String::new();
        smart_str_append_double(&mut out, 2.0, -1, true);
        assert_eq!(out, "2.0");

        let mut out = String::new();
        smart_str_append_double(&mut out, 1e17, -1, true);
        assert_eq!(out, "1.0E+17");
    }
}
