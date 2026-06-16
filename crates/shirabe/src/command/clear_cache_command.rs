//! ref: composer/src/Composer/Command/ClearCacheCommand.php

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
use crate::console::input::InputOption;
use crate::factory::Factory;
use crate::filter::platform_requirement_filter::PlatformRequirementFilterInterface;
use crate::io::IOInterface;

#[derive(Debug)]
pub struct ClearCacheCommand {
    base_command_data: BaseCommandData,
}

impl ClearCacheCommand {
    pub fn new() -> Self {
        let mut command = ClearCacheCommand {
            base_command_data: BaseCommandData::new(None),
        };
        command
            .configure()
            .expect("ClearCacheCommand::configure uses static, valid metadata");
        command
    }
}

impl Command for ClearCacheCommand {
    fn configure(&mut self) -> anyhow::Result<()> {
        self.set_name("clear-cache")?;
        self.set_aliases(vec!["clearcache".to_string(), "cc".to_string()])?;
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
        Ok(())
    }

    fn execute(
        &mut self,
        _input: Rc<RefCell<dyn InputInterface>>,
        _output: Rc<RefCell<dyn OutputInterface>>,
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

    fn initialize(
        &mut self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        base_command_initialize(self, input, output)
    }

    shirabe_external_packages::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for ClearCacheCommand {
    fn command_data_mut(
        &mut self,
    ) -> &mut shirabe_external_packages::symfony::console::command::command::CommandData {
        self.base_command_data.command_data_mut()
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}
