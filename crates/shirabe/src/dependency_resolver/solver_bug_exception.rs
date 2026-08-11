//! ref: composer/src/Composer/DependencyResolver/SolverBugException.php

use shirabe_php_shim::RuntimeException;

#[derive(Debug)]
pub struct SolverBugException(pub RuntimeException);

impl SolverBugException {
    pub fn new(message: String) -> Self {
        let full_message = format!(
            "{}\nThis exception was most likely caused by a bug in Composer.\n\
            Please report the command you ran, the exact error you received, and your composer.json on https://github.com/nsfisis/php-shirabe/issues - thank you!\n",
            message
        );
        SolverBugException(RuntimeException::new(full_message))
    }
}

shirabe_php_shim::impl_php_exception!(
    SolverBugException,
    0,
    r"Composer\DependencyResolver\SolverBugException"
);
