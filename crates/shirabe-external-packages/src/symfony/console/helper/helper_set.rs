use crate::symfony::console::command::command::Command;
use crate::symfony::console::exception::invalid_argument_exception::InvalidArgumentException;
use crate::symfony::console::helper::helper_interface::HelperInterface;
use indexmap::IndexMap;
use std::cell::RefCell;
use std::rc::Rc;

/// HelperSet represents a set of helpers to be used with a command.
///
/// @implements \IteratorAggregate<string, Helper>
#[derive(Debug, Default, Clone)]
pub struct HelperSet {
    helpers: IndexMap<String, Rc<RefCell<dyn HelperInterface>>>,
    command: Option<Rc<RefCell<dyn Command>>>,
}

impl HelperSet {
    /// @param Helper[] $helpers An array of helper
    pub fn new(
        this: &Rc<RefCell<HelperSet>>,
        helpers: IndexMap<HelperSetKey, Rc<RefCell<dyn HelperInterface>>>,
    ) {
        for (alias, helper) in helpers {
            let alias = match alias {
                HelperSetKey::Int(_) => None,
                HelperSetKey::String(alias) => Some(alias),
            };
            Self::set(this, helper, alias.as_deref());
        }
    }

    pub fn set(
        this: &Rc<RefCell<HelperSet>>,
        helper: Rc<RefCell<dyn HelperInterface>>,
        alias: Option<&str>,
    ) {
        let name = helper.borrow().get_name();
        this.borrow_mut().helpers.insert(name, helper.clone());
        if let Some(alias) = alias {
            this.borrow_mut()
                .helpers
                .insert(alias.to_string(), helper.clone());
        }

        helper.borrow_mut().set_helper_set(Some(this.clone()));
    }

    /// Returns true if the helper if defined.
    pub fn has(&self, name: &str) -> bool {
        self.helpers.contains_key(name)
    }

    /// Gets a helper value.
    ///
    /// @throws InvalidArgumentException if the helper is not defined
    pub fn get(
        &self,
        name: &str,
    ) -> Result<Rc<RefCell<dyn HelperInterface>>, InvalidArgumentException> {
        if !self.has(name) {
            return Err(InvalidArgumentException(
                shirabe_php_shim::InvalidArgumentException {
                    message: shirabe_php_shim::sprintf(
                        "The helper \"%s\" is not defined.",
                        &[shirabe_php_shim::PhpMixed::String(name.to_string())],
                    ),
                    code: 0,
                },
            ));
        }

        Ok(self.helpers[name].clone())
    }

    /// @deprecated since Symfony 5.4
    pub fn set_command(&mut self, command: Option<Rc<RefCell<dyn Command>>>) {
        shirabe_php_shim::trigger_deprecation(
            "symfony/console",
            "5.4",
            "Method \"%s()\" is deprecated.",
            "HelperSet::setCommand",
        );

        self.command = command;
    }

    /// Gets the command associated with this helper set.
    ///
    /// @deprecated since Symfony 5.4
    pub fn get_command(&self) -> Option<Rc<RefCell<dyn Command>>> {
        shirabe_php_shim::trigger_deprecation(
            "symfony/console",
            "5.4",
            "Method \"%s()\" is deprecated.",
            "HelperSet::getCommand",
        );

        self.command.clone()
    }

    /// @return \Traversable<string, Helper>
    pub fn get_iterator(
        &self,
    ) -> impl Iterator<Item = (&String, &Rc<RefCell<dyn HelperInterface>>)> {
        self.helpers.iter()
    }
}

/// PHP array keys are either integers or strings; the HelperSet constructor
/// distinguishes them via `\is_int($alias)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HelperSetKey {
    Int(i64),
    String(String),
}
