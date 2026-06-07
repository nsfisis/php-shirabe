//! ref: composer/src/Composer/Command/ProhibitsCommand.php

use crate::command::BaseDependencyCommand;
use crate::command::{BaseCommand, BaseCommandData, HasBaseCommandData};
use crate::console::input::InputArgument;
use crate::console::input::InputOption;
use crate::io::IOInterface;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;

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

    pub fn execute(
        &mut self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        self.do_execute(input, output, true)
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
