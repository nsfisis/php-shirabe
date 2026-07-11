//! ref: composer/vendor/symfony/console/Helper/HelperInterface.php

use crate::symfony::console::helper::helper_set::HelperSet;

/// HelperInterface is the interface all helpers must implement.
pub trait HelperInterface: std::fmt::Debug + shirabe_php_shim::AsAny {
    /// Sets the helper set associated with this helper.
    fn set_helper_set(&mut self, helper_set: Option<std::rc::Rc<std::cell::RefCell<HelperSet>>>);

    /// Gets the helper set associated with this helper.
    fn get_helper_set(&self) -> Option<std::rc::Rc<std::cell::RefCell<HelperSet>>>;

    /// Returns the canonical name of this helper.
    fn get_name(&self) -> String;
}
