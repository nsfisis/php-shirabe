use crate::symfony::component::console::output::output_interface::OutputInterface;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct Table;

impl Table {
    pub fn new(_output: &dyn OutputInterface) -> Self {
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

    pub fn set_style(&mut self, _style: &str) -> &mut Self {
        todo!()
    }

    pub fn set_column_widths(&mut self, _widths: Vec<i64>) -> &mut Self {
        todo!()
    }
}
