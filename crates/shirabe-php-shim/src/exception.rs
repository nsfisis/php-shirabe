use crate::PharException;

#[derive(Debug)]
pub struct Exception {
    pub message: String,
    pub code: i64,
}

impl std::fmt::Display for Exception {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Exception {}

#[derive(Debug)]
pub struct RuntimeException {
    pub message: String,
    pub code: i64,
}

impl std::fmt::Display for RuntimeException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RuntimeException {}

#[derive(Debug)]
pub struct UnexpectedValueException {
    pub message: String,
    pub code: i64,
}

impl std::fmt::Display for UnexpectedValueException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for UnexpectedValueException {}

#[derive(Debug)]
pub struct InvalidArgumentException {
    pub message: String,
    pub code: i64,
}

impl std::fmt::Display for InvalidArgumentException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for InvalidArgumentException {}

#[derive(Debug)]
pub struct TypeError {
    pub message: String,
    pub code: i64,
}

impl std::fmt::Display for TypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for TypeError {}

#[derive(Debug)]
pub struct LogicException {
    pub message: String,
    pub code: i64,
}

impl std::fmt::Display for LogicException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for LogicException {}

#[derive(Debug)]
pub struct BadMethodCallException {
    pub message: String,
    pub code: i64,
}

impl std::fmt::Display for BadMethodCallException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for BadMethodCallException {}

#[derive(Debug)]
pub struct OutOfBoundsException {
    pub message: String,
    pub code: i64,
}

impl std::fmt::Display for OutOfBoundsException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for OutOfBoundsException {}

#[derive(Debug)]
pub struct ErrorException {
    pub message: String,
    pub code: i64,
    pub severity: i64,
    pub filename: String,
    pub lineno: i64,
}

impl std::fmt::Display for ErrorException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ErrorException {}

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

pub fn php_exception_get_code(_error: &anyhow::Error) -> i32 {
    // PHP's Throwable::getCode(). anyhow::Error carries the concrete exception type, so enumerate
    // the flat standard exception structs and read their `code` field; everything else defaults to
    // 0, matching PHP's default exception code.
    if let Some(e) = _error.downcast_ref::<Exception>() {
        return e.code as i32;
    }
    if let Some(e) = _error.downcast_ref::<RuntimeException>() {
        return e.code as i32;
    }
    if let Some(e) = _error.downcast_ref::<UnexpectedValueException>() {
        return e.code as i32;
    }
    if let Some(e) = _error.downcast_ref::<InvalidArgumentException>() {
        return e.code as i32;
    }
    if let Some(e) = _error.downcast_ref::<TypeError>() {
        return e.code as i32;
    }
    if let Some(e) = _error.downcast_ref::<LogicException>() {
        return e.code as i32;
    }
    if let Some(e) = _error.downcast_ref::<BadMethodCallException>() {
        return e.code as i32;
    }
    if let Some(e) = _error.downcast_ref::<OutOfBoundsException>() {
        return e.code as i32;
    }
    if let Some(e) = _error.downcast_ref::<ErrorException>() {
        return e.code as i32;
    }
    if let Some(e) = _error.downcast_ref::<PharException>() {
        return e.code as i32;
    }
    0
}
