//! ref: composer/vendor/symfony/process/Exception/ProcessSignaledException.php

use crate::symfony::process::process::Process;

#[derive(Debug)]
pub struct ProcessSignaledException {
    pub message: String,
    pub code: i64,
    signal: i64,
}

impl ProcessSignaledException {
    pub fn new(process: &mut Process) -> anyhow::Result<Self> {
        let signal = process.get_term_signal()?;

        Ok(Self {
            message: format!("The process has been signaled with signal \"{}\".", signal),
            code: 0,
            signal,
        })
    }

    pub fn get_signal(&self) -> i64 {
        self.signal
    }
}

impl std::fmt::Display for ProcessSignaledException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ProcessSignaledException {}
