//! ref: composer/src/Composer/Command/ClearCacheCommand.php

use crate::command::{BaseCommand, BaseCommandData, HasBaseCommandData};
use crate::composer;
use crate::composer::ComposerHandle;
use crate::factory::Factory;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::component::console::input::InputInterface;
use shirabe_external_packages::symfony::component::console::output::OutputInterface;

#[derive(Debug)]
pub struct ClearCacheCommand {
    base_command_data: BaseCommandData,
}

impl ClearCacheCommand {
    pub fn configure(&mut self) {
        self.set_name("clear-cache");
        self.set_aliases(&["clearcache".to_string(), "cc".to_string()]);
        self.set_description("Clears composer's internal package cache");
        // TODO(phase-b): set_definition requires Vec<Box<dyn InputDefinitionEntry>>
        // self.set_definition(...) — InputOption::new arg shapes do not yet match
        self.set_help(
            "The <info>clear-cache</info> deletes all cached packages from composer's\n\
            cache directory.\n\n\
            Read more at https://getcomposer.org/doc/03-cli.md#clear-cache-clearcache-cc",
        );
    }

    pub fn execute(
        &mut self,
        _input: &dyn InputInterface,
        _output: &dyn OutputInterface,
    ) -> anyhow::Result<i64> {
        // TODO(phase-b): port full execute logic once Config sharing model is settled
        let _ = composer::VERSION;
        let _: IndexMap<String, String> = IndexMap::new();
        let _ = Factory::create_config(None, None);
        todo!("phase-b: ClearCacheCommand::execute requires Config sharing strategy")
    }
}

impl HasBaseCommandData for ClearCacheCommand {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    fn base_command_data_mut(&mut self) -> &mut BaseCommandData {
        &mut self.base_command_data
    }
}
