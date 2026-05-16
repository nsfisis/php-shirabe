use crate::symfony::component::console::output::output_interface::OutputInterface;

pub trait ConsoleOutputInterface: OutputInterface {
    fn get_error_output(&self) -> &dyn OutputInterface;
    fn set_error_output(&mut self, error: Box<dyn OutputInterface>);
}
