//! ref: composer/src/Composer/Command/StatusCommand.php

use crate::command::BaseCommand;
use crate::command::BaseCommandData;
use crate::command::base_command::base_command_initialize;
use crate::console::input::InputOption;
use crate::io::IOInterfaceImmutable;
use crate::package::dumper::ArrayDumper;
use crate::package::version::VersionGuesser;
use crate::package::version::VersionParser;
use crate::plugin::CommandEvent;
use crate::plugin::PluginEvents;
use crate::script::ScriptEvents;
use crate::util::ProcessExecutor;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::console::command::command::Command;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug)]
pub struct StatusCommand {
    base_command_data: BaseCommandData,
}

impl Default for StatusCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusCommand {
    const EXIT_CODE_ERRORS: i64 = 1;
    const EXIT_CODE_UNPUSHED_CHANGES: i64 = 2;
    const EXIT_CODE_VERSION_CHANGES: i64 = 4;

    pub fn new() -> Self {
        let command = StatusCommand {
            base_command_data: BaseCommandData::new(None),
        };
        command
            .configure()
            .expect("StatusCommand::configure uses static, valid metadata");
        command
    }

    fn do_execute(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
    ) -> Result<i64> {
        let composer = self.require_composer(None, None)?;
        let composer = crate::composer::composer_full(&composer);
        let io = self.get_io().clone();

        let mut errors: IndexMap<String, String> = IndexMap::new();
        let mut unpushed_changes: IndexMap<String, String> = IndexMap::new();
        let mut vcs_version_changes: IndexMap<String, IndexMap<String, IndexMap<String, String>>> =
            IndexMap::new();

        let parser = VersionParser::new();
        let process_executor = composer
            .get_loop()
            .borrow()
            .get_process_executor()
            .map(std::rc::Rc::clone)
            .unwrap_or_else(|| {
                std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
                    io.clone(),
                ))))
            });
        let mut guesser = VersionGuesser::new(
            composer.get_config(),
            process_executor.clone(),
            parser.clone(),
            Some(io.clone()),
        );
        let dumper = ArrayDumper::new();

        let dm = composer.get_download_manager().clone();
        let packages: Vec<_> = composer
            .get_repository_manager()
            .borrow()
            .get_local_repository()
            .get_canonical_packages()?;
        for package in packages {
            let target_dir = composer
                .get_installation_manager()
                .borrow_mut()
                .get_install_path(package.clone());
            let target_dir = match target_dir {
                Some(d) => d,
                None => continue,
            };
            let downloader_handle = match dm.borrow().get_downloader_for_package(package.clone())? {
                Some(d) => d,
                None => continue,
            };
            let mut downloader = downloader_handle.borrow_mut();

            if let Some(change_reporter) = downloader.as_change_report_interface() {
                if std::path::Path::new(&target_dir).is_symlink() {
                    errors.insert(
                        target_dir.clone(),
                        format!("{} is a symbolic link.", target_dir),
                    );
                }

                if let Some(changes) =
                    change_reporter.get_local_changes(package.clone(), &target_dir)?
                {
                    errors.insert(target_dir.clone(), changes);
                }
            }

            if let Some(vcs_downloader) = downloader.as_vcs_capable_downloader_interface()
                && vcs_downloader
                    .get_vcs_reference(package.clone(), target_dir.clone())
                    .is_some()
            {
                let previous_ref = match package.get_installation_source().as_deref() {
                    Some("source") => package.get_source_reference().map(|s| s.to_string()),
                    Some("dist") => package.get_dist_reference().map(|s| s.to_string()),
                    _ => None,
                };

                let current_version =
                    guesser.guess_version(&dumper.dump(package.clone()), &target_dir)?;

                if let (Some(prev_ref), Some(cur_version)) = (&previous_ref, &current_version)
                    && cur_version.commit.as_deref() != Some(prev_ref.as_str())
                    && cur_version.pretty_version.as_deref() != Some(prev_ref.as_str())
                {
                    let mut previous = IndexMap::new();
                    previous.insert(
                        "version".to_string(),
                        package.get_pretty_version().to_string(),
                    );
                    previous.insert("ref".to_string(), prev_ref.clone());

                    let mut current = IndexMap::new();
                    current.insert(
                        "version".to_string(),
                        cur_version.pretty_version.clone().unwrap_or_default(),
                    );
                    current.insert(
                        "ref".to_string(),
                        cur_version.commit.clone().unwrap_or_default(),
                    );

                    let mut change = IndexMap::new();
                    change.insert("previous".to_string(), previous);
                    change.insert("current".to_string(), current);

                    vcs_version_changes.insert(target_dir.clone(), change);
                }
            }

            if let Some(dvcs_downloader) = downloader.as_dvcs_downloader_interface()
                && let Some(unpushed) =
                    dvcs_downloader.get_unpushed_changes(package.clone(), target_dir.clone())?
            {
                unpushed_changes.insert(target_dir, unpushed);
            }
        }

        if errors.is_empty() && unpushed_changes.is_empty() && vcs_version_changes.is_empty() {
            io.write_error("<info>No local changes</info>");
            return Ok(0);
        }

        if !errors.is_empty() {
            io.write_error("<error>You have changes in the following dependencies:</error>");

            for (path, changes) in &errors {
                if input
                    .borrow()
                    .get_option("verbose")?
                    .as_bool()
                    .unwrap_or(false)
                {
                    let indented_changes = changes
                        .lines()
                        .map(|line| format!("    {}", line.trim_start()))
                        .collect::<Vec<_>>()
                        .join("\n");
                    io.write(&format!("<info>{}</info>:", path));
                    io.write(&indented_changes);
                } else {
                    io.write(path);
                }
            }
        }

        if !unpushed_changes.is_empty() {
            io.write_error("<warning>You have unpushed changes on the current branch in the following dependencies:</warning>");

            for (path, changes) in &unpushed_changes {
                if input
                    .borrow()
                    .get_option("verbose")?
                    .as_bool()
                    .unwrap_or(false)
                {
                    let indented_changes = changes
                        .lines()
                        .map(|line| format!("    {}", line.trim_start()))
                        .collect::<Vec<_>>()
                        .join("\n");
                    io.write(&format!("<info>{}</info>:", path));
                    io.write(&indented_changes);
                } else {
                    io.write(path);
                }
            }
        }

        if !vcs_version_changes.is_empty() {
            io.write_error(
                "<warning>You have version variations in the following dependencies:</warning>",
            );

            for (path, changes) in &vcs_version_changes {
                if input
                    .borrow()
                    .get_option("verbose")?
                    .as_bool()
                    .unwrap_or(false)
                {
                    let current_version = {
                        let v = changes["current"]
                            .get("version")
                            .map(|s| s.as_str())
                            .unwrap_or("");
                        let r = changes["current"]
                            .get("ref")
                            .map(|s| s.as_str())
                            .unwrap_or("");
                        if v.is_empty() {
                            r.to_string()
                        } else {
                            v.to_string()
                        }
                    };
                    let previous_version = {
                        let v = changes["previous"]
                            .get("version")
                            .map(|s| s.as_str())
                            .unwrap_or("");
                        let r = changes["previous"]
                            .get("ref")
                            .map(|s| s.as_str())
                            .unwrap_or("");
                        if v.is_empty() {
                            r.to_string()
                        } else {
                            v.to_string()
                        }
                    };

                    let (current_display, previous_display) = if io.is_very_verbose() {
                        let cur_ref = changes["current"]
                            .get("ref")
                            .map(|s| s.as_str())
                            .unwrap_or("");
                        let prev_ref = changes["previous"]
                            .get("ref")
                            .map(|s| s.as_str())
                            .unwrap_or("");
                        (
                            format!("{} ({})", current_version, cur_ref),
                            format!("{} ({})", previous_version, prev_ref),
                        )
                    } else {
                        (current_version, previous_version)
                    };

                    io.write(&format!("<info>{}</info>:", path));
                    io.write(&format!(
                        "    From <comment>{}</comment> to <comment>{}</comment>",
                        previous_display, current_display
                    ));
                } else {
                    io.write(path);
                }
            }
        }

        if (!errors.is_empty() || !unpushed_changes.is_empty() || !vcs_version_changes.is_empty())
            && !input
                .borrow()
                .get_option("verbose")?
                .as_bool()
                .unwrap_or(false)
        {
            io.write_error("Use --verbose (-v) to see a list of files");
        }

        let exit_code = (if !errors.is_empty() {
            Self::EXIT_CODE_ERRORS
        } else {
            0
        }) + (if !unpushed_changes.is_empty() {
            Self::EXIT_CODE_UNPUSHED_CHANGES
        } else {
            0
        }) + (if !vcs_version_changes.is_empty() {
            Self::EXIT_CODE_VERSION_CHANGES
        } else {
            0
        });

        Ok(exit_code)
    }
}

impl Command for StatusCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.set_name("status")?;
        self.set_description("Shows a list of locally modified packages");
        self.set_definition(&[InputOption::new(
            "verbose",
            Some(shirabe_php_shim::PhpMixed::String("v|vv|vvv".to_string())),
            Some(InputOption::VALUE_NONE),
            "Show modified files for each directory that contains changes.",
            None,
        )
        .unwrap()
        .into()]);
        self.set_help(
            "The status command displays a list of dependencies that have\nbeen modified locally.\n\nRead more at https://getcomposer.org/doc/03-cli.md#status"
        );
        Ok(())
    }

    fn execute(
        &self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let composer_rc = self.require_composer(None, None)?;
        {
            let composer = crate::composer::composer_full(&composer_rc);

            // TODO(plugin): dispatch CommandEvent
            let command_event =
                CommandEvent::new(PluginEvents::COMMAND, "status", input.clone(), output);
            composer
                .get_event_dispatcher()
                .borrow_mut()
                .dispatch(Some(command_event.get_name()), None);

            composer
                .get_event_dispatcher()
                .borrow_mut()
                .dispatch_script(
                    ScriptEvents::PRE_STATUS_CMD,
                    true,
                    vec![],
                    indexmap::IndexMap::new(),
                );
        }

        let exit_code = self.do_execute(input)?;

        {
            let composer = crate::composer::composer_full(&composer_rc);
            composer
                .get_event_dispatcher()
                .borrow_mut()
                .dispatch_script(
                    ScriptEvents::POST_STATUS_CMD,
                    true,
                    vec![],
                    indexmap::IndexMap::new(),
                );
        }

        Ok(exit_code)
    }

    fn initialize(
        &self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        base_command_initialize(self, input, output)
    }

    shirabe_external_packages::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for StatusCommand {
    fn command_data(
        &self,
    ) -> &shirabe_external_packages::symfony::console::command::command::CommandData {
        self.base_command_data.command_data()
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}
