#[derive(Debug)]
pub struct RuntimeException {
    pub message: String,
    pub code: i64,
}

#[derive(Debug)]
pub struct UnexpectedValueException {
    pub message: String,
    pub code: i64,
}
