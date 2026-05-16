use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct Validator;

impl Validator {
    pub fn new() -> Self {
        todo!()
    }

    pub fn check(&mut self, data: &PhpMixed, schema: &PhpMixed) -> anyhow::Result<()> {
        todo!()
    }

    pub fn is_valid(&self) -> bool {
        todo!()
    }

    pub fn get_errors(&self) -> Vec<String> {
        todo!()
    }
}
