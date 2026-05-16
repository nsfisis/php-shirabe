#[derive(Debug)]
pub struct InvalidArgumentException {
    pub message: String,
    pub code: i64,
}

impl std::fmt::Display for InvalidArgumentException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for InvalidArgumentException {}
