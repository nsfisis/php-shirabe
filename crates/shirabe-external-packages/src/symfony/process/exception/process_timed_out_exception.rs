//! ref: composer/vendor/symfony/process/Exception/ProcessTimedOutException.php

use crate::symfony::process::process::Process;

#[derive(Debug)]
pub struct ProcessTimedOutException {
    pub message: String,
    pub code: i64,
}

impl ProcessTimedOutException {
    pub fn new(process: &Process) -> Self {
        let exceeded_timeout = process.get_timeout();

        let message = format!(
            "The process \"{}\" exceeded the timeout of {} seconds.",
            process.get_command_line(),
            exceeded_timeout.map(|t| t.to_string()).unwrap_or_default(),
        );

        Self { message, code: 0 }
    }
}

impl std::fmt::Display for ProcessTimedOutException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ProcessTimedOutException {}
