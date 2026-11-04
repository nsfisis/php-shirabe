//! ref: composer/vendor/symfony/console/Input/StreamableInputInterface.php sha1:4536d5d88bf2d967b6a6194343fa093a02d4736e

use crate::input::InputInterface;
use shirabe_php_shim::PhpResource;

pub trait StreamableInputInterface: InputInterface {
    fn set_stream(&mut self, stream: PhpResource);

    fn get_stream(&self) -> Option<PhpResource>;
}
