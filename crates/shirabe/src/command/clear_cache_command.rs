//! ref: composer/src/Composer/Command/ClearCacheCommand.php sha1:f8702b19db8724cfbb8cf8424192b612dbfe4b99

use crate::cache::Cache;
use crate::command::{
    BaseCommand, BaseCommandData, base_command_complete, base_command_initialize,
};
use crate::config::Config;
use crate::console::input::InputOption;
use crate::delegate_base_command_trait_impls_to_inner;
use crate::factory::Factory;
use crate::io::IOInterfaceWrite;
use bstr::BStr;
use shirabe_php_shim::{bconcat, impl_php_class, path_from_bytes, realpath};
use shirabe_symfony_console::command::Command;
use shirabe_symfony_console::input::{InputInterface, InputInterfaceByName};
use shirabe_symfony_console::output::OutputInterface;
use std::path::PathBuf;

#[derive(Debug)]
pub struct ClearCacheCommand {
    base_command_data: BaseCommandData,
}

impl_php_class!(ClearCacheCommand, r"Composer\Command\ClearCacheCommand");

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

#[async_trait::async_trait(?Send)]
impl Command for ClearCacheCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.set_name("clear-cache".into())?;
        self.set_aliases(vec!["clearcache".into(), "cc".into()])?;
        self.set_description("Clears composer's internal package cache".into());
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
            Read more at https://getcomposer.org/doc/03-cli.md#clear-cache-clearcache-cc"
                .into(),
        );
        Ok(())
    }

    async fn execute(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        _output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let composer = self.try_composer(None, None);
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

        let mut cache_paths: Vec<(&BStr, PathBuf)> = Vec::new();
        for key in [
            "cache-vcs-dir",
            "cache-repo-dir",
            "cache-files-dir",
            "cache-dir",
        ] {
            cache_paths.push((
                BStr::new(key),
                path_from_bytes(&config.get_path(key)?).to_path_buf(),
            ));
        }

        for (key, cache_path) in cache_paths {
            // only individual dirs get garbage collected
            if key == "cache-dir" && gc {
                continue;
            }

            let cache_path = match realpath(&cache_path) {
                Some(path) => path,
                None => {
                    io.write_error(bconcat!(
                        "<info>Cache directory does not exist (",
                        key,
                        "): ",
                        cache_path,
                        "</info>"
                    ));
                    continue;
                }
            };
            let mut cache = Cache::new(io.clone(), &cache_path, None, None, false);
            cache.set_read_only(config.get_bool("cache-read-only"));
            if !cache.is_enabled() {
                io.write_error(bconcat!(
                    "<info>Cache is not enabled (",
                    key,
                    "): ",
                    cache_path,
                    "</info>"
                ));
                continue;
            }

            if gc {
                io.write_error(bconcat!(
                    "<info>Garbage-collecting cache (",
                    key,
                    "): ",
                    cache_path,
                    "</info>"
                ));
                if key == "cache-files-dir" {
                    cache.gc(
                        config.get_cache_files_ttl(),
                        config.get_cache_files_maxsize()?,
                    );
                } else if key == "cache-repo-dir" {
                    cache.gc(
                        config.get_cache_ttl(),
                        1024 * 1024 * 1024, // 1GB, this should almost never clear anything that is not outdated
                    );
                } else if key == "cache-vcs-dir" {
                    cache.gc_vcs_cache(config.get_cache_ttl());
                }
            } else {
                io.write_error(bconcat!(
                    "<info>Clearing cache (",
                    key,
                    "): ",
                    cache_path,
                    "</info>"
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

    fn complete(
        &self,
        input: &shirabe_symfony_console::completion::CompletionInput,
        suggestions: &mut shirabe_symfony_console::completion::CompletionSuggestions,
    ) -> anyhow::Result<()> {
        base_command_complete(self, input, suggestions)
    }

    shirabe_symfony_console::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for ClearCacheCommand {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    delegate_base_command_trait_impls_to_inner!(base_command_data);
}
