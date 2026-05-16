pub mod promise;
pub mod promise_interface;

use shirabe_php_shim::PhpMixed;
use self::promise::Promise;

pub fn resolve(value: Option<PhpMixed>) -> Promise {
    todo!()
}

pub fn all(promises: Vec<Promise>) -> Promise {
    todo!()
}
