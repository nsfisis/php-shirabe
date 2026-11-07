//! ref: composer/vendor/symfony/process/Exception/ProcessSignaledException.php sha1:f944ca4505a51a9fbc5e4435947bd917ef3d42c6

use crate::Process;
use crate::exception::RuntimeException;
use shirabe_php_shim::{bconcat, impl_php_exception};

#[derive(Debug)]
pub struct ProcessSignaledException {
    inner: RuntimeException,
    signal: i64,
}

impl ProcessSignaledException {
    #[track_caller]
    pub fn new(process: &mut Process) -> anyhow::Result<Self> {
        let signal = process.get_term_signal()?;

        Ok(Self {
            inner: RuntimeException::new(bconcat!(
                "The process has been signaled with signal \"",
                signal,
                "\"."
            )),
            signal,
        })
    }

    pub fn get_signal(&self) -> i64 {
        self.signal
    }
}

impl_php_exception!(
    ProcessSignaledException,
    inner,
    r"Symfony\Component\Process\Exception\ProcessSignaledException"
);
