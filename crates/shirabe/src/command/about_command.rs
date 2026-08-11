//! ref: composer/src/Composer/Command/AboutCommand.php

use crate::command::BaseCommand;
use crate::command::BaseCommandData;
use crate::command::base_command::base_command_initialize;
use crate::composer;
use shirabe_php_shim::impl_php_class;
use shirabe_symfony_console::command::Command;
use shirabe_symfony_console::input::InputInterface;
use shirabe_symfony_console::output::OutputInterface;

#[derive(Debug)]
pub struct AboutCommand {
    base_command_data: BaseCommandData,
}

impl_php_class!(AboutCommand, r"Composer\Command\AboutCommand");

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
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let shirabe_version = composer::SHIRABE_VERSION;
        let composer_version = composer::VERSION;
        let _ = (input, output);

        self.get_io().borrow().write(&format!(
            "<info>Shirabe - Dependency Manager for PHP - version {shirabe_version} (based on Composer {composer_version})</info>\n\
            <comment>Shirabe is a dependency manager tracking local dependencies of your projects and libraries.\n\
            See https://getcomposer.org/ for more information.</comment>"
        ));

        Ok(0)
    }

    fn initialize(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        base_command_initialize(self, input, output)
    }

    fn complete(
        &self,
        input: &shirabe_symfony_console::completion::CompletionInput,
        suggestions: &mut shirabe_symfony_console::completion::CompletionSuggestions,
    ) -> anyhow::Result<()> {
        crate::command::base_command::base_command_complete(self, input, suggestions)
    }

    shirabe_symfony_console::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for AboutCommand {
    fn base_command_data(&self) -> &crate::command::BaseCommandData {
        &self.base_command_data
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}
