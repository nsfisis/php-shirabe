use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct HelperSet;

impl HelperSet {
    pub fn new(helpers: Vec<PhpMixed>) -> Self {
        todo!()
    }

    pub fn get(&self, name: &str) -> Option<PhpMixed> {
        todo!()
    }

    pub fn set(&mut self, helper: PhpMixed, alias: Option<&str>) {
        todo!()
    }

    pub fn has(&self, name: &str) -> bool {
        todo!()
    }
}
