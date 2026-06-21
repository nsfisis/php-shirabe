pub fn max(a: i64, b: i64) -> i64 {
    a.max(b)
}

pub fn min(a: i64, b: i64) -> i64 {
    a.min(b)
}

pub fn abs(v: i64) -> i64 {
    v.abs()
}

pub fn floor(v: f64) -> f64 {
    v.floor()
}

pub fn ceil(v: f64) -> f64 {
    v.ceil()
}

pub fn round(v: f64, precision: i64) -> f64 {
    // PHP's default mode is PHP_ROUND_HALF_UP (round half away from zero),
    // which matches Rust's f64::round.
    let factor = 10f64.powi(precision as i32);
    (v * factor).round() / factor
}

pub fn intdiv(a: i64, b: i64) -> i64 {
    a / b
}
