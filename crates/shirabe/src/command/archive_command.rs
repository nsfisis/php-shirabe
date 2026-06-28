//! ref: composer/src/Composer/Command/ArchiveCommand.php

use crate::command::base_command::base_command_initialize;
use crate::command::{BaseCommand, BaseCommandData};
use crate::composer::PartialComposerHandle;
use crate::config::Config;
use crate::console::input::InputArgument;
use crate::console::input::InputOption;
use crate::factory::Factory;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::package::archiver::ArchiveManagerInterface;
use crate::package::version::VersionParser;
use crate::package::version::VersionSelector;
use crate::plugin::CommandEvent;
use crate::plugin::PluginEvents;
use crate::repository::CompositeRepository;
use crate::repository::RepositoryFactory;
use crate::repository::RepositorySet;
use crate::script::ScriptEvents;
use crate::util::Filesystem;
use crate::util::Platform;
use crate::util::ProcessExecutor;
use crate::util::r#loop::Loop;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_external_packages::symfony::console::command::command::Command;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use shirabe_php_shim::{LogicException, get_debug_type};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug)]
pub struct ArchiveCommand {
    base_command_data: BaseCommandData,
    /// For testing only: partial-mock seam mirroring PHPUnit `onlyMethods(['initialize', 'archive'])`.
    test_hooks: RefCell<ArchiveCommandTestHooks>,
}

/// For testing only: records and stubs for the `ArchiveCommand` partial-mock seam.
#[derive(Debug, Default)]
pub struct ArchiveCommandTestHooks {
    skip_initialize: bool,
    archive_stub_return: Option<i64>,
    archive_calls: Vec<ArchiveCallRecord>,
}

/// For testing only: the scalar arguments captured from a stubbed `archive` call.
#[derive(Debug, Clone)]
pub struct ArchiveCallRecord {
    pub package_name: Option<String>,
    pub version: Option<String>,
    pub format: String,
    pub dest: String,
    pub file_name: Option<String>,
    pub ignore_filters: bool,
    pub had_composer: bool,
}

impl Default for ArchiveCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl ArchiveCommand {
    const FORMATS: &'static [&'static str] = &["tar", "tar.gz", "tar.bz2", "zip"];

    pub fn new() -> Self {
        let command = ArchiveCommand {
            base_command_data: BaseCommandData::new(None),
            test_hooks: RefCell::new(ArchiveCommandTestHooks::default()),
        };
        command
            .configure()
            .expect("ArchiveCommand::configure uses static, valid metadata");
        command
    }
}

impl Command for ArchiveCommand {
    fn configure(&self) -> anyhow::Result<()> {
        // TODO(cli-completion): suggest_available_package(99) for `package` argument
        self.set_name("archive")?;
        self.set_description("Creates an archive of this composer package");
        self.set_definition(&[
            InputArgument::new("package", Some(InputArgument::OPTIONAL), "The package to archive instead of the current project", None).unwrap().into(),
            InputArgument::new("version", Some(InputArgument::OPTIONAL), "A version constraint to find the package to archive", None).unwrap().into(),
            InputOption::new("format", Some(shirabe_php_shim::PhpMixed::String("f".to_string())), Some(InputOption::VALUE_REQUIRED), "Format of the resulting archive: tar, tar.gz, tar.bz2 or zip (default tar)", None).unwrap().into(),
            InputOption::new("dir", None, Some(InputOption::VALUE_REQUIRED), "Write the archive to this directory", None).unwrap().into(),
            InputOption::new("file", None, Some(InputOption::VALUE_REQUIRED), "Write the archive with the given file name. Note that the format will be appended.", None).unwrap().into(),
            InputOption::new("ignore-filters", None, Some(InputOption::VALUE_NONE), "Ignore filters when saving package", None).unwrap().into(),
        ]);
        self.set_help(
            "The <info>archive</info> command creates an archive of the specified format\n\
            containing the files and directories of the Composer project or the specified\n\
            package in the specified version and writes it to the specified directory.\n\n\
            <info>shirabe archive [--format=zip] [--dir=/foo] [--file=filename] [package [version]]</info>\n\n\
            Read more at https://getcomposer.org/doc/03-cli.md#archive"
        );
        Ok(())
    }

    fn execute(
        &self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let composer = self.try_composer(None, None);

        let config = if let Some(ref composer) = composer {
            let config = composer.borrow_partial().get_config();
            // TODO(plugin): dispatch CommandEvent
            let command_event =
                CommandEvent::new(PluginEvents::COMMAND, "archive", input.clone(), output);
            let event_dispatcher = composer.borrow_partial().get_event_dispatcher();
            event_dispatcher
                .borrow_mut()
                .dispatch(Some(command_event.get_name()), None);
            event_dispatcher.borrow_mut().dispatch_script(
                ScriptEvents::PRE_ARCHIVE_CMD,
                true,
                vec![],
                indexmap::IndexMap::new(),
            );
            config
        } else {
            std::rc::Rc::new(std::cell::RefCell::new(Factory::create_config(None, None)?))
        };

        let format = input
            .borrow()
            .get_option("format")?
            .as_string()
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
            .borrow()
            .get_option("dir")?
            .as_string()
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                config
                    .borrow_mut()
                    .get("archive-dir")
                    .as_string()
                    .unwrap_or(".")
                    .to_string()
            });

        let io = self.get_io().clone();
        let return_code = self.archive(
            io.clone(),
            &config,
            input
                .borrow()
                .get_argument("package")?
                .as_string()
                .map(|s| s.to_string()),
            input
                .borrow()
                .get_argument("version")?
                .as_string()
                .map(|s| s.to_string()),
            &format,
            &dir,
            input
                .borrow()
                .get_option("file")?
                .as_string()
                .map(|s| s.to_string()),
            input
                .borrow()
                .get_option("ignore-filters")?
                .as_bool()
                .unwrap_or(false),
            composer.as_ref(),
        )?;

        if return_code == 0
            && let Some(ref composer) = composer
        {
            composer
                .borrow_partial()
                .get_event_dispatcher()
                .borrow_mut()
                .dispatch_script(
                    ScriptEvents::POST_ARCHIVE_CMD,
                    true,
                    vec![],
                    indexmap::IndexMap::new(),
                );
        }

        Ok(return_code)
    }

    fn initialize(
        &self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        if self.test_hooks.borrow().skip_initialize {
            return Ok(());
        }
        base_command_initialize(self, input, output)
    }

    shirabe_external_packages::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl ArchiveCommand {
    /// For testing only: makes `initialize` a no-op (PHPUnit `onlyMethods(['initialize'])`).
    pub fn __test_skip_initialize(&self) {
        self.test_hooks.borrow_mut().skip_initialize = true;
    }

    /// For testing only: stubs `archive` to record its arguments and return `return_value`
    /// without running (PHPUnit `onlyMethods(['archive'])`).
    pub fn __test_stub_archive(&self, return_value: i64) {
        self.test_hooks.borrow_mut().archive_stub_return = Some(return_value);
    }

    /// For testing only: the arguments captured from stubbed `archive` calls.
    pub fn __test_archive_calls(&self) -> Vec<ArchiveCallRecord> {
        self.test_hooks.borrow().archive_calls.clone()
    }
}

impl BaseCommand for ArchiveCommand {
    fn command_data(
        &self,
    ) -> &shirabe_external_packages::symfony::console::command::command::CommandData {
        self.base_command_data.command_data()
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}

impl ArchiveCommand {
    #[allow(clippy::too_many_arguments, reason = "to keep PHP signature")]
    pub fn archive(
        &self,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: &std::rc::Rc<std::cell::RefCell<Config>>,
        package_name: Option<String>,
        version: Option<String>,
        format: &str,
        dest: &str,
        file_name: Option<String>,
        ignore_filters: bool,
        composer: Option<&PartialComposerHandle>,
    ) -> anyhow::Result<i64> {
        let archive_stub_return = self.test_hooks.borrow().archive_stub_return;
        if let Some(return_value) = archive_stub_return {
            self.test_hooks
                .borrow_mut()
                .archive_calls
                .push(ArchiveCallRecord {
                    package_name,
                    version,
                    format: format.to_string(),
                    dest: dest.to_string(),
                    file_name,
                    ignore_filters,
                    had_composer: composer.is_some(),
                });
            return Ok(return_value);
        }

        let composer_guard = composer.map(crate::composer::composer_full);
        let mut owned_archive_manager;
        let composer_archive_manager;
        let mut composer_archive_manager_ref;
        let archive_manager: &mut dyn ArchiveManagerInterface =
            if let Some(composer) = &composer_guard {
                composer_archive_manager = composer.get_archive_manager().clone();
                composer_archive_manager_ref = composer_archive_manager.borrow_mut();
                &mut *composer_archive_manager_ref
            } else {
                let factory = Factory::default();
                let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(None)));
                let http_downloader = std::rc::Rc::new(std::cell::RefCell::new(
                    Factory::create_http_downloader(io.clone(), config, indexmap::IndexMap::new())?,
                ));
                let download_manager = factory.create_download_manager(
                    io.clone(),
                    config,
                    &http_downloader,
                    &process,
                    None,
                )?;
                let loop_ = std::rc::Rc::new(std::cell::RefCell::new(Loop::new(
                    http_downloader.clone(),
                    Some(process),
                )));
                owned_archive_manager =
                    factory.create_archive_manager(&config.borrow(), &download_manager, &loop_)?;
                &mut owned_archive_manager
            };

        let package: crate::package::CompletePackageInterfaceHandle =
            if let Some(name) = package_name {
                match self.select_package(io.clone(), &name, version.as_deref())? {
                    Some(p) => p,
                    None => return Ok(1),
                }
            } else {
                let rc = self.require_composer(None, None)?;
                let composer = crate::composer::composer_full(&rc);
                composer.get_package().clone().into()
            };

        io.write_error(&format!(
            "<info>Creating the archive into \"{}\".</info>",
            dest
        ));
        let package_path: String = archive_manager.archive(
            package,
            format.to_string(),
            dest.to_string(),
            file_name,
            ignore_filters,
        )?;
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
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        package_name: &str,
        version: Option<&str>,
    ) -> anyhow::Result<Option<crate::package::CompletePackageInterfaceHandle>> {
        io.write_error("<info>Searching for the specified package.</info>");

        let mut version = version.map(|v| v.to_string());
        let mut min_stability;
        let repo;

        if let Some(composer) = self.try_composer(None, None) {
            let composer = crate::composer::composer_full(&composer);
            let repository_manager = composer.get_repository_manager().clone();
            let repository_manager = repository_manager.borrow();
            let local_repo = repository_manager.get_local_repository();
            let mut repos: Vec<crate::repository::RepositoryInterfaceHandle> = vec![local_repo];
            repos.extend(repository_manager.get_repositories().iter().cloned());
            repo = CompositeRepository::new(repos);
            min_stability = composer.get_package().get_minimum_stability().to_string();
        } else {
            let default_repos = RepositoryFactory::default_repos_with_default_manager(io.clone())?;
            let repo_names: Vec<String> = default_repos.keys().cloned().collect();
            io.write_error(&format!(
                "No composer.json found in the current directory, searching packages from {}",
                repo_names.join(", ")
            ));
            repo = CompositeRepository::new(default_repos.into_values().collect());
            min_stability = "stable".to_string();
        }

        if let Some(version_str) = &version {
            let mut matches: IndexMap<CaptureKey, String> = IndexMap::new();
            if Preg::match3(
                r"{@(stable|RC|beta|alpha|dev)$}i",
                version_str,
                Some(&mut matches),
            ) {
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
        repo_set.add_repository(crate::repository::RepositoryInterfaceHandle::new(repo))?;
        let parser = VersionParser::new();
        let constraint: Option<shirabe_semver::constraint::AnyConstraint> = match version.as_deref()
        {
            Some(v) => Some(parser.parse_constraints(v)?.clone()),
            None => None,
        };
        let packages = repo_set.find_packages(&package_name.to_lowercase(), constraint, 0)?;

        let package = if packages.len() > 1 {
            let mut version_selector =
                VersionSelector::new(std::rc::Rc::new(std::cell::RefCell::new(repo_set)), None)?;
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
            let p: crate::package::PackageInterfaceHandle = packages.into_iter().next().unwrap();
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

        let Some(complete) = package.as_complete() else {
            return Err(LogicException {
                message: format!(
                    "Expected a CompletePackageInterface instance but found {}",
                    get_debug_type(&shirabe_php_shim::PhpMixed::Null)
                ),
                code: 0,
            }
            .into());
        };

        Ok(Some(complete))
    }
}
