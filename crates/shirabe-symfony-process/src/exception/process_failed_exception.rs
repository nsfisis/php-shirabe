//! ref: composer/vendor/symfony/process/Exception/ProcessFailedException.php sha1:a5b53e81219ba83308d6436ec5977972aea42d8f

use crate::Process;
use crate::exception::{InvalidArgumentException, RuntimeException};
use shirabe_php_shim::{bformat, bwrite, impl_php_exception};

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

        let mut error = bformat!(
            "The command \"{}\" failed.\n\nExit Code: {}({})\n\nWorking directory: {}",
            process.get_command_line(),
            process
                .get_exit_code()
                .map(|c| bformat!("{c}"))
                .unwrap_or_default(),
            process.get_exit_code_text().unwrap_or_default(),
            process.get_working_directory().unwrap_or_default()
        );

        bwrite!(
            error,
            "\n\nOutput:\n================\n{}\n\nError Output:\n================\n{}",
            process.get_output()?,
            process.get_error_output()?
        );

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
