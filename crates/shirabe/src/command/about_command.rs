//! ref: composer/src/Composer/Command/AboutCommand.php

use crate::command::BaseCommand;
use crate::command::BaseCommandData;
use crate::command::base_command::base_command_initialize;
use crate::composer;
use shirabe_external_packages::symfony::console::command::command::Command;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug)]
pub struct AboutCommand {
    base_command_data: BaseCommandData,
}

impl Default for AboutCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl AboutCommand {
    pub fn new() -> Self {
        let command = AboutCommand {
            base_command_data: BaseCommandData::new(None),
        };
        command
            .configure()
            .expect("AboutCommand::configure uses static, valid metadata");
        command
    }
}

impl Command for AboutCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.set_name("about")?;
        self.set_description("Shows a short information about Composer");
        self.set_help("<info>shirabe about</info>");
        Ok(())
    }

    fn execute(
        &self,
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
        &self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        base_command_initialize(self, input, output)
    }

    shirabe_external_packages::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for AboutCommand {
    fn command_data(
        &self,
    ) -> &shirabe_external_packages::symfony::console::command::command::CommandData {
        self.base_command_data.command_data()
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}
