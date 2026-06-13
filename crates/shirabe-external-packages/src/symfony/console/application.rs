//! ref: composer/vendor/symfony/console/Application.php

use crate::symfony::console::command::command::Command;
use crate::symfony::console::completion::completion_input::CompletionInput;
use crate::symfony::console::completion::completion_suggestions::CompletionSuggestions;
use crate::symfony::console::helper::helper_set::HelperSet;
use crate::symfony::console::input::input_definition::InputDefinition;
use indexmap::IndexMap;
use std::cell::RefCell;
use std::rc::Rc;

/// `Symfony\Component\Console\Application` is a concrete class in PHP, but it is ported here as a
/// trait rather than a struct.
/// Refer to shirabe::console::Application for the reason.
pub trait Application: std::fmt::Debug {
    fn get_name(&self) -> String;

    fn get_version(&self) -> String;

    fn get_help(&self) -> String;

    fn is_single_command(&self) -> bool;

    fn extract_namespace(&self, name: &str, limit: Option<i64>) -> String;

    fn find_namespace(&mut self, namespace: &str) -> anyhow::Result<String>;

    fn all(
        &mut self,
        namespace: Option<&str>,
    ) -> anyhow::Result<IndexMap<String, Rc<RefCell<dyn Command>>>>;

    fn find(&mut self, name: &str) -> anyhow::Result<Rc<RefCell<dyn Command>>>;

    fn get_definition(&mut self) -> Rc<RefCell<InputDefinition>>;

    fn get_helper_set(&mut self) -> Rc<RefCell<HelperSet>>;

    fn complete(
        &mut self,
        input: &CompletionInput,
        suggestions: &mut CompletionSuggestions,
    ) -> anyhow::Result<()>;
}
