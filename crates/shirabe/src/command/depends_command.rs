//! ref: composer/src/Composer/Command/DependsCommand.php

use crate::command::BaseDependencyCommand;
use crate::command::{BaseCommand, BaseCommandData, HasBaseCommandData};
use crate::console::input::InputArgument;
use crate::console::input::InputOption;
use crate::io::IOInterface;
use shirabe_external_packages::symfony::component::console::input::InputInterface;
use shirabe_external_packages::symfony::component::console::output::OutputInterface;

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
                    crate::command::ARGUMENT_PACKAGE,
                    Some(InputArgument::REQUIRED),
                    "Package to inspect",
                    None,
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
            ])
            .set_help(
                "Displays detailed information about where a package is referenced.\n\n\
                <info>php composer.phar depends composer/composer</info>\n\n\
                Read more at https://getcomposer.org/doc/03-cli.md#depends-why",
            );
    }

    pub fn execute(
        &mut self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        self.do_execute(input, output, false)
    }
}

impl BaseDependencyCommand for DependsCommand {
    fn colors(&self) -> &[String] {
        &self.colors
    }

    fn colors_mut(&mut self) -> &mut Vec<String> {
        &mut self.colors
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
