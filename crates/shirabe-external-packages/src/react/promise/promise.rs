use shirabe_php_shim::PhpMixed;

use super::PromiseInterface;

#[derive(Debug)]
pub struct Promise;

impl Promise {
    pub fn new(
        _resolver: Box<
            dyn FnOnce(Box<dyn FnOnce(Option<PhpMixed>)>, Box<dyn FnOnce(Option<PhpMixed>)>),
        >,
    ) -> Self {
        todo!()
    }
}

impl PromiseInterface for Promise {
    fn then(
        &self,
        _on_fulfilled: Option<Box<dyn FnOnce(Option<PhpMixed>) -> Option<PhpMixed>>>,
        _on_rejected: Option<Box<dyn FnOnce(Option<PhpMixed>) -> Option<PhpMixed>>>,
    ) -> Box<dyn PromiseInterface> {
        todo!()
    }
}
