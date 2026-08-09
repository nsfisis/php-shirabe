//! ref: composer/vendor/symfony/process/Exception/ProcessTimedOutException.php

use crate::exception::runtime_exception::RuntimeException;
use crate::process::Process;

#[derive(Debug)]
pub struct ProcessTimedOutException {
    inner: RuntimeException,
}

impl ProcessTimedOutException {
    pub fn new(process: &Process) -> Self {
        let exceeded_timeout = process.get_timeout();

        let message = format!(
            "The process \"{}\" exceeded the timeout of {} seconds.",
            process.get_command_line(),
            exceeded_timeout.map(|t| t.to_string()).unwrap_or_default(),
        );

        Self {
            inner: RuntimeException::new(message),
        }
    }
}

shirabe_php_shim::impl_php_exception!(
    ProcessTimedOutException,
    inner,
    r"Symfony\Component\Process\Exception\ProcessTimedOutException"
);
