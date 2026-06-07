use crate::symfony::console::output::OutputInterface;

pub trait ConsoleOutputInterface: OutputInterface {
    fn get_error_output(&self) -> std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>;
    fn set_error_output(&mut self, error: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>);
}
