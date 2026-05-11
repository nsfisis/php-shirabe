#[derive(Debug)]
pub struct UnexpectedValueException {
    pub message: String,
    pub code: i64,
}
