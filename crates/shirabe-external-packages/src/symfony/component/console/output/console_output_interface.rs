use crate::symfony::component::console::output::OutputInterface;

pub trait ConsoleOutputInterface: OutputInterface {
    fn get_error_output(&self) -> &dyn OutputInterface;
    fn set_error_output(&mut self, error: Box<dyn OutputInterface>);
}
