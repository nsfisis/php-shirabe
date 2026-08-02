//! ref: composer/src/Composer/Command/DependsCommand.php

use crate::command::BaseCommand;
use crate::command::BaseCommandData;
use crate::command::BaseDependencyCommand;
use crate::command::CompletionTrait;
use crate::command::base_command::base_command_initialize;
use crate::console::input::InputArgument;
use crate::console::input::InputOption;
use shirabe_external_packages::symfony::console::command::command::Command;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;

#[derive(Debug)]
pub struct DependsCommand {
    base_command_data: BaseCommandData,

    colors: std::cell::RefCell<Vec<String>>,
}

impl Default for DependsCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl DependsCommand {
    pub fn new() -> Self {
        let command = DependsCommand {
            base_command_data: BaseCommandData::new(None),
            colors: std::cell::RefCell::new(Vec::new()),
        };
        command
            .configure()
            .expect("DependsCommand::configure uses static, valid metadata");
        command
    }
}

impl BaseDependencyCommand for DependsCommand {
    fn colors(&self) -> std::cell::Ref<'_, Vec<String>> {
        self.colors.borrow()
    }

    fn set_colors(&self, colors: Vec<String>) {
        *self.colors.borrow_mut() = colors;
    }
}

impl Command for DependsCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.set_name("depends")?;
        self.set_aliases(vec!["why".to_string()])?;
        self.set_description("Shows which packages cause the given package to be installed");
        self.set_definition(&[
            InputArgument::new5(
                crate::command::ARGUMENT_PACKAGE,
                Some(InputArgument::REQUIRED),
                "Package to inspect",
                None,
                self.suggest_installed_package(true, true),
            )
            .unwrap()
            .into(),
            InputOption::new(
                crate::command::OPTION_RECURSIVE,
                Some(shirabe_php_shim::PhpMixed::String("r".to_string())),
                Some(InputOption::VALUE_NONE),
                "Recursively resolves up to the root package",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new(
                crate::command::OPTION_TREE,
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
            "Displays detailed information about where a package is referenced.\n\n\
            <info>shirabe depends composer/composer</info>\n\n\
            Read more at https://getcomposer.org/doc/03-cli.md#depends-why",
        );
        Ok(())
    }

    fn execute(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        self.do_execute(input, output, false)
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
        input: &shirabe_external_packages::symfony::console::completion::completion_input::CompletionInput,
        suggestions: &mut shirabe_external_packages::symfony::console::completion::completion_suggestions::CompletionSuggestions,
    ) -> anyhow::Result<()> {
        crate::command::base_command::base_command_complete(self, input, suggestions)
    }

    shirabe_external_packages::delegate_command_trait_impls_to_inner!(
        base_command_data,
        "Composer\\Command\\DependsCommand"
    );
}

impl BaseCommand for DependsCommand {
    fn base_command_data(&self) -> &crate::command::BaseCommandData {
        &self.base_command_data
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}
