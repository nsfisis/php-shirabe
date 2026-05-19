//! ref: composer/src/Composer/Command/ProhibitsCommand.php

use crate::command::base_command::{BaseCommand, BaseCommandData, HasBaseCommandData};
use crate::command::base_dependency_command::BaseDependencyCommand;
use crate::console::input::input_argument::InputArgument;
use crate::console::input::input_option::InputOption;
use crate::io::io_interface::IOInterface;
use shirabe_external_packages::symfony::component::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::component::console::output::output_interface::OutputInterface;

#[derive(Debug)]
pub struct ProhibitsCommand {
    base_command_data: BaseCommandData,

    colors: Vec<String>,
}

impl ProhibitsCommand {
    pub fn configure(&mut self) {
        // TODO(cli-completion): suggest_available_package() for `package` argument
        self.set_name("prohibits")
            .set_aliases(&["why-not".to_string()])
            .set_description("Shows which packages prevent the given package from being installed")
            .set_definition(&[
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
            ])
            .set_help(
                "Displays detailed information about why a package cannot be installed.\n\n\
                <info>php composer.phar prohibits composer/composer</info>\n\n\
                Read more at https://getcomposer.org/doc/03-cli.md#prohibits-why-not",
            );
    }

    pub fn execute(&mut self, input: &dyn InputInterface, output: &dyn OutputInterface) -> i64 {
        // TODO(phase-b): wire `do_execute` from BaseDependencyCommand trait
        let _ = (input, output);
        todo!()
    }
}

impl BaseDependencyCommand for ProhibitsCommand {
    fn colors(&self) -> &[String] {
        &self.colors
    }

    fn colors_mut(&mut self) -> &mut Vec<String> {
        &mut self.colors
    }
}

impl HasBaseCommandData for ProhibitsCommand {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    fn base_command_data_mut(&mut self) -> &mut BaseCommandData {
        &mut self.base_command_data
    }
}
