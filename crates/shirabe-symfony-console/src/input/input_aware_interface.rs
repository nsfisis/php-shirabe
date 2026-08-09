//! ref: composer/vendor/symfony/console/Input/InputAwareInterface.php

use crate::input::InputInterface;

pub trait InputAwareInterface {
    fn set_input(&mut self, input: Box<dyn InputInterface>);
}
