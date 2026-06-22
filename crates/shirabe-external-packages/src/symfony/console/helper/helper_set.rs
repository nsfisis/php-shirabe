//! ref: composer/vendor/symfony/console/Helper/HelperSet.php

use crate::symfony::console::helper::debug_formatter_helper::DebugFormatterHelper;
use crate::symfony::console::helper::formatter_helper::FormatterHelper;
use crate::symfony::console::helper::helper_interface::HelperInterface;
use crate::symfony::console::helper::process_helper::ProcessHelper;
use crate::symfony::console::helper::question_helper::QuestionHelper;
use std::cell::RefCell;
use std::rc::Rc;

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
    formatter_helper: Rc<RefCell<FormatterHelper>>,
    debug_formatter_helper: Rc<RefCell<DebugFormatterHelper>>,
    process_helper: Rc<RefCell<ProcessHelper>>,
    question_helper: Rc<RefCell<QuestionHelper>>,
}

impl HelperSet {
    /// Builds the fixed set of helpers and wires each one's back-reference to the owning set,
    /// mirroring the `$helper->setHelperSet($this)` call PHP's `HelperSet::set()` performs.
    pub fn new() -> Rc<RefCell<HelperSet>> {
        let formatter_helper = Rc::new(RefCell::new(FormatterHelper::default()));
        let debug_formatter_helper = Rc::new(RefCell::new(DebugFormatterHelper::default()));
        let process_helper = Rc::new(RefCell::new(ProcessHelper::default()));
        let question_helper = Rc::new(RefCell::new(QuestionHelper::default()));

        let this = Rc::new(RefCell::new(HelperSet {
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

    pub fn get_formatter(&self) -> Rc<RefCell<FormatterHelper>> {
        self.formatter_helper.clone()
    }

    pub fn get_debug_formatter(&self) -> Rc<RefCell<DebugFormatterHelper>> {
        self.debug_formatter_helper.clone()
    }

    pub fn get_process(&self) -> Rc<RefCell<ProcessHelper>> {
        self.process_helper.clone()
    }

    pub fn get_question(&self) -> Rc<RefCell<QuestionHelper>> {
        self.question_helper.clone()
    }
}
