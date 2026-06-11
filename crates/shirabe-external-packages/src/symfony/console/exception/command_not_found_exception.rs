use super::exception_interface::ExceptionInterface;
use super::invalid_argument_exception::InvalidArgumentException;

#[derive(Debug)]
pub struct CommandNotFoundException {
    inner: InvalidArgumentException,
    alternatives: Vec<String>,
}

impl CommandNotFoundException {
    pub fn new(message: String, alternatives: Vec<String>, code: i64) -> Self {
        Self {
            inner: InvalidArgumentException(shirabe_php_shim::InvalidArgumentException {
                message,
                code,
            }),
            alternatives,
        }
    }

    pub fn get_alternatives(&self) -> &Vec<String> {
        &self.alternatives
    }
}

impl std::fmt::Display for CommandNotFoundException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl std::error::Error for CommandNotFoundException {}

impl ExceptionInterface for CommandNotFoundException {}
