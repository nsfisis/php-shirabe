use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct SingleCommandApplication;

impl SingleCommandApplication {
    pub fn new() -> Self {
        todo!()
    }

    pub fn set_name(&mut self, name: &str) -> &mut Self {
        todo!()
    }

    pub fn set_version(&mut self, version: &str) -> &mut Self {
        todo!()
    }

    pub fn set_code(
        &mut self,
        code: Box<dyn Fn(&dyn std::any::Any, &dyn std::any::Any) -> i64>,
    ) -> &mut Self {
        todo!()
    }

    pub fn run(&mut self) -> anyhow::Result<i64> {
        todo!()
    }
}
