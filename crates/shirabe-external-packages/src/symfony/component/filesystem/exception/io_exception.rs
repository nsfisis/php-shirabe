#[derive(Debug)]
pub struct IOException {
    pub message: String,
    pub code: i64,
    pub path: Option<String>,
}

impl std::fmt::Display for IOException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for IOException {}
