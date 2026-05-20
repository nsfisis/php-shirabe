pub mod promise;
pub mod promise_interface;

pub use promise::*;
pub use promise_interface::*;

use shirabe_php_shim::PhpMixed;

pub fn resolve(_value: Option<PhpMixed>) -> Box<dyn PromiseInterface> {
    todo!()
}

pub fn all(_promises: Vec<Box<dyn PromiseInterface>>) -> Box<dyn PromiseInterface> {
    todo!()
}
