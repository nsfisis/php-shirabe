pub fn max_i64(_a: i64, _b: i64) -> i64 {
    _a.max(_b)
}

pub fn max(_a: i64, _b: i64) -> i64 {
    _a.max(_b)
}

pub fn min(_a: i64, _b: i64) -> i64 {
    _a.min(_b)
}

pub fn abs(_value: i64) -> i64 {
    _value.abs()
}

pub fn floor(_value: f64) -> f64 {
    _value.floor()
}

pub fn ceil(v: f64) -> f64 {
    v.ceil()
}

pub fn round(_value: f64, _precision: i64) -> f64 {
    todo!()
}

pub fn intdiv(a: i64, b: i64) -> i64 {
    // PHP intdiv() throws DivisionByZeroError on a zero divisor and ArithmeticError
    // for PHP_INT_MIN / -1; Rust's `/` likewise panics in both cases.
    a / b
}
