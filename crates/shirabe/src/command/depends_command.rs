//! ref: composer/src/Composer/Command/DependsCommand.php

use crate::command::base_command::{BaseCommand, BaseCommandData, HasBaseCommandData};
use crate::command::base_dependency_command::BaseDependencyCommand;
use crate::console::input::input_argument::InputArgument;
use crate::console::input::input_option::InputOption;
use crate::io::io_interface::IOInterface;
use shirabe_external_packages::symfony::component::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::component::console::output::output_interface::OutputInterface;

#[derive(Debug)]
pub struct DependsCommand {
    base_command_data: BaseCommandData,

    colors: Vec<String>,
}

impl DependsCommand {
    pub fn configure(&mut self) {
        // TODO(cli-completion): suggest_installed_package(true, true) for `package` argument
        self.set_name("depends")
            .set_aliases(&["why".to_string()])
            .set_description("Shows which packages cause the given package to be installed")
            .set_definition(&[
                InputArgument::new(
                    crate::command::base_dependency_command::ARGUMENT_PACKAGE,
                    Some(InputArgument::REQUIRED),
                    "Package to inspect",
                    None,
                )
                .unwrap()
                .into(),
                InputOption::new(
                    crate::command::base_dependency_command::OPTION_RECURSIVE,
                    Some(shirabe_php_shim::PhpMixed::String("r".to_string())),
                    Some(InputOption::VALUE_NONE),
                    "Recursively resolves up to the root package",
                    None,
                )
                .unwrap()
                .into(),
                InputOption::new(
                    crate::command::base_dependency_command::OPTION_TREE,
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
            ])
            .set_help(
                "Displays detailed information about where a package is referenced.\n\n\
                <info>php composer.phar depends composer/composer</info>\n\n\
                Read more at https://getcomposer.org/doc/03-cli.md#depends-why",
            );
    }

    pub fn execute(&mut self, input: &dyn InputInterface, output: &dyn OutputInterface) -> i64 {
        // TODO(phase-b): wire `do_execute` from BaseDependencyCommand trait without conflicting with
        // BaseCommand blanket impl
        let _ = (input, output);
        todo!()
    }
}

impl HasBaseCommandData for DependsCommand {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    fn base_command_data_mut(&mut self) -> &mut BaseCommandData {
        &mut self.base_command_data
    }
}
