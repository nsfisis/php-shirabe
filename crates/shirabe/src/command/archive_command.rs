//! ref: composer/src/Composer/Command/ArchiveCommand.php

use std::any::Any;

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::{CaptureKey, Preg};
use shirabe_external_packages::symfony::component::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::component::console::output::output_interface::OutputInterface;
use shirabe_php_shim::{LogicException, get_debug_type};

use crate::command::base_command::{BaseCommand, BaseCommandData, HasBaseCommandData};
use crate::composer::Composer;
use crate::config::Config;
use crate::console::input::input_argument::InputArgument;
use crate::console::input::input_option::InputOption;
use crate::factory::Factory;
use crate::io::io_interface::IOInterface;
use crate::package::archiver::archive_manager::ArchiveManager;
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
use crate::util::r#loop::Loop;
use crate::util::platform::Platform;
use crate::util::process_executor::ProcessExecutor;

#[derive(Debug)]
pub struct ArchiveCommand {
    base_command_data: BaseCommandData,
}

impl ArchiveCommand {
    const FORMATS: &'static [&'static str] = &["tar", "tar.gz", "tar.bz2", "zip"];

    pub fn configure(&mut self) {
        // TODO(cli-completion): suggest_available_package(99) for `package` argument
        self
            .set_name("archive")
            .set_description("Creates an archive of this composer package")
            .set_definition(&[
                InputArgument::new("package", Some(InputArgument::OPTIONAL), "The package to archive instead of the current project", None).unwrap().into(),
                InputArgument::new("version", Some(InputArgument::OPTIONAL), "A version constraint to find the package to archive", None).unwrap().into(),
                InputOption::new("format", Some(shirabe_php_shim::PhpMixed::String("f".to_string())), Some(InputOption::VALUE_REQUIRED), "Format of the resulting archive: tar, tar.gz, tar.bz2 or zip (default tar)", None).unwrap().into(),
                InputOption::new("dir", None, Some(InputOption::VALUE_REQUIRED), "Write the archive to this directory", None).unwrap().into(),
                InputOption::new("file", None, Some(InputOption::VALUE_REQUIRED), "Write the archive with the given file name. Note that the format will be appended.", None).unwrap().into(),
                InputOption::new("ignore-filters", None, Some(InputOption::VALUE_NONE), "Ignore filters when saving package", None).unwrap().into(),
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
        let composer = self.try_composer(None, None);
        let mut config: Option<std::rc::Rc<std::cell::RefCell<Config>>> = None;

        if let Some(ref composer) = composer {
            config = Some(std::rc::Rc::clone(composer.get_config()));
            // TODO(plugin): dispatch CommandEvent
            let command_event = CommandEvent::new(PluginEvents::COMMAND, "archive", input, output);
            let event_dispatcher = composer.get_event_dispatcher();
            event_dispatcher.dispatch(Some(command_event.get_name()), None);
            event_dispatcher.dispatch_script(
                ScriptEvents::PRE_ARCHIVE_CMD,
                true,
                vec![],
                indexmap::IndexMap::new(),
            );
        }

        let config = match config {
            Some(c) => c,
            None => std::rc::Rc::new(std::cell::RefCell::new(Factory::create_config(None, None)?)),
        };

        let format = input
            .get_option("format")
            .as_string_opt()
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                config
                    .borrow_mut()
                    .get("archive-format")
                    .as_string()
                    .unwrap_or("tar")
                    .to_string()
            });

        let dir = input
            .get_option("dir")
            .as_string_opt()
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                config
                    .borrow_mut()
                    .get("archive-dir")
                    .as_string()
                    .unwrap_or(".")
                    .to_string()
            });

        let return_code = self.archive(
            self.get_io(),
            &config,
            input
                .get_argument("package")
                .as_string_opt()
                .map(|s| s.to_string()),
            input
                .get_argument("version")
                .as_string_opt()
                .map(|s| s.to_string()),
            &format,
            &dir,
            input
                .get_option("file")
                .as_string_opt()
                .map(|s| s.to_string()),
            input
                .get_option("ignore-filters")
                .as_bool()
                .unwrap_or(false),
            composer.as_ref(),
        )?;

        if return_code == 0 {
            if let Some(ref composer) = composer {
                composer.get_event_dispatcher().dispatch_script(
                    ScriptEvents::POST_ARCHIVE_CMD,
                    true,
                    vec![],
                    indexmap::IndexMap::new(),
                );
            }
        }

        Ok(return_code)
    }

    pub fn archive(
        &self,
        io: &dyn IOInterface,
        config: &std::rc::Rc<std::cell::RefCell<Config>>,
        package_name: Option<String>,
        version: Option<String>,
        format: &str,
        dest: &str,
        file_name: Option<String>,
        ignore_filters: bool,
        composer: Option<&Composer>,
    ) -> Result<i64> {
        let owned_archive_manager;
        let archive_manager: &ArchiveManager = if let Some(composer) = composer {
            composer.get_archive_manager()
        } else {
            let factory = Factory;
            let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(None)));
            let http_downloader = std::rc::Rc::new(std::cell::RefCell::new(
                Factory::create_http_downloader(io, config, indexmap::IndexMap::new())?,
            ));
            let download_manager =
                factory.create_download_manager(io, config, &http_downloader, &process, None)?;
            let loop_ = std::rc::Rc::new(std::cell::RefCell::new(Loop::new(
                std::rc::Rc::clone(&http_downloader),
                Some(process),
            )));
            owned_archive_manager =
                factory.create_archive_manager(&*config.borrow(), &download_manager, &loop_)?;
            &owned_archive_manager
        };

        let package = if let Some(name) = package_name {
            match self.select_package(io, &name, version.as_deref())? {
                Some(p) => p,
                None => return Ok(1),
            }
        } else {
            self.require_composer(None, None)?.get_package().clone_box()
        };

        io.write_error(&format!(
            "<info>Creating the archive into \"{}\".</info>",
            dest
        ));
        // TODO(phase-b): ArchiveManager.archive needs &mut self and &mut CompletePackageInterface;
        // current composer.get_archive_manager() returns &ArchiveManager. Needs RefCell wrapper.
        let _ = archive_manager;
        let _ = (
            package.as_ref(),
            format,
            dest,
            file_name.as_deref(),
            ignore_filters,
        );
        let package_path: String = todo!("ArchiveManager.archive call");
        let fs = Filesystem::new(None);
        let short_path =
            fs.find_shortest_path(&Platform::get_cwd(false)?, &package_path, true, false);

        io.write_error_no_newline("Created: ");
        let display = if short_path.len() < package_path.len() {
            &short_path
        } else {
            &package_path
        };
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

        if let Some(composer) = self.try_composer(None, None) {
            let local_repo = composer.get_repository_manager().get_local_repository();
            let mut repos: Vec<
                Box<dyn crate::repository::repository_interface::RepositoryInterface>,
            > = vec![local_repo.clone_box()];
            repos.extend(
                composer
                    .get_repository_manager()
                    .get_repositories()
                    .iter()
                    .map(|r| r.clone_box()),
            );
            repo = CompositeRepository::new(repos);
            min_stability = composer.get_package().get_minimum_stability().to_string();
        } else {
            let default_repos = RepositoryFactory::default_repos_with_default_manager(io)?;
            let repo_names: Vec<String> = default_repos.iter().map(|r| r.get_repo_name()).collect();
            io.write_error(&format!(
                "No composer.json found in the current directory, searching packages from {}",
                repo_names.join(", ")
            ));
            repo = CompositeRepository::new(default_repos);
            min_stability = "stable".to_string();
        }

        if let Some(version_str) = &version {
            let mut matches: IndexMap<CaptureKey, String> = IndexMap::new();
            if Preg::match_strict_groups3(
                r"{@(stable|RC|beta|alpha|dev)$}i",
                version_str,
                Some(&mut matches),
            )
            .unwrap_or(false)
            {
                let m1 = matches
                    .get(&CaptureKey::ByIndex(1))
                    .cloned()
                    .unwrap_or_default();
                let m0 = matches
                    .get(&CaptureKey::ByIndex(0))
                    .cloned()
                    .unwrap_or_default();
                min_stability = VersionParser::normalize_stability(&m1)?;
                let full_match_len = m0.len();
                version = Some(version_str[..version_str.len() - full_match_len].to_string());
            }
        }

        let mut repo_set = RepositorySet::new(
            &min_stability,
            IndexMap::new(),
            Vec::new(),
            IndexMap::new(),
            IndexMap::new(),
            IndexMap::new(),
        );
        repo_set.add_repository(Box::new(repo))?;
        let parser = VersionParser::new();
        let constraint: Option<
            Box<dyn shirabe_semver::constraint::constraint_interface::ConstraintInterface>,
        > = match version.as_deref() {
            Some(v) => Some(parser.parse_constraints(v)?.clone_box()),
            None => None,
        };
        let packages = repo_set.find_packages(&package_name.to_lowercase(), constraint, 0);

        let package = if packages.len() > 1 {
            let mut version_selector = VersionSelector::new(repo_set, None)?;
            let best = version_selector.find_best_candidate(
                &package_name.to_lowercase(),
                version.as_deref(),
                &min_stability,
                None,
                0,
                None,
                shirabe_php_shim::PhpMixed::Bool(true),
            )?;
            let p = best.unwrap_or_else(|| packages.into_iter().next().unwrap());

            io.write_error(&format!(
                "<info>Found multiple matches, selected {}.</info>",
                p.get_pretty_string()
            ));
            // alternatives message omitted for brevity (already logged via p being selected)
            io.write_error("<comment>Please use a more specific constraint to pick a different package.</comment>");
            p
        } else if packages.len() == 1 {
            let p = packages.into_iter().next().unwrap();
            io.write_error(&format!(
                "<info>Found an exact match {}.</info>",
                p.get_pretty_string()
            ));
            p
        } else {
            io.write_error(&format!(
                "<error>Could not find a package matching {}.</error>",
                package_name
            ));
            return Ok(None);
        };

        // TODO(phase-b): instanceof CompletePackageInterface / BasePackage runtime
        // checks require downcast support that BasePackage trait does not yet expose.
        let _ = &package;
        todo!("convert Box<dyn BasePackage> into Box<dyn CompletePackageInterface>")
    }
}

impl HasBaseCommandData for ArchiveCommand {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    fn base_command_data_mut(&mut self) -> &mut BaseCommandData {
        &mut self.base_command_data
    }
}
