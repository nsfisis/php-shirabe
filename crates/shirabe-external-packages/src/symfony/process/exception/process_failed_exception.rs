//! ref: composer/vendor/symfony/process/Exception/ProcessFailedException.php

use crate::symfony::process::exception::invalid_argument_exception::InvalidArgumentException;
use crate::symfony::process::process::Process;

#[derive(Debug)]
pub struct ProcessFailedException {
    pub message: String,
    pub code: i64,
}

impl ProcessFailedException {
    pub fn new(process: &mut Process) -> anyhow::Result<Self> {
        if process.is_successful() {
            return Err(InvalidArgumentException::new(
                "Expected a failed process, but the given process was successful.".to_string(),
            )
            .into());
        }

        let mut error = format!(
            "The command \"{}\" failed.\n\nExit Code: {}({})\n\nWorking directory: {}",
            process.get_command_line(),
            process
                .get_exit_code()
                .map(|c| c.to_string())
                .unwrap_or_default(),
            process.get_exit_code_text().unwrap_or_default(),
            process.get_working_directory().unwrap_or_default(),
        );

        error += &format!(
            "\n\nOutput:\n================\n{}\n\nError Output:\n================\n{}",
            process.get_output()?,
            process.get_error_output()?,
        );

        Ok(Self {
            message: error,
            code: 0,
        })
    }
}

impl std::fmt::Display for ProcessFailedException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ProcessFailedException {}
