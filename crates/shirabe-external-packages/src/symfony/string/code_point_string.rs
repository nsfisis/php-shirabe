#[derive(Debug, Clone)]
pub struct CodePointString {
    pub(crate) string: String,
}

impl CodePointString {
    pub fn wordwrap(&self, _width: i64, _break: &str, _cut: bool) -> Self {
        todo!()
    }

    pub fn to_byte_string(&self, _encoding: &str) -> String {
        todo!()
    }
}
