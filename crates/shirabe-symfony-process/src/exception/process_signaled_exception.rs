//! ref: composer/vendor/symfony/process/Exception/ProcessSignaledException.php

use crate::exception::RuntimeException;
use crate::process::Process;

#[derive(Debug)]
pub struct ProcessSignaledException {
    inner: RuntimeException,
    signal: i64,
}

impl ProcessSignaledException {
    pub fn new(process: &mut Process) -> anyhow::Result<Self> {
        let signal = process.get_term_signal()?;

        Ok(Self {
            inner: RuntimeException::new(format!(
                "The process has been signaled with signal \"{}\".",
                signal
            )),
            signal,
        })
    }

    pub fn get_signal(&self) -> i64 {
        self.signal
    }
}

shirabe_php_shim::impl_php_exception!(
    ProcessSignaledException,
    inner,
    r"Symfony\Component\Process\Exception\ProcessSignaledException"
);
