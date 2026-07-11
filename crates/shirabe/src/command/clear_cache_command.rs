//! ref: composer/src/Composer/Command/ClearCacheCommand.php

use crate::cache::Cache;
use crate::command::BaseCommand;
use crate::command::BaseCommandData;
use crate::command::base_command::base_command_initialize;
use crate::config::Config;
use crate::console::input::InputOption;
use crate::factory::Factory;
use crate::io::IOInterfaceImmutable;
use shirabe_external_packages::symfony::console::command::command::Command;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use shirabe_php_shim::realpath;

#[derive(Debug)]
pub struct ClearCacheCommand {
    base_command_data: BaseCommandData,
}

impl Default for ClearCacheCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl ClearCacheCommand {
    pub fn new() -> Self {
        let command = ClearCacheCommand {
            base_command_data: BaseCommandData::new(None),
        };
        command
            .configure()
            .expect("ClearCacheCommand::configure uses static, valid metadata");
        command
    }
}

impl Command for ClearCacheCommand {
    fn configure(&self) -> anyhow::Result<()> {
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
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        _output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let composer = self.try_composer(None, None);
        // composer.getConfig() yields an Rc<RefCell<Config>>; Factory::createConfig() an owned
        // Config. Keep whichever applies alive in a local and read both through a shared &Config.
        let config_rc = composer.as_ref().map(|c| c.borrow_partial().get_config());
        let created_config = if config_rc.is_none() {
            Some(Factory::create_config(None, None)?)
        } else {
            None
        };
        let config_guard = config_rc.as_ref().map(|rc| rc.borrow());
        let config: &Config = match (config_guard.as_deref(), created_config.as_ref()) {
            (Some(config), _) => config,
            (None, Some(config)) => config,
            (None, None) => unreachable!(),
        };

        let io = self.get_io();

        let gc = input.borrow().get_option("gc")?.as_bool().unwrap_or(false);

        let cache_paths: [(&str, String); 4] = [
            (
                "cache-vcs-dir",
                config
                    .get("cache-vcs-dir")
                    .as_string()
                    .unwrap_or("")
                    .to_string(),
            ),
            (
                "cache-repo-dir",
                config
                    .get("cache-repo-dir")
                    .as_string()
                    .unwrap_or("")
                    .to_string(),
            ),
            (
                "cache-files-dir",
                config
                    .get("cache-files-dir")
                    .as_string()
                    .unwrap_or("")
                    .to_string(),
            ),
            (
                "cache-dir",
                config
                    .get("cache-dir")
                    .as_string()
                    .unwrap_or("")
                    .to_string(),
            ),
        ];

        for (key, cache_path) in cache_paths {
            // only individual dirs get garbage collected
            if key == "cache-dir" && gc {
                continue;
            }

            let cache_path = match realpath(&cache_path) {
                Some(path) => path,
                None => {
                    io.write_error(&format!(
                        "<info>Cache directory does not exist ({key}): {cache_path}</info>"
                    ));
                    continue;
                }
            };
            let mut cache = Cache::new(io.clone(), &cache_path, None, None, false);
            cache.set_read_only(config.get("cache-read-only").as_bool().unwrap_or(false));
            if !cache.is_enabled() {
                io.write_error(&format!(
                    "<info>Cache is not enabled ({key}): {cache_path}</info>"
                ));
                continue;
            }

            if gc {
                io.write_error(&format!(
                    "<info>Garbage-collecting cache ({key}): {cache_path}</info>"
                ));
                if key == "cache-files-dir" {
                    cache.gc(
                        config.get("cache-files-ttl").as_int().unwrap_or(0),
                        config.get("cache-files-maxsize").as_int().unwrap_or(0),
                    );
                } else if key == "cache-repo-dir" {
                    cache.gc(
                        config.get("cache-ttl").as_int().unwrap_or(0),
                        1024 * 1024 * 1024, // 1GB, this should almost never clear anything that is not outdated
                    );
                } else if key == "cache-vcs-dir" {
                    cache.gc_vcs_cache(config.get("cache-ttl").as_int().unwrap_or(0));
                }
            } else {
                io.write_error(&format!(
                    "<info>Clearing cache ({key}): {cache_path}</info>"
                ));
                cache.clear();
            }
        }

        if gc {
            io.write_error("<info>All caches garbage-collected.</info>");
        } else {
            io.write_error("<info>All caches cleared.</info>");
        }

        Ok(0)
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

impl BaseCommand for ClearCacheCommand {
    fn command_data(
        &self,
    ) -> &shirabe_external_packages::symfony::console::command::command::CommandData {
        self.base_command_data.command_data()
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}
