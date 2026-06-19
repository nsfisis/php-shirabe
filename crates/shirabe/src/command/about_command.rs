//! ref: composer/src/Composer/Command/AboutCommand.php

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
use crate::command::base_command::base_command_initialize;
use crate::composer;
use crate::composer::PartialComposerHandle;
use crate::config::Config;
use crate::filter::platform_requirement_filter::PlatformRequirementFilterInterface;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;

#[derive(Debug)]
pub struct AboutCommand {
    base_command_data: BaseCommandData,
}

impl AboutCommand {
    pub fn new() -> Self {
        let mut command = AboutCommand {
            base_command_data: BaseCommandData::new(None),
        };
        command
            .configure()
            .expect("AboutCommand::configure uses static, valid metadata");
        command
    }
}

impl Command for AboutCommand {
    fn configure(&mut self) -> anyhow::Result<()> {
        self.set_name("about")?;
        self.set_description("Shows a short information about Composer");
        self.set_help("<info>shirabe about</info>");
        Ok(())
    }

    fn execute(
        &mut self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let composer_version = composer::get_version();
        let _ = (input, output);

        self.get_io().borrow().write(&format!(
            "<info>Composer - Dependency Manager for PHP - version {composer_version}</info>\n\
            <comment>Composer is a dependency manager tracking local dependencies of your projects and libraries.\n\
            See https://getcomposer.org/ for more information.</comment>"
        ));

        Ok(0)
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

impl BaseCommand for AboutCommand {
    fn command_data_mut(
        &mut self,
    ) -> &mut shirabe_external_packages::symfony::console::command::command::CommandData {
        self.base_command_data.command_data_mut()
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}
