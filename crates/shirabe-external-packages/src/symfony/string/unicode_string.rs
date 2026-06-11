#[derive(Debug, Clone)]
pub struct UnicodeString {
    pub(crate) string: String,
}

impl UnicodeString {
    pub fn new(_string: &str) -> Self {
        todo!()
    }

    pub fn width(&self, _ignore_unsupported_encoding: bool) -> i64 {
        todo!()
    }

    pub fn length(&self) -> i64 {
        todo!()
    }

    pub fn slice(&self, _start: i64, _length: Option<i64>) -> Self {
        todo!()
    }
}
