//! ref: composer/src/Composer/Command/AboutCommand.php

use crate::command::base_command::BaseCommand;
use crate::composer::Composer;
use crate::io::io_interface::IOInterface;
use shirabe_external_packages::symfony::component::console::command::command::Command;
use shirabe_external_packages::symfony::component::console::command::command::CommandBase;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;

#[derive(Debug)]
pub struct AboutCommand {
    inner: CommandBase,
    composer: Option<Composer>,
    io: Option<Box<dyn IOInterface>>,
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

impl BaseCommand for AboutCommand {
    fn inner(&self) -> &CommandBase {
        &self.inner
    }

    fn inner_mut(&mut self) -> &mut CommandBase {
        &mut self.inner
    }

    fn composer(&self) -> Option<&Composer> {
        self.composer.as_ref()
    }

    fn composer_mut(&mut self) -> &mut Option<Composer> {
        &mut self.composer
    }

    fn io(&self) -> Option<&dyn IOInterface> {
        self.io.as_deref()
    }

    fn io_mut(&mut self) -> &mut Option<Box<dyn IOInterface>> {
        &mut self.io
    }
}

impl Command for AboutCommand {}
