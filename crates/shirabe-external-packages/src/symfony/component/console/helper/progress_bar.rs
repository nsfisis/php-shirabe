use crate::symfony::component::console::output::output_interface::OutputInterface;

#[derive(Debug)]
pub struct ProgressBar;

impl ProgressBar {
    pub fn new(_output: &dyn OutputInterface, _max: i64) -> Self {
        todo!()
    }

    pub fn start(&mut self, _max: Option<i64>) {
        todo!()
    }

    pub fn advance(&mut self, _step: i64) {
        todo!()
    }

    pub fn finish(&mut self) {
        todo!()
    }

    pub fn set_format(&mut self, _format: &str) {
        todo!()
    }

    pub fn get_progress(&self) -> i64 {
        todo!()
    }

    pub fn get_max_steps(&self) -> i64 {
        todo!()
    }
}
