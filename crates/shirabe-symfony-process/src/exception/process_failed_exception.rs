//! ref: composer/vendor/symfony/process/Exception/ProcessFailedException.php sha1:a5b53e81219ba83308d6436ec5977972aea42d8f

use crate::exception::{InvalidArgumentException, RuntimeException};
use crate::process::Process;
use shirabe_php_shim::{bconcat, impl_php_exception};

#[derive(Debug)]
pub struct ProcessFailedException {
    inner: RuntimeException,
}

impl ProcessFailedException {
    #[track_caller]
    pub fn new(process: &mut Process) -> anyhow::Result<Self> {
        if process.is_successful() {
            return Err(InvalidArgumentException::new(
                "Expected a failed process, but the given process was successful.",
            )
            .into());
        }

        let mut error = bconcat!(
            "The command \"",
            process.get_command_line(),
            "\" failed.\n\nExit Code: ",
            process
                .get_exit_code()
                .map(|c| bconcat!(c))
                .unwrap_or_default(),
            "(",
            process.get_exit_code_text().unwrap_or_default(),
            ")\n\nWorking directory: ",
            process.get_working_directory().unwrap_or_default(),
        );

        error.extend_from_slice(&bconcat!(
            "\n\nOutput:\n================\n",
            process.get_output()?,
            "\n\nError Output:\n================\n",
            process.get_error_output()?,
        ));

        Ok(Self {
            inner: RuntimeException::new(error),
        })
    }
}

impl_php_exception!(
    ProcessFailedException,
    inner,
    r"Symfony\Component\Process\Exception\ProcessFailedException"
);
