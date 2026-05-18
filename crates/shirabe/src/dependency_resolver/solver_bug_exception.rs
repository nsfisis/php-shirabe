//! ref: composer/src/Composer/DependencyResolver/SolverBugException.php

use shirabe_php_shim::RuntimeException;

#[derive(Debug)]
pub struct SolverBugException(pub RuntimeException);

impl SolverBugException {
    pub fn new(message: String) -> Self {
        let full_message = format!(
            "{}\nThis exception was most likely caused by a bug in Composer.\n\
            Please report the command you ran, the exact error you received, and your composer.json on https://github.com/composer/composer/issues - thank you!\n",
            message
        );
        SolverBugException(RuntimeException {
            message: full_message,
            code: 0,
        })
    }
}

impl std::fmt::Display for SolverBugException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for SolverBugException {}
