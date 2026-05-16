use crate::symfony::console::input::input_interface::InputInterface;
use shirabe_php_shim::PhpMixed;

pub trait StreamableInputInterface: InputInterface {
    fn set_stream(&mut self, stream: PhpMixed);
    fn get_stream(&self) -> Option<PhpMixed>;
}
