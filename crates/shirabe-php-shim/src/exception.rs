use crate::PhpClass;

/// The fields a PHP `\Throwable` carries: its message and code, and the exception it wraps. Ported
/// exception types embed this, either directly or through the parent exception they extend.
#[derive(Debug, Clone)]
pub struct ThrowableFields {
    message: String,
    code: i64,
    previous: Option<std::sync::Arc<AnyThrowable>>,
}

impl ThrowableFields {
    pub fn get_message(&self) -> &str {
        &self.message
    }

    pub fn get_code(&self) -> i64 {
        self.code
    }

    /// PHP's `code` property is protected with no setter; Composer writes it through reflection.
    pub fn set_code(&mut self, code: i64) {
        self.code = code;
    }

    pub fn get_previous(&self) -> Option<&AnyThrowable> {
        self.previous.as_deref()
    }
}

impl std::fmt::Display for ThrowableFields {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

crate::impl_php_class!(ThrowableFields, r"Throwable");

/// A ported PHP exception class, seen as the object PHP `throw`s: its [`ThrowableFields`], its
/// concrete Rust type, and the instance of its parent class it embeds.
/// [`impl_php_exception!`] implements this for every ported exception.
///
/// Every ported exception embeds an instance of the class it extends, so [`Self::parent`] walks
/// exactly PHP's chain of superclasses and bottoms out at the [`ThrowableFields`].
pub trait Throwable:
    PhpClass + std::fmt::Debug + std::fmt::Display + Send + Sync + 'static
{
    fn fields(&self) -> &ThrowableFields;
    fn as_any(&self) -> &(dyn std::any::Any + 'static);
    fn as_any_mut(&mut self) -> &mut (dyn std::any::Any + 'static);
    fn parent(&self) -> Option<&dyn Throwable>;
    fn parent_mut(&mut self) -> Option<&mut dyn Throwable>;
}

impl Throwable for ThrowableFields {
    fn fields(&self) -> &ThrowableFields {
        self
    }

    fn as_any(&self) -> &(dyn std::any::Any + 'static) {
        self
    }

    fn as_any_mut(&mut self) -> &mut (dyn std::any::Any + 'static) {
        self
    }

    fn parent(&self) -> Option<&dyn Throwable> {
        None
    }

    fn parent_mut(&mut self) -> Option<&mut dyn Throwable> {
        None
    }
}

/// The form a thrown PHP exception takes while it travels as a Rust error. Ported exception types
/// deliberately do not implement [`std::error::Error`], so this box is the only way one reaches an
/// `anyhow::Error`: an error either carries an `AnyThrowable`, and PHP would see a `\Throwable`,
/// or it does not, and PHP would see nothing catchable.
///
/// This is what makes `catch (\RuntimeException $e)` portable: [`Catch::catch`] answers over the
/// whole class hierarchy, rather than over the Rust type, which is a leaf of it.
#[derive(Debug)]
pub struct AnyThrowable(Box<dyn Throwable>);

impl AnyThrowable {
    pub fn new(exception: impl Throwable) -> Self {
        Self(Box::new(exception))
    }

    /// The exception a Rust error carries, or `None` if it carries none.
    // TODO(phase-c): this matches only an error that *is* the exception, where [`Catch`]'s
    // `anyhow::Error` impl also sees one behind an `anyhow::Context` layer. Nothing in the port
    // adds context to an error yet, so an exception wrapped that way would go silently unseen.
    pub fn of<'e>(error: &'e (dyn std::error::Error + 'static)) -> Option<&'e Self> {
        error.downcast_ref::<Self>()
    }

    /// PHP's `catch (T $e)`: the exception seen as an instance of `T`, or `None` if it is not one.
    /// A subclass answers through the instance of `T` it embeds, so `T`'s own state is reachable
    /// the way PHP reaches an inherited property.
    fn downcast_ref<T: Throwable>(&self) -> Option<&T> {
        let mut class: &dyn Throwable = &*self.0;
        loop {
            if let Some(instance) = class.as_any().downcast_ref::<T>() {
                return Some(instance);
            }
            class = class.parent()?;
        }
    }

    /// [`AnyThrowable::downcast_ref`] for a caught exception that is about to be mutated, the way
    /// PHP writes to a property of the object it caught.
    fn downcast_mut<T: Throwable>(&mut self) -> Option<&mut T> {
        let mut superclasses = 0;
        let mut class: &dyn Throwable = &*self.0;
        while !class.as_any().is::<T>() {
            class = class.parent()?;
            superclasses += 1;
        }

        let mut class: &mut dyn Throwable = &mut *self.0;
        for _ in 0..superclasses {
            class = class.parent_mut().expect("walked immutably just above");
        }
        class.as_any_mut().downcast_mut::<T>()
    }

    pub fn get_message(&self) -> &str {
        self.0.fields().get_message()
    }

    pub fn get_code(&self) -> i64 {
        self.0.fields().get_code()
    }

    pub fn get_previous(&self) -> Option<&AnyThrowable> {
        self.0.fields().get_previous()
    }
}

impl std::fmt::Display for AnyThrowable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl std::error::Error for AnyThrowable {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.get_previous()
            .map(|previous| previous as &(dyn std::error::Error + 'static))
    }
}

impl PhpClass for AnyThrowable {
    fn php_class_name(&self) -> String {
        self.0.php_class_name()
    }
}

/// PHP's `catch` applied to a Rust error.
pub trait Catch {
    /// The exception the error carries, seen as an instance of `T`, or `None` if it carries no
    /// exception or one of an unrelated class.
    fn catch<T: Throwable>(&self) -> Option<&T>;

    /// [`Catch::catch`] for a caught exception that is about to be mutated, the way PHP writes to
    /// a property of the object it caught.
    fn catch_mut<T: Throwable>(&mut self) -> Option<&mut T>;

    /// PHP's `$e instanceof T`.
    fn is_instanceof<T: Throwable>(&self) -> bool {
        self.catch::<T>().is_some()
    }

    /// PHP's `get_class($e) === T::class`: the class the exception was thrown as, rather than
    /// [`Catch::is_instanceof`]'s walk over its superclasses.
    fn is_class<T: Throwable>(&self) -> bool;
}

impl Catch for anyhow::Error {
    fn catch<T: Throwable>(&self) -> Option<&T> {
        self.downcast_ref::<AnyThrowable>()?.downcast_ref::<T>()
    }

    fn catch_mut<T: Throwable>(&mut self) -> Option<&mut T> {
        self.downcast_mut::<AnyThrowable>()?.downcast_mut::<T>()
    }

    fn is_class<T: Throwable>(&self) -> bool {
        self.downcast_ref::<AnyThrowable>()
            .is_some_and(|e| e.is_class::<T>())
    }
}

impl Catch for AnyThrowable {
    fn catch<T: Throwable>(&self) -> Option<&T> {
        self.downcast_ref::<T>()
    }

    fn catch_mut<T: Throwable>(&mut self) -> Option<&mut T> {
        self.downcast_mut::<T>()
    }

    fn is_class<T: Throwable>(&self) -> bool {
        self.0.as_any().is::<T>()
    }
}

/// Implements the `\Throwable` surface for a ported exception type, given the field holding the
/// fields it inherits — a [`ThrowableFields`] for a type that extends a PHP built-in directly, or
/// the embedded parent exception otherwise — and the fully-qualified name of the PHP class.
///
/// ```ignore
/// impl_php_exception!(SolverBugException, 0, r"Composer\DependencyResolver\SolverBugException");
/// ```
///
/// The type is deliberately left without a [`std::error::Error`] impl, so that the only route from
/// it to an `anyhow::Error` is the [`AnyThrowable`] this generates a conversion to.
#[macro_export]
macro_rules! impl_php_exception {
    ($ty:ty, $field:tt, $class_name:expr) => {
        $crate::impl_php_exception!(@accessors $ty, $field, $class_name);

        impl $crate::Throwable for $ty {
            fn fields(&self) -> &$crate::ThrowableFields {
                $crate::Throwable::fields(&self.$field)
            }

            fn as_any(&self) -> &(dyn std::any::Any + 'static) {
                self
            }

            fn as_any_mut(&mut self) -> &mut (dyn std::any::Any + 'static) {
                self
            }

            fn parent(&self) -> Option<&dyn $crate::Throwable> {
                Some(&self.$field)
            }

            fn parent_mut(&mut self) -> Option<&mut dyn $crate::Throwable> {
                Some(&mut self.$field)
            }
        }

        impl From<$ty> for $crate::AnyThrowable {
            fn from(exception: $ty) -> Self {
                $crate::AnyThrowable::new(exception)
            }
        }

        impl From<$ty> for ::anyhow::Error {
            fn from(exception: $ty) -> Self {
                ::anyhow::Error::new($crate::AnyThrowable::new(exception))
            }
        }
    };
    // For an exception the port cannot let travel as a Rust error, because its state is not
    // `Send + Sync`. It gets the accessors but no [`Throwable`], so asking for it in a `catch`
    // does not compile, rather than silently never matching.
    ($ty:ty, $field:tt, $class_name:expr, !Send) => {
        $crate::impl_php_exception!(@accessors $ty, $field, $class_name);
    };
    (@accessors $ty:ty, $field:tt, $class_name:expr) => {
        impl $ty {
            pub fn get_message(&self) -> &str {
                self.$field.get_message()
            }

            pub fn get_code(&self) -> i64 {
                self.$field.get_code()
            }

            pub fn set_code(&mut self, code: i64) {
                self.$field.set_code(code);
            }

            pub fn get_previous(&self) -> Option<&$crate::AnyThrowable> {
                self.$field.get_previous()
            }
        }

        impl std::fmt::Display for $ty {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::Display::fmt(&self.$field, f)
            }
        }

        impl $crate::PhpClass for $ty {
            fn php_class_name(&self) -> String {
                $class_name.to_string()
            }
        }
    };
}

/// Defines a PHP built-in exception class as a struct carrying nothing but the instance of the
/// class it extends, or the [`ThrowableFields`] itself for a class that extends nothing.
macro_rules! define_php_exception {
    ($ty:ident, ThrowableFields, $class_name:expr) => {
        define_php_exception!(@shared $ty, ThrowableFields, $class_name);

        impl $ty {
            pub fn with_code_and_previous(
                message: String,
                code: i64,
                previous: Option<std::sync::Arc<AnyThrowable>>,
            ) -> Self {
                Self {
                    inner: ThrowableFields {
                        message,
                        code,
                        previous,
                    },
                }
            }
        }
    };
    ($ty:ident, $parent:ty, $class_name:expr) => {
        define_php_exception!(@shared $ty, $parent, $class_name);

        impl $ty {
            pub fn with_code_and_previous(
                message: String,
                code: i64,
                previous: Option<std::sync::Arc<AnyThrowable>>,
            ) -> Self {
                Self {
                    inner: <$parent>::with_code_and_previous(message, code, previous),
                }
            }
        }
    };
    (@shared $ty:ident, $parent:ty, $class_name:expr) => {
        #[derive(Debug, Clone)]
        pub struct $ty {
            inner: $parent,
        }

        impl $ty {
            pub fn new(message: String) -> Self {
                Self::with_code_and_previous(message, 0, None)
            }

            pub fn with_code(message: String, code: i64) -> Self {
                Self::with_code_and_previous(message, code, None)
            }
        }

        crate::impl_php_exception!($ty, inner, $class_name);
    };
}

define_php_exception!(Exception, ThrowableFields, r"Exception");
define_php_exception!(Error, ThrowableFields, r"Error");
define_php_exception!(TypeError, Error, r"TypeError");
define_php_exception!(RuntimeException, Exception, r"RuntimeException");
define_php_exception!(
    UnexpectedValueException,
    RuntimeException,
    r"UnexpectedValueException"
);
define_php_exception!(
    OutOfBoundsException,
    RuntimeException,
    r"OutOfBoundsException"
);
define_php_exception!(LogicException, Exception, r"LogicException");
define_php_exception!(
    InvalidArgumentException,
    LogicException,
    r"InvalidArgumentException"
);
define_php_exception!(
    BadFunctionCallException,
    LogicException,
    r"BadFunctionCallException"
);
define_php_exception!(
    BadMethodCallException,
    BadFunctionCallException,
    r"BadMethodCallException"
);

#[derive(Debug)]
pub struct ErrorException {
    inner: Exception,
    pub severity: i64,
    pub filename: String,
    pub lineno: i64,
}

impl ErrorException {
    pub fn new(
        message: String,
        code: i64,
        severity: i64,
        filename: String,
        lineno: i64,
        previous: Option<std::sync::Arc<AnyThrowable>>,
    ) -> Self {
        Self {
            inner: Exception::with_code_and_previous(message, code, previous),
            severity,
            filename,
            lineno,
        }
    }
}

crate::impl_php_exception!(ErrorException, inner, r"ErrorException");

/// Models PHP's `exit`/`die` language construct propagated as a recoverable error so the actual
/// process termination happens at a single top-level site instead of deep in the call stack.
///
/// Like PHP's `exit`, this must NOT be caught by ported `try`/`catch` blocks: any broad catch on
/// the propagation path has to re-raise it untouched, and only the outermost handler converts it
/// into the process exit code.
#[derive(Debug)]
pub struct ExitException {
    pub code: i64,
}

impl std::fmt::Display for ExitException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "exit({})", self.code)
    }
}

impl std::error::Error for ExitException {}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Subclass {
        inner: UnexpectedValueException,
        detail: i64,
    }

    impl Subclass {
        fn new(detail: i64) -> Self {
            Self {
                inner: UnexpectedValueException::new("boom".to_string()),
                detail,
            }
        }
    }

    crate::impl_php_exception!(Subclass, inner, r"Vendor\Subclass");

    #[test]
    fn catch_reaches_every_superclass() {
        let error: anyhow::Error = Subclass::new(7).into();

        assert_eq!(error.catch::<Subclass>().map(|e| e.detail), Some(7));
        assert!(error.catch::<UnexpectedValueException>().is_some());
        assert!(error.catch::<RuntimeException>().is_some());
        assert!(error.catch::<Exception>().is_some());
        assert!(error.catch::<ThrowableFields>().is_some());
    }

    #[test]
    fn catch_reaches_no_sibling_or_subclass() {
        let error: anyhow::Error = RuntimeException::new("boom".to_string()).into();

        assert!(error.catch::<Subclass>().is_none());
        assert!(error.catch::<LogicException>().is_none());
        assert!(error.catch::<Error>().is_none());
    }

    #[test]
    fn an_error_is_not_an_exception() {
        let error: anyhow::Error = TypeError::new("boom".to_string()).into();

        assert!(error.catch::<Error>().is_some());
        assert!(error.catch::<ThrowableFields>().is_some());
        assert!(error.catch::<Exception>().is_none());
    }

    #[test]
    fn catch_reaches_nothing_in_a_plain_rust_error() {
        let error = anyhow::anyhow!("boom");

        assert!(error.catch::<ThrowableFields>().is_none());
    }

    #[test]
    fn is_class_reaches_no_superclass() {
        let error: anyhow::Error = Subclass::new(7).into();

        assert!(error.is_class::<Subclass>());
        assert!(error.is_instanceof::<UnexpectedValueException>());
        assert!(!error.is_class::<UnexpectedValueException>());
        assert!(!error.is_class::<ThrowableFields>());
    }

    #[test]
    fn is_class_reaches_nothing_in_a_plain_rust_error() {
        let error = anyhow::anyhow!("boom");

        assert!(!error.is_class::<ThrowableFields>());
    }

    #[test]
    fn catch_mut_writes_through_to_the_superclass_state() {
        let mut error: anyhow::Error = Subclass::new(7).into();

        error
            .catch_mut::<UnexpectedValueException>()
            .unwrap()
            .set_code(42);

        assert_eq!(error.catch::<Subclass>().unwrap().get_code(), 42);
    }

    #[test]
    fn the_previous_exception_is_the_error_source() {
        let previous = std::sync::Arc::new(AnyThrowable::new(RuntimeException::new(
            "cause".to_string(),
        )));
        let error: anyhow::Error =
            Exception::with_code_and_previous("boom".to_string(), 0, Some(previous)).into();

        let source = std::error::Error::source(
            error.downcast_ref::<AnyThrowable>().unwrap() as &dyn std::error::Error
        );
        assert_eq!(source.map(ToString::to_string), Some("cause".to_string()));
    }
}
