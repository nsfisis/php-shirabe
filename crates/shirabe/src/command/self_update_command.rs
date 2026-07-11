//! ref: composer/src/Composer/Command/SelfUpdateCommand.php

use crate::command::BaseCommand;
use crate::command::BaseCommandData;
use crate::command::base_command::base_command_initialize;
use crate::console::input::InputArgument;
use crate::console::input::InputOption;
use crate::io::IOInterfaceImmutable;
use crate::io::io_interface;
use shirabe_external_packages::symfony::console::command::command::Command;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct SelfUpdateCommand {
    base_command_data: BaseCommandData,
}

impl Default for SelfUpdateCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl SelfUpdateCommand {
    pub fn new() -> Self {
        let command = SelfUpdateCommand {
            base_command_data: BaseCommandData::new(None),
        };
        command
            .configure()
            .expect("SelfUpdateCommand::configure uses static, valid metadata");
        command
    }
}

impl Command for SelfUpdateCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.set_name("self-update")?;
        self.set_aliases(vec!["selfupdate".to_string()])?;
        self.set_description("Updates composer.phar to the latest version");
        self.set_definition(&[
            InputOption::new("rollback", Some(PhpMixed::String("r".to_string())), Some(InputOption::VALUE_NONE), "Revert to an older installation of composer", None).unwrap().into(),
            InputOption::new("clean-backups", None, Some(InputOption::VALUE_NONE), "Delete old backups during an update. This makes the current version of composer the only backup available after the update", None).unwrap().into(),
            InputArgument::new("version", Some(InputArgument::OPTIONAL), "The version to update to", None).unwrap().into(),
            InputOption::new("no-progress", None, Some(InputOption::VALUE_NONE), "Do not output download progress.", None).unwrap().into(),
            InputOption::new("update-keys", None, Some(InputOption::VALUE_NONE), "Prompt user for a key update", None).unwrap().into(),
            InputOption::new("stable", None, Some(InputOption::VALUE_NONE), "Force an update to the stable channel", None).unwrap().into(),
            InputOption::new("preview", None, Some(InputOption::VALUE_NONE), "Force an update to the preview channel", None).unwrap().into(),
            InputOption::new("snapshot", None, Some(InputOption::VALUE_NONE), "Force an update to the snapshot channel", None).unwrap().into(),
            InputOption::new("1", None, Some(InputOption::VALUE_NONE), "Force an update to the stable channel, but only use 1.x versions", None).unwrap().into(),
            InputOption::new("2", None, Some(InputOption::VALUE_NONE), "Force an update to the stable channel, but only use 2.x versions", None).unwrap().into(),
            InputOption::new("2.2", None, Some(InputOption::VALUE_NONE), "Force an update to the stable channel, but only use 2.2.x LTS versions", None).unwrap().into(),
            InputOption::new("set-channel-only", None, Some(InputOption::VALUE_NONE), "Only store the channel as the default one and then exit", None).unwrap().into(),
        ]);
        self.set_help(
            "The <info>self-update</info> command checks getcomposer.org for newer\n\
            versions of composer and if found, installs the latest.\n\
            \n\
            <info>shirabe self-update</info>\n\
            \n\
            Read more at https://getcomposer.org/doc/03-cli.md#self-update-selfupdate",
        );
        Ok(())
    }

    /// The self-update mechanism does not apply to Shirabe: the update flow
    /// differs fundamentally from the PHP phar-based one, and no release has
    /// been published yet. The command is therefore disabled.
    fn execute(
        &self,
        _input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        _output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let io = self.get_io();
        io.write_error3(
            "<error>The self-update command is not available in Shirabe.</error>",
            true,
            io_interface::NORMAL,
        );

        Ok(1)
    }

    fn initialize(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        base_command_initialize(self, input, output)
    }

    shirabe_external_packages::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for SelfUpdateCommand {
    fn command_data(
        &self,
    ) -> &shirabe_external_packages::symfony::console::command::command::CommandData {
        self.base_command_data.command_data()
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);

    fn is_self_update_command(&self) -> bool {
        true
    }
}
