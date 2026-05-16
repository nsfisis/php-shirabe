pub mod promise;
pub mod promise_interface;

use self::promise::Promise;
use shirabe_php_shim::PhpMixed;

pub fn resolve(value: Option<PhpMixed>) -> Promise {
    todo!()
}

pub fn all(promises: Vec<Promise>) -> Promise {
    todo!()
}
