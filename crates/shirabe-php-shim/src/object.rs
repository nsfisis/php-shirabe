/// A property of a Rust object standing for a PHP object that the PHP worker can reach over RPC.
///
/// Plugins run in a separate PHP process and hold RPC handles to Rust objects. Whenever Rust code
/// calls into the worker, the PHP side may call back any method of any object it has a handle to,
/// including an object whose method is still running on the Rust side. Whether a given call can
/// reach the worker is not visible at the call site and is hard to rule out, so a property must not
/// stay borrowed across any call. This cell enforces that by only moving values in and out, never
/// handing out a borrow guard.
///
/// Store a PHP array as `Rc<IndexMap<..>>` / `Rc<Vec<..>>` and modify it with `Rc::make_mut` inside
/// [`PhpProp::update`]. This matches PHP's copy-on-write arrays: a `cloned()` snapshot keeps its
/// contents when the property is modified later, e.g. by a call back from the worker.
///
/// The closure passed to [`PhpProp::update`] is the one place where the property stays borrowed, so
/// it must only manipulate the value and must not call anything that can reach the PHP worker; a
/// call back into this property from there panics.
#[derive(Default)]
pub struct PhpProp<T>(std::cell::RefCell<T>);

impl<T> PhpProp<T> {
    pub fn new(value: T) -> Self {
        Self(std::cell::RefCell::new(value))
    }

    pub fn cloned(&self) -> T
    where
        T: Clone,
    {
        self.0.borrow().clone()
    }

    pub fn set(&self, value: T) {
        *self.0.borrow_mut() = value;
    }

    pub fn replace(&self, value: T) -> T {
        self.0.replace(value)
    }

    pub fn take(&self) -> T
    where
        T: Default,
    {
        self.0.take()
    }

    pub fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        f(&mut self.0.borrow_mut())
    }

    pub fn into_inner(self) -> T {
        self.0.into_inner()
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for PhpProp<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0.try_borrow() {
            Ok(value) => f.debug_tuple("PhpProp").field(&*value).finish(),
            Err(_) => f.write_str("PhpProp(<updating>)"),
        }
    }
}
