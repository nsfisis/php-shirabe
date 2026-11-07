//! ref: composer/vendor/symfony/console/Exception/CommandNotFoundException.php sha1:4cbfe3d0bd964e1153c09786e456a73af9234eb9

use crate::exception::ExceptionInterface;
use bstr::BString;
use shirabe_php_shim::{InvalidArgumentException, impl_php_exception};

#[derive(Debug)]
pub struct CommandNotFoundException {
    inner: InvalidArgumentException,
    alternatives: Vec<BString>,
}

impl CommandNotFoundException {
    #[track_caller]
    pub fn new(message: impl Into<BString>, alternatives: Vec<BString>, code: i64) -> Self {
        Self {
            inner: InvalidArgumentException::with_code(message, code),
            alternatives,
        }
    }

    pub fn get_alternatives(&self) -> &Vec<BString> {
        &self.alternatives
    }
}

impl_php_exception!(
    CommandNotFoundException,
    inner,
    r"Symfony\Component\Console\Exception\CommandNotFoundException"
);

impl ExceptionInterface for CommandNotFoundException {}
