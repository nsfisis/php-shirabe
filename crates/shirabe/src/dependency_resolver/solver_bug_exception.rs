//! ref: composer/src/Composer/DependencyResolver/SolverBugException.php sha1:258f0748dc9343317c51383d3aae87882147610f

use bstr::BString;
use shirabe_php_shim::{RuntimeException, bconcat};

#[derive(Debug)]
pub struct SolverBugException(pub RuntimeException);

impl SolverBugException {
    #[track_caller]
    pub fn new(message: impl Into<BString>) -> Self {
        let full_message = bconcat!(
            message.into(),
            "\nThis exception was most likely caused by a bug in Composer.\n\
            Please report the command you ran, the exact error you received, and your composer.json on https://github.com/nsfisis/php-shirabe/issues - thank you!\n",
        );
        SolverBugException(RuntimeException::new(full_message))
    }
}

shirabe_php_shim::impl_php_exception!(
    SolverBugException,
    0,
    r"Composer\DependencyResolver\SolverBugException"
);
