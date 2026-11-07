//! ref: composer/vendor/symfony/console/Exception/NamespaceNotFoundException.php sha1:d05538674c77701898da90ca8f9010ffa14d6aba

use super::command_not_found_exception::CommandNotFoundException;
use super::exception_interface::ExceptionInterface;
use bstr::BString;
use shirabe_php_shim::impl_php_exception;

#[derive(Debug)]
pub struct NamespaceNotFoundException(pub CommandNotFoundException);

impl NamespaceNotFoundException {
    #[track_caller]
    pub fn new(message: impl Into<BString>, alternatives: Vec<BString>, code: i64) -> Self {
        Self(CommandNotFoundException::new(message, alternatives, code))
    }
}

impl_php_exception!(
    NamespaceNotFoundException,
    0,
    r"Symfony\Component\Console\Exception\NamespaceNotFoundException"
);

impl ExceptionInterface for NamespaceNotFoundException {}
