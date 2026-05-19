use shirabe_php_shim::PhpMixed;

pub trait PromiseInterface {
    fn then(
        &self,
        on_fulfilled: Option<Box<dyn FnOnce(Option<PhpMixed>) -> Option<PhpMixed>>>,
        on_rejected: Option<Box<dyn FnOnce(Option<PhpMixed>) -> Option<PhpMixed>>>,
    ) -> Box<dyn PromiseInterface>;

    /// PHP closure shape: `fn(mixed): mixed`. Convenience wrapper around `then`
    /// for call sites that don't deal in `Option<PhpMixed>`.
    fn then_with(
        &self,
        _on_fulfilled: Option<Box<dyn FnOnce(PhpMixed) -> PhpMixed>>,
        _on_rejected: Option<Box<dyn FnOnce(PhpMixed) -> PhpMixed>>,
    ) -> Box<dyn PromiseInterface> {
        todo!()
    }

    /// PHP closure shape: `fn(mixed): mixed`, returning a `Box<dyn PromiseInterface>`.
    fn then_boxed(
        &self,
        _on_fulfilled: Option<Box<dyn FnOnce(PhpMixed) -> Box<dyn PromiseInterface>>>,
        _on_rejected: Option<Box<dyn FnOnce(PhpMixed) -> Box<dyn PromiseInterface>>>,
    ) -> Box<dyn PromiseInterface> {
        todo!()
    }

    /// Variant used by composer_repository where accept returns PhpMixed but reject returns
    /// an error/PhpMixed and the result is the same Promise pipeline.
    fn then_with_reject_boxed(
        &self,
        _on_fulfilled: Box<dyn FnOnce(PhpMixed) -> anyhow::Result<PhpMixed>>,
        _on_rejected: Box<dyn FnOnce(anyhow::Error) -> anyhow::Result<PhpMixed>>,
    ) -> Box<dyn PromiseInterface> {
        todo!()
    }
}
