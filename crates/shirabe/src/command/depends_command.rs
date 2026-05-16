//! ref: composer/src/Composer/Command/DependsCommand.php

use crate::command::base_dependency_command::BaseDependencyCommand;
use crate::command::completion_trait::CompletionTrait;
use crate::console::input::input_argument::InputArgument;
use crate::console::input::input_option::InputOption;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;

pub struct DependsCommand {
    inner: BaseDependencyCommand,
}

impl CompletionTrait for DependsCommand {}

impl DependsCommand {
    pub fn configure(&mut self) {
        let package_suggestions = self.suggest_installed_package(true, true);
        self.inner
            .set_name("depends")
            .set_aliases(vec!["why".to_string()])
            .set_description("Shows which packages cause the given package to be installed")
            .set_definition(vec![
                InputArgument::new(
                    BaseDependencyCommand::ARGUMENT_PACKAGE,
                    InputArgument::REQUIRED,
                    "Package to inspect",
                    None,
                    package_suggestions,
                ),
                InputOption::new(
                    BaseDependencyCommand::OPTION_RECURSIVE,
                    Some("r"),
                    InputOption::VALUE_NONE,
                    "Recursively resolves up to the root package",
                ),
                InputOption::new(
                    BaseDependencyCommand::OPTION_TREE,
                    Some("t"),
                    InputOption::VALUE_NONE,
                    "Prints the results as a nested tree",
                ),
                InputOption::new(
                    "locked",
                    None,
                    InputOption::VALUE_NONE,
                    "Read dependency information from composer.lock",
                ),
            ])
            .set_help(
                "Displays detailed information about where a package is referenced.\n\n\
                <info>php composer.phar depends composer/composer</info>\n\n\
                Read more at https://getcomposer.org/doc/03-cli.md#depends-why",
            );
    }

    pub fn execute(&self, input: &dyn InputInterface, output: &dyn OutputInterface) -> i64 {
        self.inner.do_execute(input, output)
    }
}
