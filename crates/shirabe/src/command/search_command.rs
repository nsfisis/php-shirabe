//! ref: composer/src/Composer/Command/SearchCommand.php

use crate::command::base_command::BaseCommand;
use crate::console::input::input_argument::InputArgument;
use crate::console::input::input_option::InputOption;
use crate::json::json_file::JsonFile;
use crate::plugin::command_event::CommandEvent;
use crate::plugin::plugin_events::PluginEvents;
use crate::repository::composite_repository::CompositeRepository;
use crate::repository::platform_repository::PlatformRepository;
use crate::repository::repository_interface::RepositoryInterface;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::console::formatter::output_formatter::OutputFormatter;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;
use shirabe_php_shim::{InvalidArgumentException, PhpMixed, implode, in_array, preg_quote};

#[derive(Debug)]
pub struct SearchCommand {
    inner: BaseCommand,
}

impl SearchCommand {
    pub fn configure(&mut self) {
        self.inner
            .set_name("search")
            .set_description("Searches for packages")
            .set_definition(vec![
                InputOption::new("only-name", Some(PhpMixed::String("N".to_string())), Some(InputOption::VALUE_NONE), "Search only in package names", None, vec![]),
                InputOption::new("only-vendor", Some(PhpMixed::String("O".to_string())), Some(InputOption::VALUE_NONE), "Search only for vendor / organization names, returns only \"vendor\" as result", None, vec![]),
                InputOption::new("type", Some(PhpMixed::String("t".to_string())), Some(InputOption::VALUE_REQUIRED), "Search for a specific package type", None, vec![]),
                InputOption::new("format", Some(PhpMixed::String("f".to_string())), Some(InputOption::VALUE_REQUIRED), "Format of the output: text or json", Some(PhpMixed::String("text".to_string())), vec!["json".to_string(), "text".to_string()]),
                InputArgument::new("tokens", Some(InputArgument::IS_ARRAY | InputArgument::REQUIRED), "tokens to search for", None, vec![]),
            ])
            .set_help(
                "The search command searches for packages by its name\n\
                <info>php composer.phar search symfony composer</info>\n\n\
                Read more at https://getcomposer.org/doc/03-cli.md#search"
            );
    }

    pub fn execute(
        &mut self,
        input: &dyn InputInterface,
        output: &dyn OutputInterface,
    ) -> Result<i64> {
        let platform_repo = PlatformRepository::new(vec![], IndexMap::new(), None, None)?;
        let io = self.inner.get_io();

        let format = input
            .get_option("format")
            .as_string_opt()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "text".to_string());
        if !in_array(
            PhpMixed::String(format.clone()),
            &PhpMixed::List(vec![
                Box::new(PhpMixed::String("text".to_string())),
                Box::new(PhpMixed::String("json".to_string())),
            ]),
            false,
        ) {
            io.write_error(&format!(
                "Unsupported format \"{}\". See help for supported formats.",
                format
            ));
            return Ok(1);
        }

        let composer = if let Some(c) = self.inner.try_composer() {
            c
        } else {
            self.inner
                .create_composer_instance(input, self.inner.get_io(), vec![])?
        };
        let local_repo = composer.get_repository_manager().get_local_repository();
        let installed_repo =
            CompositeRepository::new(vec![Box::new(local_repo), Box::new(platform_repo)]);
        let mut all_repos: Vec<Box<dyn RepositoryInterface>> = vec![Box::new(installed_repo)];
        all_repos.extend(composer.get_repository_manager().get_repositories());
        let repos = CompositeRepository::new(all_repos);

        // TODO(plugin): dispatch CommandEvent for search command
        let command_event = CommandEvent::new(
            PluginEvents::COMMAND.to_string(),
            "search".to_string(),
            Box::new(input),
            Box::new(output),
            vec![],
            vec![],
        );
        composer
            .get_event_dispatcher()
            .dispatch(command_event.get_name(), &command_event);

        let mut mode: i64 = RepositoryInterface::SEARCH_FULLTEXT;
        if input.get_option("only-name").as_bool().unwrap_or(false) {
            if input.get_option("only-vendor").as_bool().unwrap_or(false) {
                return Err(InvalidArgumentException {
                    message: "--only-name and --only-vendor cannot be used together".to_string(),
                    code: 0,
                }
                .into());
            }
            mode = RepositoryInterface::SEARCH_NAME;
        } else if input.get_option("only-vendor").as_bool().unwrap_or(false) {
            mode = RepositoryInterface::SEARCH_VENDOR;
        }

        let r#type = input
            .get_option("type")
            .as_string_opt()
            .map(|s| s.to_string());

        let tokens_arg = input.get_argument("tokens");
        let token_strings: Vec<String> = tokens_arg
            .as_array()
            .map(|arr| {
                arr.values()
                    .filter_map(|v| v.as_string().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let mut query = implode(" ", &token_strings);
        if mode != RepositoryInterface::SEARCH_FULLTEXT {
            query = preg_quote(&query, None);
        }

        let results = repos.search(query, mode, r#type);

        if results.len() > 0 && format == "text" {
            let width = self.inner.get_terminal_width();
            let mut name_length: i64 = 0;
            for result in &results {
                name_length = name_length.max(result.name.len() as i64);
            }
            name_length += 1;
            for result in &results {
                let description = result.description.clone().unwrap_or_default();
                let warning = if result.abandoned.is_some() {
                    "<warning>! Abandoned !</warning> "
                } else {
                    ""
                };
                let remaining = width - name_length - warning.len() as i64 - 2;
                let description = if description.len() as i64 > remaining {
                    format!("{}...", &description[..(remaining - 3) as usize])
                } else {
                    description
                };

                let link = result.url.as_deref();
                if let Some(link) = link {
                    io.write(&format!(
                        "<href={}>{}</>{}{}{}",
                        OutputFormatter::escape(link),
                        result.name,
                        " ".repeat(name_length as usize - result.name.len()),
                        warning,
                        description
                    ));
                } else {
                    io.write(&format!(
                        "{:<width$}{}{}",
                        result.name,
                        warning,
                        description,
                        width = name_length as usize
                    ));
                }
            }
        } else if format == "json" {
            io.write(&JsonFile::encode(&results));
        }

        Ok(0)
    }
}
