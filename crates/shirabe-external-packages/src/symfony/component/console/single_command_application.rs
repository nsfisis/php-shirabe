#[derive(Debug)]
pub struct SingleCommandApplication;

impl Default for SingleCommandApplication {
    fn default() -> Self {
        Self::new()
    }
}

impl SingleCommandApplication {
    pub fn new() -> Self {
        todo!()
    }

    pub fn set_name(&mut self, _name: &str) -> &mut Self {
        todo!()
    }

    pub fn set_version(&mut self, _version: &str) -> &mut Self {
        todo!()
    }

    pub fn set_code(
        &mut self,
        _code: Box<dyn Fn(&dyn std::any::Any, &dyn std::any::Any) -> i64>,
    ) -> &mut Self {
        todo!()
    }

    pub fn run(&mut self) -> anyhow::Result<i64> {
        todo!()
    }
}
