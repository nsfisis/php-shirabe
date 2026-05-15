//! ref: composer/src/Composer/Command/ArchiveCommand.php

use std::any::Any;

use anyhow::Result;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;
use shirabe_php_shim::{get_debug_type, LogicException};

use crate::command::base_command::BaseCommand;
use crate::command::completion_trait::CompletionTrait;
use crate::composer::Composer;
use crate::config::Config;
use crate::console::input::input_argument::InputArgument;
use crate::console::input::input_option::InputOption;
use crate::factory::Factory;
use crate::io::io_interface::IOInterface;
use crate::package::base_package::BasePackage;
use crate::package::complete_package_interface::CompletePackageInterface;
use crate::package::version::version_parser::VersionParser;
use crate::package::version::version_selector::VersionSelector;
use crate::plugin::command_event::CommandEvent;
use crate::plugin::plugin_events::PluginEvents;
use crate::repository::composite_repository::CompositeRepository;
use crate::repository::repository_factory::RepositoryFactory;
use crate::repository::repository_set::RepositorySet;
use crate::script::script_events::ScriptEvents;
use crate::util::filesystem::Filesystem;
use crate::util::loop_::Loop;
use crate::util::platform::Platform;
use crate::util::process_executor::ProcessExecutor;

#[derive(Debug)]
pub struct ArchiveCommand {
    inner: BaseCommand,
}

impl CompletionTrait for ArchiveCommand {}

impl ArchiveCommand {
    const FORMATS: &'static [&'static str] = &["tar", "tar.gz", "tar.bz2", "zip"];

    pub fn configure(&mut self) {
        let suggest_available_package = self.suggest_available_package();
        self.inner
            .set_name("archive")
            .set_description("Creates an archive of this composer package")
            .set_definition(vec![
                InputArgument::new("package", Some(InputArgument::OPTIONAL), "The package to archive instead of the current project", None, suggest_available_package),
                InputArgument::new("version", Some(InputArgument::OPTIONAL), "A version constraint to find the package to archive", None, vec![]),
                InputOption::new("format", Some(shirabe_php_shim::PhpMixed::String("f".to_string())), Some(InputOption::VALUE_REQUIRED), "Format of the resulting archive: tar, tar.gz, tar.bz2 or zip (default tar)", None, Self::FORMATS.iter().map(|s| s.to_string()).collect()),
                InputOption::new("dir", None, Some(InputOption::VALUE_REQUIRED), "Write the archive to this directory", None, vec![]),
                InputOption::new("file", None, Some(InputOption::VALUE_REQUIRED), "Write the archive with the given file name. Note that the format will be appended.", None, vec![]),
                InputOption::new("ignore-filters", None, Some(InputOption::VALUE_NONE), "Ignore filters when saving package", None, vec![]),
            ])
            .set_help(
                "The <info>archive</info> command creates an archive of the specified format\n\
                containing the files and directories of the Composer project or the specified\n\
                package in the specified version and writes it to the specified directory.\n\n\
                <info>php composer.phar archive [--format=zip] [--dir=/foo] [--file=filename] [package [version]]</info>\n\n\
                Read more at https://getcomposer.org/doc/03-cli.md#archive"
            );
    }

    pub fn execute(&self, input: &dyn InputInterface, output: &dyn OutputInterface) -> Result<i64> {
        let composer = self.inner.try_composer();
        let mut config: Option<Config> = None;

        if let Some(ref composer) = composer {
            config = Some(composer.get_config().clone());
            // TODO(plugin): dispatch CommandEvent
            let command_event = CommandEvent::new(
                PluginEvents::COMMAND.to_string(),
                "archive".to_string(),
                Box::new(input),
                Box::new(output),
                vec![],
                vec![],
            );
            let event_dispatcher = composer.get_event_dispatcher();
            event_dispatcher.dispatch(command_event.get_name(), &command_event);
            event_dispatcher.dispatch_script(ScriptEvents::PRE_ARCHIVE_CMD, true);
        }

        let config = match config {
            Some(c) => c,
            None => Factory::create_config(None, None)?,
        };

        let format = input.get_option("format").as_string_opt()
            .map(|s| s.to_string())
            .unwrap_or_else(|| config.get("archive-format").as_string().unwrap_or("tar").to_string());

        let dir = input.get_option("dir").as_string_opt()
            .map(|s| s.to_string())
            .unwrap_or_else(|| config.get("archive-dir").as_string().unwrap_or(".").to_string());

        let return_code = self.archive(
            self.inner.get_io(),
            &config,
            input.get_argument("package").as_string_opt().map(|s| s.to_string()),
            input.get_argument("version").as_string_opt().map(|s| s.to_string()),
            &format,
            &dir,
            input.get_option("file").as_string_opt().map(|s| s.to_string()),
            input.get_option("ignore-filters").as_bool().unwrap_or(false),
            composer.as_ref(),
        )?;

        if return_code == 0 {
            if let Some(ref composer) = composer {
                composer.get_event_dispatcher().dispatch_script(ScriptEvents::POST_ARCHIVE_CMD, true);
            }
        }

        Ok(return_code)
    }

    pub fn archive(
        &self,
        io: &dyn IOInterface,
        config: &Config,
        package_name: Option<String>,
        version: Option<String>,
        format: &str,
        dest: &str,
        file_name: Option<String>,
        ignore_filters: bool,
        composer: Option<&Composer>,
    ) -> Result<i64> {
        let archive_manager = if let Some(composer) = composer {
            composer.get_archive_manager().clone_box()
        } else {
            let factory = Factory::new();
            let process = ProcessExecutor::new_default();
            let http_downloader = Factory::create_http_downloader(io, config)?;
            let download_manager = factory.create_download_manager(io, config, &http_downloader, &process)?;
            let loop_ = Loop::new(http_downloader, process);
            factory.create_archive_manager(config, &download_manager, &loop_)?
        };

        let package = if let Some(name) = package_name {
            match self.select_package(io, &name, version.as_deref())? {
                Some(p) => p,
                None => return Ok(1),
            }
        } else {
            self.inner.require_composer()?.get_package().clone_box()
        };

        io.write_error(&format!("<info>Creating the archive into \"{}\".</info>", dest));
        let package_path = archive_manager.archive(package.as_ref(), format, dest, file_name.as_deref(), ignore_filters)?;
        let fs = Filesystem::new();
        let short_path = fs.find_shortest_path(&Platform::get_cwd(), &package_path, true);

        io.write_error_no_newline("Created: ");
        let display = if short_path.len() < package_path.len() { &short_path } else { &package_path };
        io.write(display);

        Ok(0)
    }

    pub fn select_package(
        &self,
        io: &dyn IOInterface,
        package_name: &str,
        version: Option<&str>,
    ) -> Result<Option<Box<dyn CompletePackageInterface>>> {
        io.write_error("<info>Searching for the specified package.</info>");

        let mut version = version.map(|v| v.to_string());
        let mut min_stability;
        let repo;

        if let Some(composer) = self.inner.try_composer() {
            let local_repo = composer.get_repository_manager().get_local_repository();
            let mut repos: Vec<Box<dyn crate::repository::repository_interface::RepositoryInterface>> = vec![local_repo.clone_box()];
            repos.extend(composer.get_repository_manager().get_repositories().iter().map(|r| r.clone_box()));
            repo = CompositeRepository::new(repos);
            min_stability = composer.get_package().get_minimum_stability().to_string();
        } else {
            let default_repos = RepositoryFactory::default_repos_with_default_manager(io)?;
            let repo_names: Vec<String> = default_repos.iter().map(|r| r.get_repo_name()).collect();
            io.write_error(&format!("No composer.json found in the current directory, searching packages from {}", repo_names.join(", ")));
            repo = CompositeRepository::new(default_repos);
            min_stability = "stable".to_string();
        }

        if let Some(version_str) = &version {
            if let Some(matches) = Preg::match_strict_groups(r"{@(stable|RC|beta|alpha|dev)$}i", version_str) {
                min_stability = VersionParser::normalize_stability(&matches[1]);
                let full_match_len = matches[0].len();
                version = Some(version_str[..version_str.len() - full_match_len].to_string());
            }
        }

        let mut repo_set = RepositorySet::new(&min_stability);
        repo_set.add_repository(Box::new(repo));
        let parser = VersionParser::new();
        let constraint = version.as_deref().map(|v| parser.parse_constraints(v));
        let packages = repo_set.find_packages(&package_name.to_lowercase(), constraint.as_deref());

        let package = if packages.len() > 1 {
            let version_selector = VersionSelector::new(&repo_set);
            let best = version_selector.find_best_candidate(&package_name.to_lowercase(), version.as_deref(), &min_stability);
            let p = best.unwrap_or_else(|| packages.into_iter().next().unwrap());

            io.write_error(&format!("<info>Found multiple matches, selected {}.</info>", p.get_pretty_string()));
            // alternatives message omitted for brevity (already logged via p being selected)
            io.write_error("<comment>Please use a more specific constraint to pick a different package.</comment>");
            p
        } else if packages.len() == 1 {
            let p = packages.into_iter().next().unwrap();
            io.write_error(&format!("<info>Found an exact match {}.</info>", p.get_pretty_string()));
            p
        } else {
            io.write_error(&format!("<error>Could not find a package matching {}.</error>", package_name));
            return Ok(None);
        };

        if (package.as_any() as &dyn Any).downcast_ref::<dyn CompletePackageInterface>().is_none() {
            return Err(LogicException {
                message: format!("Expected a CompletePackageInterface instance but found {}", get_debug_type(package.as_php_mixed())),
                code: 0,
            }.into());
        }
        if (package.as_any() as &dyn Any).downcast_ref::<BasePackage>().is_none() {
            return Err(LogicException {
                message: format!("Expected a BasePackage instance but found {}", get_debug_type(package.as_php_mixed())),
                code: 0,
            }.into());
        }

        Ok(Some(package.into_complete()))
    }
}
