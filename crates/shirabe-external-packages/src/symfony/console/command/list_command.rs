//! ref: composer/vendor/symfony/console/Command/ListCommand.php

use crate::symfony::console::command::command::{Command, CommandData, SetDefinitionArg};
use crate::symfony::console::completion::completion_input::CompletionInput;
use crate::symfony::console::completion::completion_suggestions::{
    CompletionSuggestions, StringOrSuggestion,
};
use crate::symfony::console::descriptor::application_description::ApplicationDescription;
use crate::symfony::console::descriptor::descriptor_interface::DescribableObject;
use crate::symfony::console::helper::descriptor_helper::DescriptorHelper;
use crate::symfony::console::input::input_argument::InputArgument;
use crate::symfony::console::input::input_definition::DefinitionItem;
use crate::symfony::console::input::input_interface::InputInterface;
use crate::symfony::console::input::input_option::InputOption;
use crate::symfony::console::output::output_interface::OutputInterface;
use shirabe_php_shim::PhpMixed;
use std::ops::{Deref, DerefMut};

/// ListCommand displays the list of all available commands for the application.
#[derive(Debug)]
pub struct ListCommand {
    inner: CommandData,
}

impl Deref for ListCommand {
    type Target = CommandData;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for ListCommand {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl Default for ListCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl ListCommand {
    pub fn new() -> Self {
        let command = ListCommand {
            inner: CommandData::new(None),
        };
        command
            .configure()
            .expect("ListCommand::configure uses static, valid metadata");
        command
    }

    pub fn complete_impl(&self, input: &CompletionInput, suggestions: &mut CompletionSuggestions) {
        if input.must_suggest_argument_values_for("namespace") {
            let application = self.get_application().unwrap();
            let mut descriptor = ApplicationDescription::new(application, None, false);
            suggestions.suggest_values(
                descriptor
                    .get_namespaces()
                    .keys()
                    .cloned()
                    .map(StringOrSuggestion::String)
                    .collect(),
            );

            return;
        }

        if input.must_suggest_option_values_for("format") {
            let helper = DescriptorHelper::new();
            suggestions.suggest_values(
                helper
                    .get_formats()
                    .into_iter()
                    .map(StringOrSuggestion::String)
                    .collect(),
            );
        }
    }
}

impl Command for ListCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.inner.set_name("list")?;
        self.inner.set_definition(SetDefinitionArg::Array(vec![
            DefinitionItem::InputArgument(InputArgument::new(
                "namespace".to_string(),
                Some(InputArgument::OPTIONAL),
                "The namespace name".to_string(),
                PhpMixed::Null,
            )?),
            DefinitionItem::InputOption(InputOption::new(
                "raw",
                PhpMixed::Null,
                Some(InputOption::VALUE_NONE),
                "To output raw command list".to_string(),
                PhpMixed::Null,
            )?),
            DefinitionItem::InputOption(InputOption::new(
                "format",
                PhpMixed::Null,
                Some(InputOption::VALUE_REQUIRED),
                "The output format (txt, xml, json, or md)".to_string(),
                PhpMixed::from("txt".to_string()),
            )?),
            DefinitionItem::InputOption(InputOption::new(
                "short",
                PhpMixed::Null,
                Some(InputOption::VALUE_NONE),
                "To skip describing commands' arguments".to_string(),
                PhpMixed::Null,
            )?),
        ]));
        self.inner.set_description("List commands");
        self.inner.set_help(
                "The <info>%command.name%</info> command lists all commands:\n\
                 \n\
                 \x20\x20<info>%command.full_name%</info>\n\
                 \n\
                 You can also display the commands for a specific namespace:\n\
                 \n\
                 \x20\x20<info>%command.full_name% test</info>\n\
                 \n\
                 You can also output the information in other formats by using the <comment>--format</comment> option:\n\
                 \n\
                 \x20\x20<info>%command.full_name% --format=xml</info>\n\
                 \n\
                 It's also possible to get raw list of commands (useful for embedding command runner):\n\
                 \n\
                 \x20\x20<info>%command.full_name% --raw</info>",
            );

        Ok(())
    }

    fn execute(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let mut helper = DescriptorHelper::new();
        let object = DescribableObject::Application(self.get_application().unwrap());
        let mut options = indexmap::IndexMap::new();
        options.insert("format".to_string(), input.borrow().get_option("format")?);
        options.insert("raw_text".to_string(), input.borrow().get_option("raw")?);
        options.insert(
            "namespace".to_string(),
            input.borrow().get_argument("namespace")?,
        );
        options.insert("short".to_string(), input.borrow().get_option("short")?);
        helper.describe2(output.clone(), object, options)?;

        Ok(0)
    }

    fn complete(&self, input: &CompletionInput, suggestions: &mut CompletionSuggestions) {
        self.complete_impl(input, suggestions);
    }

    crate::delegate_command_trait_impls_to_inner!(inner);
}
