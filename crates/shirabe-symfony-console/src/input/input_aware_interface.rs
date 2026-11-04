//! ref: composer/vendor/symfony/console/Input/InputAwareInterface.php sha1:84d22388e0ff2dabfb91d0795b8d6557743ac7b6

use crate::input::InputInterface;

pub trait InputAwareInterface {
    fn set_input(&mut self, input: Box<dyn InputInterface>);
}
