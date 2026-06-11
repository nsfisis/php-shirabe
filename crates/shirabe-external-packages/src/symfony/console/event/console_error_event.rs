use super::console_event::ConsoleEvent;
use crate::symfony::console::command::command::Command;
use crate::symfony::console::input::input_interface::InputInterface;
use crate::symfony::console::output::output_interface::OutputInterface;

#[derive(Debug)]
pub struct ConsoleErrorEvent {
    inner: ConsoleEvent,
    error: Box<dyn std::error::Error + Send + Sync>,
    exit_code: Option<i64>,
}

impl ConsoleErrorEvent {
    pub fn new(
        input: Box<dyn InputInterface>,
        output: Box<dyn OutputInterface>,
        error: Box<dyn std::error::Error + Send + Sync>,
        command: Option<Box<dyn Command>>,
    ) -> Self {
        Self {
            inner: ConsoleEvent::new(command, input, output),
            error,
            exit_code: None,
        }
    }

    pub fn get_error(&self) -> &(dyn std::error::Error + Send + Sync) {
        self.error.as_ref()
    }

    pub fn set_error(&mut self, error: Box<dyn std::error::Error + Send + Sync>) {
        self.error = error;
    }

    pub fn set_exit_code(&mut self, exit_code: i64) {
        self.exit_code = Some(exit_code);
        // TODO: The PHP implementation uses \ReflectionProperty to forcibly set the `code`
        // field on the error object. This requires a Reflection API equivalent; review needed.
        todo!("set error code via reflection equivalent")
    }

    pub fn get_exit_code(&self) -> i64 {
        match self.exit_code {
            Some(code) => code,
            None => {
                // PHP: is_int($error->getCode()) && 0 !== $error->getCode() ? $error->getCode() : 1
                // Throwable::getCode() has no direct equivalent on std::error::Error.
                todo!("retrieve error code from Throwable equivalent")
            }
        }
    }
}
