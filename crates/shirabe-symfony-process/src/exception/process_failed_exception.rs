//! ref: composer/vendor/symfony/process/Exception/ProcessFailedException.php

use crate::exception::{InvalidArgumentException, RuntimeException};
use crate::process::Process;

#[derive(Debug)]
pub struct ProcessFailedException {
    inner: RuntimeException,
}

impl ProcessFailedException {
    #[track_caller]
    pub fn new(process: &mut Process) -> anyhow::Result<Self> {
        if process.is_successful() {
            return Err(InvalidArgumentException::new(
                "Expected a failed process, but the given process was successful.".to_string(),
            )
            .into());
        }

        let mut error = format!(
            "The command \"{}\" failed.\n\nExit Code: {}({})\n\nWorking directory: {}",
            process.get_command_line().display_lossy(),
            process
                .get_exit_code()
                .map(|c| c.to_string())
                .unwrap_or_default(),
            process.get_exit_code_text().unwrap_or_default(),
            process
                .get_working_directory()
                .unwrap_or_default()
                .display(),
        );

        error += &format!(
            "\n\nOutput:\n================\n{}\n\nError Output:\n================\n{}",
            process.get_output()?.display_lossy(),
            process.get_error_output()?.display_lossy(),
        );

        Ok(Self {
            inner: RuntimeException::new(error),
        })
    }
}

shirabe_php_shim::impl_php_exception!(
    ProcessFailedException,
    inner,
    r"Symfony\Component\Process\Exception\ProcessFailedException"
);
