//! ref: composer/src/Composer/Command/StatusCommand.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::component::console::command::command::Command;
use shirabe_external_packages::symfony::component::console::command::command::CommandBase;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;

use crate::command::base_command::BaseCommand;
use crate::composer::Composer;
use crate::console::input::input_option::InputOption;
use crate::io::io_interface::IOInterface;
use crate::package::dumper::array_dumper::ArrayDumper;
use crate::package::version::version_guesser::VersionGuesser;
use crate::package::version::version_parser::VersionParser;
use crate::plugin::command_event::CommandEvent;
use crate::plugin::plugin_events::PluginEvents;
use crate::script::script_events::ScriptEvents;
use crate::util::process_executor::ProcessExecutor;

#[derive(Debug)]
pub struct StatusCommand {
    inner: CommandBase,
    composer: Option<Composer>,
    io: Option<Box<dyn IOInterface>>,
}

impl StatusCommand {
    const EXIT_CODE_ERRORS: i64 = 1;
    const EXIT_CODE_UNPUSHED_CHANGES: i64 = 2;
    const EXIT_CODE_VERSION_CHANGES: i64 = 4;

    pub fn configure(&mut self) {
        self.inner
            .set_name("status")
            .set_description("Shows a list of locally modified packages")
            .set_definition(vec![
                InputOption::new("verbose", Some(shirabe_php_shim::PhpMixed::String("v|vv|vvv".to_string())), Some(InputOption::VALUE_NONE), "Show modified files for each directory that contains changes.", None, vec![]),
            ])
            .set_help(
                "The status command displays a list of dependencies that have\nbeen modified locally.\n\nRead more at https://getcomposer.org/doc/03-cli.md#status"
            );
    }

    pub fn execute(&self, input: &dyn InputInterface, output: &dyn OutputInterface) -> Result<i64> {
        let composer = self.inner.require_composer()?;

        // TODO(plugin): dispatch CommandEvent
        let command_event = CommandEvent::new(
            PluginEvents::COMMAND.to_string(),
            "status".to_string(),
            Box::new(input),
            Box::new(output),
            vec![],
            vec![],
        );
        composer
            .get_event_dispatcher()
            .dispatch(command_event.get_name(), &command_event);

        composer
            .get_event_dispatcher()
            .dispatch_script(ScriptEvents::PRE_STATUS_CMD, true);

        let exit_code = self.do_execute(input)?;

        composer
            .get_event_dispatcher()
            .dispatch_script(ScriptEvents::POST_STATUS_CMD, true);

        Ok(exit_code)
    }

    fn do_execute(&self, input: &dyn InputInterface) -> Result<i64> {
        let composer = self.inner.require_composer()?;

        let installed_repo = composer.get_repository_manager().get_local_repository();

        let dm = composer.get_download_manager();
        let im = composer.get_installation_manager();

        let mut errors: IndexMap<String, String> = IndexMap::new();
        let io = self.inner.get_io();
        let mut unpushed_changes: IndexMap<String, String> = IndexMap::new();
        let mut vcs_version_changes: IndexMap<String, IndexMap<String, IndexMap<String, String>>> =
            IndexMap::new();

        let parser = VersionParser::new();
        let process_executor = composer
            .get_loop()
            .get_process_executor()
            .cloned()
            .unwrap_or_else(|| ProcessExecutor::new(io));
        let guesser = VersionGuesser::new(composer.get_config(), &process_executor, &parser, io);
        let dumper = ArrayDumper::new();

        for package in installed_repo.get_canonical_packages() {
            let downloader = dm.get_downloader_for_package(package.as_ref());
            let target_dir = im.get_install_path(package.as_ref());
            let target_dir = match target_dir {
                Some(d) => d,
                None => continue,
            };

            // TODO(phase-b): isinstance checks using ChangeReportInterface/VcsCapableDownloaderInterface/DvcsDownloaderInterface
            if let Some(change_reporter) = downloader.as_change_report_interface() {
                if std::path::Path::new(&target_dir).is_symlink() {
                    errors.insert(
                        target_dir.clone(),
                        format!("{} is a symbolic link.", target_dir),
                    );
                }

                if let Some(changes) =
                    change_reporter.get_local_changes(package.as_ref(), &target_dir)?
                {
                    errors.insert(target_dir.clone(), changes);
                }
            }

            if let Some(vcs_downloader) = downloader.as_vcs_capable_downloader_interface() {
                if vcs_downloader
                    .get_vcs_reference(package.as_ref(), target_dir.clone())
                    .is_some()
                {
                    let previous_ref = match package.get_installation_source().as_deref() {
                        Some("source") => package.get_source_reference().map(|s| s.to_string()),
                        Some("dist") => package.get_dist_reference().map(|s| s.to_string()),
                        _ => None,
                    };

                    let current_version =
                        guesser.guess_version(&dumper.dump(package.as_ref()), &target_dir);

                    if let (Some(prev_ref), Some(cur_version)) = (&previous_ref, &current_version) {
                        if cur_version.get("commit").map(|s| s.as_str()) != Some(prev_ref.as_str())
                            && cur_version.get("pretty_version").map(|s| s.as_str())
                                != Some(prev_ref.as_str())
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
                                cur_version
                                    .get("pretty_version")
                                    .cloned()
                                    .unwrap_or_default(),
                            );
                            current.insert(
                                "ref".to_string(),
                                cur_version.get("commit").cloned().unwrap_or_default(),
                            );

                            let mut change = IndexMap::new();
                            change.insert("previous".to_string(), previous);
                            change.insert("current".to_string(), current);

                            vcs_version_changes.insert(target_dir.clone(), change);
                        }
                    }
                }
            }

            if let Some(dvcs_downloader) = downloader.as_dvcs_downloader_interface() {
                if let Some(unpushed) =
                    dvcs_downloader.get_unpushed_changes(package.as_ref(), target_dir.clone())
                {
                    unpushed_changes.insert(target_dir, unpushed);
                }
            }
        }

        if errors.is_empty() && unpushed_changes.is_empty() && vcs_version_changes.is_empty() {
            io.write_error("<info>No local changes</info>");
            return Ok(0);
        }

        if !errors.is_empty() {
            io.write_error("<error>You have changes in the following dependencies:</error>");

            for (path, changes) in &errors {
                if input.get_option("verbose").as_bool().unwrap_or(false) {
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
                if input.get_option("verbose").as_bool().unwrap_or(false) {
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
                if input.get_option("verbose").as_bool().unwrap_or(false) {
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
            && !input.get_option("verbose").as_bool().unwrap_or(false)
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

impl BaseCommand for StatusCommand {
    fn inner(&self) -> &CommandBase {
        &self.inner
    }

    fn inner_mut(&mut self) -> &mut CommandBase {
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

impl Command for StatusCommand {}
