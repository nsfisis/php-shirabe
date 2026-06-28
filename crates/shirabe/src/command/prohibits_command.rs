//! ref: composer/src/Composer/Command/ProhibitsCommand.php

use crate::command::BaseDependencyCommand;
use crate::command::base_command::base_command_initialize;
use crate::command::{BaseCommand, BaseCommandData};
use crate::console::input::InputArgument;
use crate::console::input::InputOption;
use anyhow::Result;
use shirabe_external_packages::symfony::console::command::command::Command;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug)]
pub struct ProhibitsCommand {
    base_command_data: BaseCommandData,

    colors: std::cell::RefCell<Vec<String>>,
}

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
        // TODO(cli-completion): suggest_available_package() for `package` argument
        self.set_name("prohibits")?;
        self.set_aliases(vec!["why-not".to_string()])?;
        self.set_description("Shows which packages prevent the given package from being installed");
        self.set_definition(&[
            InputArgument::new(
                <Self as BaseDependencyCommand>::ARGUMENT_PACKAGE,
                Some(InputArgument::REQUIRED),
                "Package to inspect",
                None,
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
                Some(shirabe_php_shim::PhpMixed::String("r".to_string())),
                Some(InputOption::VALUE_NONE),
                "Recursively resolves up to the root package",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new(
                <Self as BaseDependencyCommand>::OPTION_TREE,
                Some(shirabe_php_shim::PhpMixed::String("t".to_string())),
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
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        self.do_execute(input, output, true)
    }

    fn initialize(
        &self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        base_command_initialize(self, input, output)
    }

    shirabe_external_packages::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for ProhibitsCommand {
    fn command_data(
        &self,
    ) -> &shirabe_external_packages::symfony::console::command::command::CommandData {
        self.base_command_data.command_data()
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}
