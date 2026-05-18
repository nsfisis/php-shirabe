pub mod promise;
pub mod promise_interface;

use self::promise_interface::PromiseInterface;
use shirabe_php_shim::PhpMixed;

pub fn resolve(_value: Option<PhpMixed>) -> Box<dyn PromiseInterface> {
    todo!()
}

pub fn all(_promises: Vec<Box<dyn PromiseInterface>>) -> Box<dyn PromiseInterface> {
    todo!()
}
