//! ref: composer/vendor/symfony/console/Command/HelpCommand.php

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
use std::cell::RefCell;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

/// HelpCommand displays the help for a given command.
#[derive(Debug)]
pub struct HelpCommand {
    inner: CommandData,
    command: Option<Rc<RefCell<dyn Command>>>,
}

impl Deref for HelpCommand {
    type Target = CommandData;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for HelpCommand {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl Default for HelpCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl HelpCommand {
    pub fn new() -> Self {
        let mut command = HelpCommand {
            inner: CommandData::new(None),
            command: None,
        };
        command
            .configure()
            .expect("HelpCommand::configure uses static, valid metadata");
        command
    }

    pub fn set_command(&mut self, command: Rc<RefCell<dyn Command>>) {
        self.command = Some(command);
    }

    pub fn complete_impl(&self, input: &CompletionInput, suggestions: &mut CompletionSuggestions) {
        if input.must_suggest_argument_values_for("command_name") {
            let application = self.get_application().unwrap();
            let mut descriptor = ApplicationDescription::new(application, None, false);
            suggestions.suggest_values(
                descriptor
                    .get_commands()
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

impl Command for HelpCommand {
    fn configure(&mut self) -> anyhow::Result<()> {
        self.inner.ignore_validation_errors();

        self.inner.set_name("help")?;
        self.inner.set_definition(SetDefinitionArg::Array(vec![
            DefinitionItem::InputArgument(InputArgument::new(
                "command_name".to_string(),
                Some(InputArgument::OPTIONAL),
                "The command name".to_string(),
                PhpMixed::from("help".to_string()),
            )?),
            DefinitionItem::InputOption(InputOption::new(
                "format",
                PhpMixed::Null,
                Some(InputOption::VALUE_REQUIRED),
                "The output format (txt, xml, json, or md)".to_string(),
                PhpMixed::from("txt".to_string()),
            )?),
            DefinitionItem::InputOption(InputOption::new(
                "raw",
                PhpMixed::Null,
                Some(InputOption::VALUE_NONE),
                "To output raw command help".to_string(),
                PhpMixed::Null,
            )?),
        ]));
        self.inner.set_description("Display help for a command");
        self.inner.set_help(
                "The <info>%command.name%</info> command displays help for a given command:\n\
                 \n\
                 \x20\x20<info>%command.full_name% list</info>\n\
                 \n\
                 You can also output the help in other formats by using the <comment>--format</comment> option:\n\
                 \n\
                 \x20\x20<info>%command.full_name% --format=xml list</info>\n\
                 \n\
                 To display the list of available commands, please use the <info>list</info> command.",
            );

        Ok(())
    }

    fn execute(
        &mut self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        if self.command.is_none() {
            let application = self.get_application().unwrap();
            let command_name = input.borrow().get_argument("command_name")?.to_string();
            self.command = Some(application.borrow_mut().find(&command_name)?);
        }

        let mut helper = DescriptorHelper::new();
        let object = DescribableObject::Command(self.command.clone().unwrap());
        let mut options = indexmap::IndexMap::new();
        options.insert("format".to_string(), input.borrow().get_option("format")?);
        options.insert("raw_text".to_string(), input.borrow().get_option("raw")?);
        helper.describe2(output.clone(), object, options)?;

        self.command = None;

        Ok(0)
    }

    fn complete(&self, input: &CompletionInput, suggestions: &mut CompletionSuggestions) {
        self.complete_impl(input, suggestions);
    }

    crate::delegate_command_trait_impls_to_inner!(inner);
}
