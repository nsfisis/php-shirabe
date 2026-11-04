//! ref: composer/vendor/symfony/process/Exception/ProcessTimedOutException.php sha1:7bed6e96f41a9c813a9f71419e703905ae3d4872

use crate::exception::RuntimeException;
use crate::process::Process;
use shirabe_php_shim::{PhpMixed, bconcat, php_to_string};

#[derive(Debug)]
pub struct ProcessTimedOutException {
    inner: RuntimeException,
}

impl ProcessTimedOutException {
    #[track_caller]
    pub fn new(process: &Process) -> Self {
        let exceeded_timeout = process.get_timeout();

        let message = bconcat!(
            "The process \"",
            process.get_command_line(),
            "\" exceeded the timeout of ",
            exceeded_timeout
                .map(|t| php_to_string(&PhpMixed::Float(t)))
                .unwrap_or_default(),
            " seconds."
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
