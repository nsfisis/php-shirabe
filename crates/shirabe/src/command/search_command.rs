//! ref: composer/src/Composer/Command/SearchCommand.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::console::command::command::Command;
use shirabe_external_packages::symfony::console::formatter::OutputFormatter;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use shirabe_php_shim::{InvalidArgumentException, PhpMixed, implode, in_array, preg_quote};
use std::cell::RefCell;
use std::rc::Rc;

use crate::advisory::AuditConfig;
use crate::command::base_command::base_command_initialize;
use crate::command::{BaseCommand, BaseCommandData};
use crate::composer::PartialComposerHandle;
use crate::config::Config;
use crate::console::input::InputArgument;
use crate::console::input::InputOption;
use crate::filter::platform_requirement_filter::PlatformRequirementFilterInterface;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::json::JsonFile;
use crate::plugin::CommandEvent;
use crate::plugin::PluginEvents;
use crate::repository::CompositeRepository;
use crate::repository::PlatformRepository;
use crate::repository::RepositoryInterfaceHandle;
use crate::repository::repository_interface::{self, RepositoryInterface};

#[derive(Debug)]
pub struct SearchCommand {
    base_command_data: BaseCommandData,
}

impl SearchCommand {
    pub fn new() -> Self {
        let mut command = SearchCommand {
            base_command_data: BaseCommandData::new(None),
        };
        command
            .configure()
            .expect("SearchCommand::configure uses static, valid metadata");
        command
    }
}

impl Command for SearchCommand {
    fn configure(&mut self) -> anyhow::Result<()> {
        self.set_name("search")?;
        self.set_description("Searches for packages");
        self.set_definition(&[
            InputOption::new(
                "only-name",
                Some(PhpMixed::String("N".to_string())),
                Some(InputOption::VALUE_NONE),
                "Search only in package names",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new(
                "only-vendor",
                Some(PhpMixed::String("O".to_string())),
                Some(InputOption::VALUE_NONE),
                "Search only for vendor / organization names, returns only \"vendor\" as result",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new(
                "type",
                Some(PhpMixed::String("t".to_string())),
                Some(InputOption::VALUE_REQUIRED),
                "Search for a specific package type",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new(
                "format",
                Some(PhpMixed::String("f".to_string())),
                Some(InputOption::VALUE_REQUIRED),
                "Format of the output: text or json",
                Some(PhpMixed::String("text".to_string())),
            )
            .unwrap()
            .into(),
            InputArgument::new(
                "tokens",
                Some(InputArgument::IS_ARRAY | InputArgument::REQUIRED),
                "tokens to search for",
                None,
            )
            .unwrap()
            .into(),
        ]);
        self.set_help(
            "The search command searches for packages by its name\n\
            <info>shirabe search symfony composer</info>\n\n\
            Read more at https://getcomposer.org/doc/03-cli.md#search",
        );
        Ok(())
    }

    fn execute(
        &mut self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let platform_repo = PlatformRepository::new4(vec![], IndexMap::new(), None, None)?;
        let io = self.get_io();

        let format = input
            .borrow()
            .get_option("format")?
            .as_string()
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

        let composer = if let Some(c) = self.try_composer(None, None) {
            c
        } else {
            self.create_composer_instance(input.clone(), io.clone(), None, false, None)?
        };
        let composer_ref = crate::command::composer_full(&composer);
        let local_repo = composer_ref
            .get_repository_manager()
            .borrow()
            .get_local_repository();
        let installed_repo = CompositeRepository::new(vec![
            local_repo,
            RepositoryInterfaceHandle::new(platform_repo),
        ]);
        let mut all_repos: Vec<RepositoryInterfaceHandle> =
            vec![RepositoryInterfaceHandle::new(installed_repo)];
        for r in composer_ref
            .get_repository_manager()
            .borrow()
            .get_repositories()
        {
            all_repos.push(r.clone());
        }
        let mut repos = CompositeRepository::new(all_repos);

        // TODO(plugin): dispatch CommandEvent for search command
        let command_event =
            CommandEvent::new(PluginEvents::COMMAND, "search", input.clone(), output);
        let dispatcher = composer_ref.get_event_dispatcher().clone();
        drop(composer_ref);
        dispatcher
            .borrow_mut()
            .dispatch(Some(command_event.get_name()), None);

        let mut mode: i64 = repository_interface::SEARCH_FULLTEXT;
        if input
            .borrow()
            .get_option("only-name")?
            .as_bool()
            .unwrap_or(false)
        {
            if input
                .borrow()
                .get_option("only-vendor")?
                .as_bool()
                .unwrap_or(false)
            {
                return Err(InvalidArgumentException {
                    message: "--only-name and --only-vendor cannot be used together".to_string(),
                    code: 0,
                }
                .into());
            }
            mode = repository_interface::SEARCH_NAME;
        } else if input
            .borrow()
            .get_option("only-vendor")?
            .as_bool()
            .unwrap_or(false)
        {
            mode = repository_interface::SEARCH_VENDOR;
        }

        let r#type = input
            .borrow()
            .get_option("type")?
            .as_string()
            .map(|s| s.to_string());

        let tokens_arg = input.borrow().get_argument("tokens")?;
        let token_strings: Vec<String> = tokens_arg
            .as_array()
            .map(|arr| {
                arr.values()
                    .filter_map(|v| v.as_string().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let mut query = implode(" ", &token_strings);
        if mode != repository_interface::SEARCH_FULLTEXT {
            query = preg_quote(&query, None);
        }

        let results = repos.search(query, mode, r#type)?;

        if results.len() > 0 && format == "text" {
            let width = self.get_terminal_width();
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
                        OutputFormatter::escape(link)?,
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
            // TODO(phase-c): faithful JSON output requires SearchResult to retain the raw result
            // array. PHP's fulltext search passes through arbitrary API fields (downloads, favers,
            // repository, ...) which the typed SearchResult (name/description/abandoned/url) drops,
            // so encoding it here would diverge from Composer's output.
            let _ = &results;
            io.write(&JsonFile::encode(&PhpMixed::Null));
        }

        Ok(0)
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

impl BaseCommand for SearchCommand {
    fn command_data_mut(
        &mut self,
    ) -> &mut shirabe_external_packages::symfony::console::command::command::CommandData {
        self.base_command_data.command_data_mut()
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}
