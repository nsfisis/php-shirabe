use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct Promise;

impl Promise {
    pub fn new(resolver: Box<dyn FnOnce(Box<dyn FnOnce(Option<PhpMixed>)>, Box<dyn FnOnce(Option<PhpMixed>)>)>) -> Self {
        todo!()
    }

    pub fn then<F, G>(self, on_fulfilled: Option<F>, on_rejected: Option<G>) -> Self
    where
        F: FnOnce(Option<PhpMixed>) -> Option<PhpMixed>,
        G: FnOnce(Option<PhpMixed>) -> Option<PhpMixed>,
    {
        todo!()
    }
}
