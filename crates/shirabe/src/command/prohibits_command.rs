//! ref: composer/src/Composer/Command/ProhibitsCommand.php

use crate::command::BaseDependencyCommand;
use crate::command::CompletionTrait;
use crate::command::base_command::base_command_initialize;
use crate::command::{BaseCommand, BaseCommandData};
use crate::console::input::InputArgument;
use crate::console::input::InputOption;
use shirabe_php_shim::impl_php_class;
use shirabe_symfony_console::command::Command;
use shirabe_symfony_console::input::InputInterface;
use shirabe_symfony_console::output::OutputInterface;

#[derive(Debug)]
pub struct ProhibitsCommand {
    base_command_data: BaseCommandData,

    colors: std::cell::RefCell<Vec<String>>,
}

impl_php_class!(ProhibitsCommand, r"Composer\Command\ProhibitsCommand");

impl Default for ProhibitsCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl ProhibitsCommand {
    pub fn new() -> Self {
        let command = ProhibitsCommand {
            base_command_data: BaseCommandData::new(None),
            colors: std::cell::RefCell::new(Vec::new()),
        };
        command
            .configure()
            .expect("ProhibitsCommand::configure uses static, valid metadata");
        command
    }
}

impl BaseDependencyCommand for ProhibitsCommand {
    fn colors(&self) -> std::cell::Ref<'_, Vec<String>> {
        self.colors.borrow()
    }

    fn set_colors(&self, colors: Vec<String>) {
        *self.colors.borrow_mut() = colors;
    }
}

impl Command for ProhibitsCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.set_name("prohibits")?;
        self.set_aliases(vec!["why-not".to_string()])?;
        self.set_description("Shows which packages prevent the given package from being installed");
        self.set_definition(&[
            InputArgument::new5(
                <Self as BaseDependencyCommand>::ARGUMENT_PACKAGE,
                Some(InputArgument::REQUIRED),
                "Package to inspect",
                None,
                self.suggest_available_package(99),
            )
            .unwrap()
            .into(),
            InputArgument::new(
                <Self as BaseDependencyCommand>::ARGUMENT_CONSTRAINT,
                Some(InputArgument::REQUIRED),
                "Version constraint, which version you expected to be installed",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new(
                <Self as BaseDependencyCommand>::OPTION_RECURSIVE,
                Some("r"),
                Some(InputOption::VALUE_NONE),
                "Recursively resolves up to the root package",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new(
                <Self as BaseDependencyCommand>::OPTION_TREE,
                Some("t"),
                Some(InputOption::VALUE_NONE),
                "Prints the results as a nested tree",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new(
                "locked",
                None,
                Some(InputOption::VALUE_NONE),
                "Read dependency information from composer.lock",
                None,
            )
            .unwrap()
            .into(),
        ]);
        self.set_help(
            "Displays detailed information about why a package cannot be installed.\n\n\
            <info>shirabe prohibits composer/composer</info>\n\n\
            Read more at https://getcomposer.org/doc/03-cli.md#prohibits-why-not",
        );
        Ok(())
    }

    fn execute(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        self.do_execute(input, output, true)
    }

    fn initialize(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        base_command_initialize(self, input, output)
    }

    fn complete(
        &self,
        input: &shirabe_symfony_console::completion::CompletionInput,
        suggestions: &mut shirabe_symfony_console::completion::CompletionSuggestions,
    ) -> anyhow::Result<()> {
        crate::command::base_command::base_command_complete(self, input, suggestions)
    }

    shirabe_symfony_console::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for ProhibitsCommand {
    fn base_command_data(&self) -> &crate::command::BaseCommandData {
        &self.base_command_data
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}
