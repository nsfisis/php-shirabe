//! ref: composer/vendor/symfony/console/Application.php

use crate::command::Command;
use crate::completion::{CompletionInput, CompletionSuggestions};
use crate::helper::HelperSet;
use crate::input::InputDefinition;
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_shim::AsAny;

/// `Symfony\Component\Console\Application` is a concrete class in PHP, but it is ported here as a
/// trait rather than a struct.
/// Refer to shirabe::console::Application for the reason.
pub trait Application: std::fmt::Debug + AsAny {
    fn get_name(&self) -> BString;

    fn get_version(&self) -> BString;

    fn get_help(&self) -> BString;

    fn is_single_command(&self) -> bool;

    fn extract_namespace(&self, name: &BStr, limit: Option<i64>) -> BString;

    fn find_namespace(&mut self, namespace: &BStr) -> anyhow::Result<BString>;

    fn all(
        &mut self,
        namespace: Option<&BStr>,
    ) -> anyhow::Result<IndexMap<BString, std::rc::Rc<std::cell::RefCell<dyn Command>>>>;

    fn find(&mut self, name: &BStr)
    -> anyhow::Result<std::rc::Rc<std::cell::RefCell<dyn Command>>>;

    fn get_definition(&mut self) -> std::rc::Rc<std::cell::RefCell<InputDefinition>>;

    fn get_helper_set(&mut self) -> std::rc::Rc<std::cell::RefCell<HelperSet>>;

    fn complete(
        &mut self,
        input: &CompletionInput,
        suggestions: &mut CompletionSuggestions,
    ) -> anyhow::Result<()>;
}
