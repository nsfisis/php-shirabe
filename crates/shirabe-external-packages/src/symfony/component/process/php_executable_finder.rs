#[derive(Debug)]
pub struct PhpExecutableFinder;

impl Default for PhpExecutableFinder {
    fn default() -> Self {
        Self::new()
    }
}

impl PhpExecutableFinder {
    pub fn new() -> Self {
        todo!()
    }

    pub fn find(&self, _include_args: bool) -> Option<String> {
        todo!()
    }

    pub fn find_arguments(&self) -> Vec<String> {
        todo!()
    }
}
