//! ref: composer/vendor/symfony/process/Exception/ProcessFailedException.php

use crate::symfony::process::process::Process;

#[derive(Debug)]
pub struct ProcessFailedException {
    pub message: String,
    pub code: i64,
}

impl ProcessFailedException {
    pub fn new(_process: &Process) -> Self {
        todo!()
    }
}

impl std::fmt::Display for ProcessFailedException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ProcessFailedException {}
