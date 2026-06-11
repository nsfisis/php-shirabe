use crate::symfony::console::input::input_interface::InputInterface;

pub trait InputAwareInterface {
    fn set_input(&mut self, input: Box<dyn InputInterface>);
}
