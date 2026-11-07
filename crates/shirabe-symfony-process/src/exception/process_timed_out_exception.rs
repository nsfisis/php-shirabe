//! ref: composer/vendor/symfony/process/Exception/ProcessTimedOutException.php sha1:7bed6e96f41a9c813a9f71419e703905ae3d4872

use crate::Process;
use crate::exception::RuntimeException;
use shirabe_php_shim::{PhpMixed, ToPhpString, bformat, impl_php_exception};

#[derive(Debug)]
pub struct ProcessTimedOutException {
    inner: RuntimeException,
}

impl ProcessTimedOutException {
    #[track_caller]
    pub fn new(process: &Process) -> Self {
        let exceeded_timeout = process.get_timeout();

        let message = bformat!(
            "The process \"{}\" exceeded the timeout of {} seconds.",
            process.get_command_line(),
            exceeded_timeout
                .map(|t| PhpMixed::Float(t).to_php_string())
                .unwrap_or_default()
        );

        Self {
            inner: RuntimeException::new(message),
        }
    }
}

impl_php_exception!(
    ProcessTimedOutException,
    inner,
    r"Symfony\Component\Process\Exception\ProcessTimedOutException"
);
