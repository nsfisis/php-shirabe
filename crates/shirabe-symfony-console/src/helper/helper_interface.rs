//! ref: composer/vendor/symfony/console/Helper/HelperInterface.php sha1:1a5ec81e253b2f3f8dbd96b14b3b205b76a4c655

use crate::helper::HelperSet;
use bstr::BString;
use shirabe_php_shim::AsAny;

/// HelperInterface is the interface all helpers must implement.
pub trait HelperInterface: std::fmt::Debug + AsAny {
    /// Sets the helper set associated with this helper.
    fn set_helper_set(&mut self, helper_set: Option<std::rc::Rc<std::cell::RefCell<HelperSet>>>);

    /// Gets the helper set associated with this helper.
    fn get_helper_set(&self) -> Option<std::rc::Rc<std::cell::RefCell<HelperSet>>>;

    /// Returns the canonical name of this helper.
    fn get_name(&self) -> BString;
}
