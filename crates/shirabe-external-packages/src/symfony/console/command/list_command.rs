use crate::symfony::console::command::command::{BaseCommand, Command, SetDefinitionArg};
use crate::symfony::console::completion::completion_input::CompletionInput;
use crate::symfony::console::completion::completion_suggestions::{
    CompletionSuggestions, StringOrSuggestion,
};
use crate::symfony::console::descriptor::application_description::ApplicationDescription;
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

/// ListCommand displays the list of all available commands for the application.
#[derive(Debug)]
pub struct ListCommand {
    inner: BaseCommand,
}

impl Deref for ListCommand {
    type Target = BaseCommand;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for ListCommand {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl ListCommand {
    fn configure(&mut self) -> anyhow::Result<()> {
        self.inner
            .set_name("list")?
            .set_definition(SetDefinitionArg::Array(vec![
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
            ]))
            .set_description("List commands")
            .set_help(
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
        &mut self,
        input: &mut dyn InputInterface,
        output: &mut dyn OutputInterface,
    ) -> anyhow::Result<i64> {
        let helper = DescriptorHelper::new();
        // TODO: DescriptorHelper::describe2 takes the described object as Option<PhpMixed>,
        // but PhpMixed cannot hold an Application. The Command/Application object mixing for
        // describe needs a dedicated type (Phase C).
        let object: Option<PhpMixed> = todo!();
        let mut options = indexmap::IndexMap::new();
        options.insert("format".to_string(), input.get_option("format")?);
        options.insert("raw_text".to_string(), input.get_option("raw")?);
        options.insert("namespace".to_string(), input.get_argument("namespace")?);
        options.insert("short".to_string(), input.get_option("short")?);
        let _ = helper.describe2(output, object, options);

        Ok(0)
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
    fn configure(&mut self) {
        let _ = ListCommand::configure(self);
    }

    fn run(
        &mut self,
        input: &mut dyn InputInterface,
        output: &mut dyn OutputInterface,
    ) -> anyhow::Result<i64> {
        self.inner.run(input, output)
    }

    fn complete(&self, input: &CompletionInput, suggestions: &mut CompletionSuggestions) {
        self.complete_impl(input, suggestions);
    }

    fn is_enabled(&self) -> bool {
        self.inner.is_enabled()
    }

    fn set_application(
        &mut self,
        application: Option<Rc<RefCell<crate::symfony::console::application::Application>>>,
    ) {
        self.inner.set_application(application);
    }

    fn get_application(
        &self,
    ) -> Option<Rc<RefCell<crate::symfony::console::application::Application>>> {
        self.inner.get_application()
    }

    fn set_helper_set(
        &mut self,
        helper_set: Rc<RefCell<crate::symfony::console::helper::helper_set::HelperSet>>,
    ) {
        self.inner.set_helper_set(helper_set);
    }

    fn get_helper_set(
        &self,
    ) -> Option<Rc<RefCell<crate::symfony::console::helper::helper_set::HelperSet>>> {
        self.inner.get_helper_set()
    }

    fn merge_application_definition(&mut self, merge_args: bool) {
        self.inner.merge_application_definition(merge_args);
    }

    fn get_definition(&self) -> &crate::symfony::console::input::input_definition::InputDefinition {
        self.inner.get_definition()
    }

    fn get_native_definition(
        &self,
    ) -> &crate::symfony::console::input::input_definition::InputDefinition {
        self.inner.get_native_definition()
    }

    fn set_name(&mut self, name: &str) -> anyhow::Result<()> {
        self.inner.set_name(name)?;
        Ok(())
    }

    fn get_name(&self) -> Option<String> {
        self.inner.get_name()
    }

    fn set_hidden(&mut self, hidden: bool) {
        self.inner.set_hidden(hidden);
    }

    fn is_hidden(&self) -> bool {
        self.inner.is_hidden()
    }

    fn set_description(&mut self, description: &str) {
        self.inner.set_description(description);
    }

    fn get_description(&self) -> String {
        self.inner.get_description()
    }

    fn set_help(&mut self, help: &str) {
        self.inner.set_help(help);
    }

    fn get_help(&self) -> String {
        self.inner.get_help()
    }

    fn get_processed_help(&self) -> String {
        self.inner.get_processed_help()
    }

    fn set_aliases(&mut self, aliases: Vec<String>) -> anyhow::Result<()> {
        self.inner.set_aliases(aliases)?;
        Ok(())
    }

    fn get_aliases(&self) -> Vec<String> {
        self.inner.get_aliases()
    }

    fn get_synopsis(&mut self, short: bool) -> String {
        self.inner.get_synopsis(short)
    }

    fn get_usages(&self) -> Vec<String> {
        self.inner.get_usages()
    }

    fn get_helper(
        &self,
        name: &str,
    ) -> anyhow::Result<
        Result<PhpMixed, crate::symfony::console::exception::logic_exception::LogicException>,
    > {
        self.inner.get_helper(name)
    }

    fn ignore_validation_errors(&mut self) {
        self.inner.ignore_validation_errors();
    }
}
