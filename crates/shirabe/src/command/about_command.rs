//! ref: composer/src/Composer/Command/AboutCommand.php

use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;
use crate::command::base_command::BaseCommand;
use crate::composer::Composer;

pub struct AboutCommand {
    inner: BaseCommand,
}

impl AboutCommand {
    pub fn configure(&mut self) {
        self.inner
            .set_name("about")
            .set_description("Shows a short information about Composer")
            .set_help("<info>php composer.phar about</info>");
    }

    pub fn execute(&self, input: &dyn InputInterface, output: &dyn OutputInterface) -> i64 {
        let composer_version = Composer::get_version();

        self.inner.get_io().write(&format!(
            "<info>Composer - Dependency Manager for PHP - version {composer_version}</info>\n\
            <comment>Composer is a dependency manager tracking local dependencies of your projects and libraries.\n\
            See https://getcomposer.org/ for more information.</comment>"
        ));

        0
    }
}
