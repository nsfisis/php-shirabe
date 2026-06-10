//! ref: composer/src/Composer/Command/ClearCacheCommand.php

use crate::command::{BaseCommand, BaseCommandData, HasBaseCommandData};
use crate::composer;
use crate::composer::ComposerHandle;
use crate::console::input::InputOption;
use crate::factory::Factory;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;

#[derive(Debug)]
pub struct ClearCacheCommand {
    base_command_data: BaseCommandData,
}

impl ClearCacheCommand {
    pub fn configure(&mut self) {
        self.set_name("clear-cache");
        self.set_aliases(&["clearcache".to_string(), "cc".to_string()]);
        self.set_description("Clears composer's internal package cache");
        self.set_definition(&[InputOption::new(
            "gc",
            None,
            Some(InputOption::VALUE_NONE),
            "Only run garbage collection, not a full cache clear",
            None,
        )
        .unwrap()
        .into()]);
        self.set_help(
            "The <info>clear-cache</info> deletes all cached packages from composer's\n\
            cache directory.\n\n\
            Read more at https://getcomposer.org/doc/03-cli.md#clear-cache-clearcache-cc",
        );
    }

    pub fn execute(
        &mut self,
        _input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        _output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        // PHP: $config = $this->tryComposer()?->getConfig() ?? Factory::createConfig(); then
        // iterate the cache-* paths and clear/gc each via Cache.
        // TODO(phase-c): two blockers. (1) The first statement calls self.try_composer(), which is
        // a deferred todo!() (it needs get_application() -> the Symfony command registry). (2) The
        // two config sources differ in ownership — composer.get_config() is Rc<RefCell<Config>>
        // while Factory::create_config() returns an owned Config — so a shared Config model is
        // needed before the per-path Cache logic can read both uniformly.
        let _ = composer::VERSION;
        let _: IndexMap<String, String> = IndexMap::new();
        let _ = Factory::create_config(None, None);
        todo!("ClearCacheCommand::execute pending try_composer + Config sharing model")
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
