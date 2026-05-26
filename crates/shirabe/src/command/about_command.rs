//! ref: composer/src/Composer/Command/AboutCommand.php

use crate::command::BaseCommand;
use crate::command::BaseCommandData;
use crate::command::HasBaseCommandData;
use crate::composer;
use crate::composer::ComposerHandle;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use shirabe_external_packages::symfony::component::console::input::InputInterface;
use shirabe_external_packages::symfony::component::console::output::OutputInterface;

#[derive(Debug)]
pub struct AboutCommand {
    base_command_data: BaseCommandData,
}

impl AboutCommand {
    pub fn configure(&mut self) {
        self.set_name("about")
            .set_description("Shows a short information about Composer")
            .set_help("<info>php composer.phar about</info>");
    }

    pub fn execute(&mut self, input: &dyn InputInterface, output: &dyn OutputInterface) -> i64 {
        let composer_version = composer::get_version();
        let _ = (input, output);

        self.get_io().write(&format!(
            "<info>Composer - Dependency Manager for PHP - version {composer_version}</info>\n\
            <comment>Composer is a dependency manager tracking local dependencies of your projects and libraries.\n\
            See https://getcomposer.org/ for more information.</comment>"
        ));

        0
    }
}

impl HasBaseCommandData for AboutCommand {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    fn base_command_data_mut(&mut self) -> &mut BaseCommandData {
        &mut self.base_command_data
    }
}
