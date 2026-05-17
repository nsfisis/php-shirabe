#[derive(Debug)]
pub struct ExecutableFinder;

impl Default for ExecutableFinder {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutableFinder {
    pub fn new() -> Self {
        todo!()
    }

    pub fn add_suffix(&mut self, _suffix: &str) {
        todo!()
    }

    pub fn find(&self, _name: &str, _default: Option<&str>, _dirs: &[String]) -> Option<String> {
        todo!()
    }
}
