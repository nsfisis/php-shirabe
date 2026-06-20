//! ref: composer/vendor/symfony/process/Exception/ProcessTimedOutException.php

use crate::symfony::process::process::Process;

#[derive(Debug)]
pub struct ProcessTimedOutException {
    pub message: String,
    pub code: i64,
    timeout_type: i64,
    exceeded_timeout: Option<f64>,
}

impl ProcessTimedOutException {
    pub const TYPE_GENERAL: i64 = 1;
    pub const TYPE_IDLE: i64 = 2;

    pub fn new(process: &Process, timeout_type: i64) -> Self {
        let exceeded_timeout = match timeout_type {
            Self::TYPE_GENERAL => process.get_timeout(),
            Self::TYPE_IDLE => process.get_idle_timeout(),
            _ => panic!("Unknown timeout type \"{}\".", timeout_type),
        };

        let message = format!(
            "The process \"{}\" exceeded the timeout of {} seconds.",
            process.get_command_line(),
            exceeded_timeout.map(|t| t.to_string()).unwrap_or_default(),
        );

        Self {
            message,
            code: 0,
            timeout_type,
            exceeded_timeout,
        }
    }

    pub fn is_general_timeout(&self) -> bool {
        Self::TYPE_GENERAL == self.timeout_type
    }

    pub fn is_idle_timeout(&self) -> bool {
        Self::TYPE_IDLE == self.timeout_type
    }

    pub fn get_exceeded_timeout(&self) -> Option<f64> {
        self.exceeded_timeout
    }
}

impl std::fmt::Display for ProcessTimedOutException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ProcessTimedOutException {}
