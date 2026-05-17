//! ref: composer/src/Composer/Command/ClearCacheCommand.php

use crate::cache::Cache;
use crate::command::base_command::BaseCommand;
use crate::composer::Composer;
use crate::console::input::input_option::InputOption;
use crate::factory::Factory;
use crate::io::io_interface::IOInterface;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::component::console::command::command::Command;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;

#[derive(Debug)]
pub struct ClearCacheCommand {
    inner: Command,
    composer: Option<Composer>,
    io: Option<Box<dyn IOInterface>>,
}

impl ClearCacheCommand {
    pub fn configure(&mut self) {
        self.inner
            .set_name("clear-cache")
            .set_aliases(vec!["clearcache".to_string(), "cc".to_string()])
            .set_description("Clears composer's internal package cache")
            .set_definition(vec![InputOption::new(
                "gc",
                None,
                InputOption::VALUE_NONE,
                "Only run garbage collection, not a full cache clear",
            )])
            .set_help(
                "The <info>clear-cache</info> deletes all cached packages from composer's\n\
                cache directory.\n\n\
                Read more at https://getcomposer.org/doc/03-cli.md#clear-cache-clearcache-cc",
            );
    }

    pub fn execute(
        &self,
        input: &dyn InputInterface,
        _output: &dyn OutputInterface,
    ) -> anyhow::Result<i64> {
        let composer = self.inner.try_composer();
        let config = if let Some(composer) = composer {
            composer.get_config()
        } else {
            Factory::create_config(None, None)?
        };

        let io = self.inner.get_io();

        let mut cache_paths: IndexMap<String, String> = IndexMap::new();
        cache_paths.insert(
            "cache-vcs-dir".to_string(),
            config.get("cache-vcs-dir").to_string(),
        );
        cache_paths.insert(
            "cache-repo-dir".to_string(),
            config.get("cache-repo-dir").to_string(),
        );
        cache_paths.insert(
            "cache-files-dir".to_string(),
            config.get("cache-files-dir").to_string(),
        );
        cache_paths.insert("cache-dir".to_string(), config.get("cache-dir").to_string());

        for (key, cache_path) in &cache_paths {
            // only individual dirs get garbage collected
            if key == "cache-dir" && input.get_option("gc").as_bool() {
                continue;
            }

            let cache_path = shirabe_php_shim::realpath(cache_path);
            if !cache_path.as_ref().map(|s| !s.is_empty()).unwrap_or(false) {
                let cache_path_display = cache_path.as_deref().unwrap_or("");
                io.write_error(&format!(
                    "<info>Cache directory does not exist ({key}): {cache_path_display}</info>"
                ));
                continue;
            }
            let cache_path = cache_path.unwrap();
            let mut cache = Cache::new(io, &cache_path);
            cache.set_read_only(config.get("cache-read-only").as_bool().unwrap_or(false));
            if !cache.is_enabled() {
                io.write_error(&format!(
                    "<info>Cache is not enabled ({key}): {cache_path}</info>"
                ));
                continue;
            }

            if input.get_option("gc").as_bool() {
                io.write_error(&format!(
                    "<info>Garbage-collecting cache ({key}): {cache_path}</info>"
                ));
                if key == "cache-files-dir" {
                    cache.gc(
                        config.get("cache-files-ttl"),
                        config.get("cache-files-maxsize"),
                    )?;
                } else if key == "cache-repo-dir" {
                    cache.gc(config.get("cache-ttl"), 1024 * 1024 * 1024)?;
                } else if key == "cache-vcs-dir" {
                    cache.gc_vcs_cache(config.get("cache-ttl"))?;
                }
            } else {
                io.write_error(&format!(
                    "<info>Clearing cache ({key}): {cache_path}</info>"
                ));
                cache.clear()?;
            }
        }

        if input.get_option("gc").as_bool() {
            io.write_error("<info>All caches garbage-collected.</info>");
        } else {
            io.write_error("<info>All caches cleared.</info>");
        }

        Ok(0)
    }
}

impl BaseCommand for ClearCacheCommand {
    fn inner(&self) -> &Command {
        &self.inner
    }

    fn inner_mut(&mut self) -> &mut Command {
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
