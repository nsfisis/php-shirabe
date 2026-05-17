use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct Table;

impl Table {
    pub fn new(_output: &dyn std::any::Any) -> Self {
        todo!()
    }

    pub fn set_headers(&mut self, _headers: Vec<PhpMixed>) -> &mut Self {
        todo!()
    }

    pub fn set_rows(&mut self, _rows: Vec<PhpMixed>) -> &mut Self {
        todo!()
    }

    pub fn add_row(&mut self, _row: PhpMixed) -> &mut Self {
        todo!()
    }

    pub fn render(&mut self) {
        todo!()
    }
}
