use shirabe_php_shim::PhpMixed;

pub trait PromiseInterface {
    fn then(
        &self,
        on_fulfilled: Option<Box<dyn FnOnce(Option<PhpMixed>) -> Option<PhpMixed>>>,
        on_rejected: Option<Box<dyn FnOnce(Option<PhpMixed>) -> Option<PhpMixed>>>,
    ) -> Box<dyn PromiseInterface>;
}
