//! ref: composer/vendor/symfony/console/Helper/HelperSet.php

use crate::symfony::console::helper::debug_formatter_helper::DebugFormatterHelper;
use crate::symfony::console::helper::formatter_helper::FormatterHelper;
use crate::symfony::console::helper::helper_interface::HelperInterface;
use crate::symfony::console::helper::process_helper::ProcessHelper;
use crate::symfony::console::helper::question_helper::QuestionHelper;

/// HelperSet represents a set of helpers to be used with a command.
///
/// Symfony lets arbitrary helpers be registered by name, but Composer only ever uses the four
/// helpers `Application::getDefaultHelperSet()` installs. This port closes the set to exactly those
/// four, instantiates them in the argument-less constructor, and exposes them through typed getters
/// instead of Symfony's string-keyed `get()`/`has()`/`set()`.
///
/// TODO(plugin): a plugin-defined custom command may register extra helpers dynamically via
/// `getApplication()->getHelperSet()`. Restoring that path (a `set()` equivalent plus name-based
/// lookup) is deferred until the plugin API is implemented.
#[derive(Debug)]
pub struct HelperSet {
    formatter_helper: std::rc::Rc<std::cell::RefCell<FormatterHelper>>,
    debug_formatter_helper: std::rc::Rc<std::cell::RefCell<DebugFormatterHelper>>,
    process_helper: std::rc::Rc<std::cell::RefCell<ProcessHelper>>,
    question_helper: std::rc::Rc<std::cell::RefCell<QuestionHelper>>,
}

impl HelperSet {
    /// Builds the fixed set of helpers and wires each one's back-reference to the owning set,
    /// mirroring the `$helper->setHelperSet($this)` call PHP's `HelperSet::set()` performs.
    pub fn new() -> std::rc::Rc<std::cell::RefCell<HelperSet>> {
        let formatter_helper =
            std::rc::Rc::new(std::cell::RefCell::new(FormatterHelper::default()));
        let debug_formatter_helper =
            std::rc::Rc::new(std::cell::RefCell::new(DebugFormatterHelper::default()));
        let process_helper = std::rc::Rc::new(std::cell::RefCell::new(ProcessHelper::default()));
        let question_helper = std::rc::Rc::new(std::cell::RefCell::new(QuestionHelper::default()));

        let this = std::rc::Rc::new(std::cell::RefCell::new(HelperSet {
            formatter_helper: formatter_helper.clone(),
            debug_formatter_helper: debug_formatter_helper.clone(),
            process_helper: process_helper.clone(),
            question_helper: question_helper.clone(),
        }));

        formatter_helper
            .borrow_mut()
            .set_helper_set(Some(this.clone()));
        debug_formatter_helper
            .borrow_mut()
            .set_helper_set(Some(this.clone()));
        process_helper
            .borrow_mut()
            .set_helper_set(Some(this.clone()));
        question_helper
            .borrow_mut()
            .set_helper_set(Some(this.clone()));

        this
    }

    pub fn get_formatter(&self) -> std::rc::Rc<std::cell::RefCell<FormatterHelper>> {
        self.formatter_helper.clone()
    }

    pub fn get_debug_formatter(&self) -> std::rc::Rc<std::cell::RefCell<DebugFormatterHelper>> {
        self.debug_formatter_helper.clone()
    }

    pub fn get_process(&self) -> std::rc::Rc<std::cell::RefCell<ProcessHelper>> {
        self.process_helper.clone()
    }

    pub fn get_question(&self) -> std::rc::Rc<std::cell::RefCell<QuestionHelper>> {
        self.question_helper.clone()
    }
}
