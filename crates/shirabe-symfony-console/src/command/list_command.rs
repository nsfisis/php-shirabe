//! ref: composer/vendor/symfony/console/Command/ListCommand.php sha1:89f524e92adad752f60a172c332ca10a2ea2256f

use crate::command::{Command, CommandData, SetDefinitionArg};
use crate::completion::{CompletionInput, CompletionSuggestions, StringOrSuggestion};
use crate::delegate_command_trait_impls_to_inner;
use crate::descriptor::{ApplicationDescription, DescribableObject, DescriptorOptions};
use crate::helper::DescriptorHelper;
use crate::input::{
    DefinitionItem, InputArgument, InputInterface, InputInterfaceByName, InputOption, InputValue,
};
use crate::output::OutputInterface;
use shirabe_php_shim::{ToPhpString, impl_php_class};

/// ListCommand displays the list of all available commands for the application.
#[derive(Debug)]
pub struct ListCommand {
    inner: CommandData,
}

impl_php_class!(
    ListCommand,
    r"Symfony\Component\Console\Command\ListCommand"
);

impl std::ops::Deref for ListCommand {
    type Target = CommandData;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for ListCommand {
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

    pub fn complete_impl(
        &self,
        input: &CompletionInput,
        suggestions: &mut CompletionSuggestions,
    ) -> anyhow::Result<()> {
        if input.must_suggest_argument_values_for("namespace") {
            let application = self.get_application().unwrap();
            let mut descriptor = ApplicationDescription::new(application, None, false);
            suggestions.suggest_values(
                descriptor
                    .get_namespaces()?
                    .keys()
                    .cloned()
                    .map(StringOrSuggestion::String)
                    .collect(),
            );

            return Ok(());
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

        Ok(())
    }
}

#[async_trait::async_trait(?Send)]
impl Command for ListCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.inner.set_name("list".into())?;
        self.inner.set_definition(SetDefinitionArg::Array(vec![
            DefinitionItem::InputArgument(InputArgument::new(
                "namespace",
                Some(InputArgument::OPTIONAL),
                "The namespace name",
                InputValue::Null,
            )?),
            DefinitionItem::InputOption(InputOption::new(
                "raw",
                None,
                Some(InputOption::VALUE_NONE),
                "To output raw command list",
                InputValue::Null,
            )?),
            DefinitionItem::InputOption(InputOption::new(
                "format",
                None,
                Some(InputOption::VALUE_REQUIRED),
                "The output format (txt, xml, json, or md)",
                InputValue::from("txt"),
            )?),
            DefinitionItem::InputOption(InputOption::new(
                "short",
                None,
                Some(InputOption::VALUE_NONE),
                "To skip describing commands' arguments",
                InputValue::Null,
            )?),
        ]));
        self.inner.set_description("List commands".into());
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
                 \x20\x20<info>%command.full_name% --raw</info>".into(),
            );

        Ok(())
    }

    async fn execute(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let mut helper = DescriptorHelper::new();
        let object = DescribableObject::Application(self.get_application().unwrap());
        let options = DescriptorOptions {
            format: input.borrow().get_option("format")?.to_php_string(),
            raw_text: input.borrow().get_option("raw")?.to_bool(),
            namespace: match input.borrow().get_argument("namespace")? {
                InputValue::Null => None,
                InputValue::String(namespace) => Some(namespace),
                other => panic!("the namespace argument holds {:?}, not a string", other),
            },
            short: input.borrow().get_option("short")?.to_bool(),
            ..DescriptorOptions::default()
        };
        helper.describe(output.clone(), object, options)?;

        Ok(0)
    }

    fn complete(
        &self,
        input: &CompletionInput,
        suggestions: &mut CompletionSuggestions,
    ) -> anyhow::Result<()> {
        self.complete_impl(input, suggestions)
    }

    delegate_command_trait_impls_to_inner!(inner);
}
