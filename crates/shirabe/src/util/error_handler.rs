//! ref: composer/src/Composer/Util/ErrorHandler.php sha1:f6945f00fd097a36ed223e0c527ff1b7fe1cd0ac
//!
//! The handler itself is not ported: the real class runs in the PHP worker, where plugin and script
//! code raises PHP errors, and `register` forwards there (see `php_worker::error_handler`).

use crate::io::IOInterface;
use crate::php_worker::error_handler;

#[derive(Debug)]
pub struct ErrorHandler;

impl ErrorHandler {
    /// Register error handler.
    pub fn register(
        io: Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>,
    ) -> anyhow::Result<()> {
        error_handler::register(io)
    }
}
