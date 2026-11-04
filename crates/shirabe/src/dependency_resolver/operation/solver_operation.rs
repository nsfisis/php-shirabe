//! ref: composer/src/Composer/DependencyResolver/Operation/SolverOperation.php sha1:6458da353f676173fb2fd98e8252fb2cf29f7fd2

use bstr::BStr;

/// PHP's abstract `SolverOperation` carries the per-class `TYPE` constant and the
/// `getOperationType()` / `__toString()` implementations shared by every operation. Only the
/// constant remains here; the shared implementations live on
/// [`AnyOperation`](crate::dependency_resolver::operation::AnyOperation).
pub trait SolverOperation {
    const TYPE: &'static BStr;
}
