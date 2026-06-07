#[derive(Debug)]
pub struct ProcessSignaledException {
    pub message: String,
    pub code: i64,
}

impl std::fmt::Display for ProcessSignaledException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ProcessSignaledException {}
