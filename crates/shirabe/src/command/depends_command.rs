//! ref: composer/src/Composer/Command/DependsCommand.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::console::command::command::Command;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use shirabe_php_shim::PhpMixed;
use std::cell::RefCell;
use std::rc::Rc;

use crate::advisory::AuditConfig;
use crate::command::BaseCommand;
use crate::command::BaseCommandData;
use crate::command::BaseDependencyCommand;
use crate::command::base_command::base_command_initialize;
use crate::composer::PartialComposerHandle;
use crate::config::Config;
use crate::console::input::InputArgument;
use crate::console::input::InputOption;
use crate::filter::platform_requirement_filter::PlatformRequirementFilterInterface;
use crate::io::IOInterface;

#[derive(Debug)]
pub struct DependsCommand {
    base_command_data: BaseCommandData,

    colors: Vec<String>,
}

impl DependsCommand {
    pub fn new() -> Self {
        let mut command = DependsCommand {
            base_command_data: BaseCommandData::new(None),
            colors: Vec::new(),
        };
        command
            .configure()
            .expect("DependsCommand::configure uses static, valid metadata");
        command
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

impl Command for DependsCommand {
    fn configure(&mut self) -> anyhow::Result<()> {
        // TODO(cli-completion): suggest_installed_package(true, true) for `package` argument
        self.set_name("depends")?;
        self.set_aliases(vec!["why".to_string()])?;
        self.set_description("Shows which packages cause the given package to be installed");
        self.set_definition(&[
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
        ]);
        self.set_help(
            "Displays detailed information about where a package is referenced.\n\n\
            <info>php composer.phar depends composer/composer</info>\n\n\
            Read more at https://getcomposer.org/doc/03-cli.md#depends-why",
        );
        Ok(())
    }

    fn execute(
        &mut self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        self.do_execute(input, output, false)
    }

    fn initialize(
        &mut self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        base_command_initialize(self, input, output)
    }

    shirabe_external_packages::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for DependsCommand {
    fn command_data_mut(
        &mut self,
    ) -> &mut shirabe_external_packages::symfony::console::command::command::CommandData {
        self.base_command_data.command_data_mut()
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}
