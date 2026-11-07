//! ref: composer/vendor/symfony/console/Helper/HelperSet.php sha1:42581141559106863690d91c4ed00ce6a968be6a

use crate::exception::InvalidArgumentException;
use crate::helper::{
    DebugFormatterHelper, FormatterHelper, HelperInterface, ProcessHelper, QuestionHelper,
};
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_shim::bformat;

/// HelperSet represents a set of helpers to be used with a command.
#[derive(Debug)]
pub struct HelperSet {
    helpers: IndexMap<BString, std::rc::Rc<std::cell::RefCell<dyn HelperInterface>>>,
}

impl HelperSet {
    /// Each entry pairs an optional alias with a helper; PHP's integer-keyed entries of
    /// `$helpers` are the ones without an alias.
    pub fn new(
        helpers: Vec<(
            Option<BString>,
            std::rc::Rc<std::cell::RefCell<dyn HelperInterface>>,
        )>,
    ) -> std::rc::Rc<std::cell::RefCell<HelperSet>> {
        let this = std::rc::Rc::new(std::cell::RefCell::new(HelperSet {
            helpers: IndexMap::new(),
        }));
        for (alias, helper) in helpers {
            HelperSet::set(&this, helper, alias.as_ref().map(|alias| alias.as_ref()));
        }

        this
    }

    pub fn set(
        this: &std::rc::Rc<std::cell::RefCell<HelperSet>>,
        helper: std::rc::Rc<std::cell::RefCell<dyn HelperInterface>>,
        alias: Option<&BStr>,
    ) {
        {
            let name = helper.borrow().get_name();
            let mut helper_set = this.borrow_mut();
            helper_set.helpers.insert(name, helper.clone());
            if let Some(alias) = alias {
                helper_set.helpers.insert(alias.into(), helper.clone());
            }
        }

        helper.borrow_mut().set_helper_set(Some(this.clone()));
    }

    /// Returns true if the helper if defined.
    pub fn has(&self, name: &BStr) -> bool {
        self.helpers.contains_key(name)
    }

    /// Gets a helper value.
    pub fn get(
        &self,
        name: &BStr,
    ) -> anyhow::Result<std::rc::Rc<std::cell::RefCell<dyn HelperInterface>>> {
        if !self.has(name) {
            anyhow::bail!(InvalidArgumentException::new(bformat!(
                "The helper \"{name}\" is not defined."
            )));
        }

        Ok(self.helpers[name].clone())
    }

    pub fn get_iterator(
        &self,
    ) -> &IndexMap<BString, std::rc::Rc<std::cell::RefCell<dyn HelperInterface>>> {
        &self.helpers
    }

    /// `get('formatter')` as the concrete class the callers rely on.
    pub fn get_formatter(&self) -> std::rc::Rc<std::cell::RefCell<FormatterHelper>> {
        self.get_typed(BStr::new("formatter"))
    }

    /// `get('debug_formatter')` as the concrete class the callers rely on.
    pub fn get_debug_formatter(&self) -> std::rc::Rc<std::cell::RefCell<DebugFormatterHelper>> {
        self.get_typed(BStr::new("debug_formatter"))
    }

    /// `get('process')` as the concrete class the callers rely on.
    pub fn get_process(&self) -> std::rc::Rc<std::cell::RefCell<ProcessHelper>> {
        self.get_typed(BStr::new("process"))
    }

    /// `get('question')` as the concrete class the callers rely on.
    pub fn get_question(&self) -> std::rc::Rc<std::cell::RefCell<QuestionHelper>> {
        self.get_typed(BStr::new("question"))
    }

    /// Every helper in this set is registered by Rust code (a plugin command runs in the PHP
    /// worker and resolves helpers against that process's own helper set), so a missing name or
    /// a helper of another class under a built-in name is a programming error.
    fn get_typed<T: HelperInterface + 'static>(
        &self,
        name: &BStr,
    ) -> std::rc::Rc<std::cell::RefCell<T>> {
        let Some(helper) = self.helpers.get(name).cloned() else {
            panic!("The helper {name:?} is not defined.");
        };
        if !helper.borrow().as_any().is::<T>() {
            panic!(
                "The helper {name:?} is not a {}.",
                std::any::type_name::<T>()
            );
        }
        let ptr = std::rc::Rc::into_raw(helper) as *const std::cell::RefCell<T>;
        // SAFETY: `is::<T>()` proved the value is `T`, and the `dyn HelperInterface` handle was
        // unsize-coerced from an `Rc<RefCell<T>>`, so the allocation is an `RcBox<RefCell<T>>`.
        unsafe { std::rc::Rc::from_raw(ptr) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shirabe_php_shim::Catch;

    fn formatter() -> std::rc::Rc<std::cell::RefCell<dyn HelperInterface>> {
        std::rc::Rc::new(std::cell::RefCell::new(FormatterHelper::default()))
    }

    #[test]
    fn constructor_registers_helpers_by_name_and_alias() {
        let helper_set = HelperSet::new(vec![
            (None, formatter()),
            (
                Some(BString::from("alias")),
                std::rc::Rc::new(std::cell::RefCell::new(QuestionHelper::default())),
            ),
        ]);

        let helper_set = helper_set.borrow();
        assert!(helper_set.has(BStr::new("formatter")));
        assert!(helper_set.has(BStr::new("question")));
        assert!(helper_set.has(BStr::new("alias")));
        assert!(!helper_set.has(BStr::new("process")));
        assert!(std::rc::Rc::ptr_eq(
            &helper_set.get(BStr::new("question")).unwrap(),
            &helper_set.get(BStr::new("alias")).unwrap(),
        ));
        assert_eq!(
            helper_set.get_iterator().keys().collect::<Vec<_>>(),
            vec!["formatter", "question", "alias"],
        );
    }

    #[test]
    fn set_wires_the_helper_back_to_the_set() {
        let helper_set = HelperSet::new(vec![]);
        let helper = formatter();
        HelperSet::set(&helper_set, helper.clone(), Some(BStr::new("fmt")));

        assert!(std::rc::Rc::ptr_eq(
            &helper.borrow().get_helper_set().unwrap(),
            &helper_set,
        ));
        assert!(helper_set.borrow().has(BStr::new("formatter")));
        assert!(helper_set.borrow().has(BStr::new("fmt")));
    }

    #[test]
    fn get_returns_the_registered_instance() {
        let helper = formatter();
        let helper_set = HelperSet::new(vec![(None, helper.clone())]);

        assert!(std::rc::Rc::ptr_eq(
            &helper_set.borrow().get(BStr::new("formatter")).unwrap(),
            &helper,
        ));
        let typed: std::rc::Rc<std::cell::RefCell<dyn HelperInterface>> =
            helper_set.borrow().get_formatter();
        assert!(std::rc::Rc::ptr_eq(&typed, &helper));
    }

    #[test]
    fn get_throws_for_an_undefined_helper() {
        let helper_set = HelperSet::new(vec![]);
        let error = helper_set.borrow().get(BStr::new("foo")).unwrap_err();

        let exception = error.catch::<InvalidArgumentException>().unwrap();
        assert_eq!(
            exception.get_message(),
            "The helper \"foo\" is not defined."
        );
    }
}
