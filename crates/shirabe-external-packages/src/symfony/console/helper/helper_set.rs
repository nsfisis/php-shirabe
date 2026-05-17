use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct HelperSet;

impl HelperSet {
    pub fn new(_helpers: Vec<PhpMixed>) -> Self {
        todo!()
    }

    pub fn get(&self, _name: &str) -> Option<PhpMixed> {
        todo!()
    }

    pub fn set(&mut self, _helper: PhpMixed, _alias: Option<&str>) {
        todo!()
    }

    pub fn has(&self, _name: &str) -> bool {
        todo!()
    }
}
