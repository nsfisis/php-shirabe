#[derive(Debug)]
pub struct DuplicateKeyException {
    pub message: String,
    pub code: i64,
}

impl std::fmt::Display for DuplicateKeyException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for DuplicateKeyException {}
