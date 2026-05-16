use crate::symfony::component::console::output::output_interface::OutputInterface;

#[derive(Debug)]
pub struct ProgressBar;

impl ProgressBar {
    pub fn new(output: &dyn OutputInterface, max: i64) -> Self {
        todo!()
    }

    pub fn start(&mut self, max: Option<i64>) {
        todo!()
    }

    pub fn advance(&mut self, step: i64) {
        todo!()
    }

    pub fn finish(&mut self) {
        todo!()
    }

    pub fn set_format(&mut self, format: &str) {
        todo!()
    }

    pub fn get_progress(&self) -> i64 {
        todo!()
    }

    pub fn get_max_steps(&self) -> i64 {
        todo!()
    }
}
