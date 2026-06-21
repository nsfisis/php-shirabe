pub fn round(v: f64, precision: i64) -> f64 {
    // PHP's default mode is PHP_ROUND_HALF_UP (round half away from zero),
    // which matches Rust's f64::round.
    let factor = 10f64.powi(precision as i32);
    (v * factor).round() / factor
}
