//! ref: composer/src/Composer/Command/StatusCommand.php sha1:f06fd28e83aa8c1d61b61f3962a15f413cfd75af

use crate::command::base_command::base_command_initialize;
use crate::command::{BaseCommand, BaseCommandData};
use crate::console::input::InputOption;
use crate::event_dispatcher::EventFlags;
use crate::io::{IOInterfaceImmutable, IOInterfaceWrite};
use crate::package::dumper::ArrayDumper;
use crate::package::version::{VersionGuesser, VersionParser};
use crate::plugin::{CommandEvent, PluginEvents};
use crate::script::ScriptEvents;
use crate::util::ProcessExecutor;
use bstr::{B, BStr, BString};
use indexmap::IndexMap;
use shirabe_php_shim::{bconcat, explode, impl_php_class, implode, ltrim, path_to_bytes};
use shirabe_symfony_console::command::Command;
use shirabe_symfony_console::input::{InputInterface, InputInterfaceByName};
use shirabe_symfony_console::output::OutputInterface;

#[derive(Debug)]
pub struct StatusCommand {
    base_command_data: BaseCommandData,
}

impl_php_class!(StatusCommand, r"Composer\Command\StatusCommand");

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

    fn indent_changes(changes: &BStr) -> BString {
        let lines: Vec<BString> = explode("\n", changes)
            .iter()
            .map(|line| bconcat!("    ", ltrim(line)))
            .collect();
        implode("\n", &lines)
    }

    async fn do_execute(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
    ) -> anyhow::Result<i64> {
        let composer = self.require_composer(None, None)?;
        let composer = crate::composer::composer_full(&composer);
        let io = self.get_io().clone();

        let mut errors: IndexMap<BString, BString> = IndexMap::new();
        let mut unpushed_changes: IndexMap<BString, BString> = IndexMap::new();
        let mut vcs_version_changes: IndexMap<
            BString,
            IndexMap<BString, IndexMap<BString, BString>>,
        > = IndexMap::new();

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
            process_executor,
            parser,
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
            let target_path = match target_dir {
                Some(d) => d,
                None => continue,
            };
            let target_dir = BString::from(path_to_bytes(&target_path));
            let downloader_handle = match dm.borrow().get_downloader_for_package(package.clone())? {
                Some(d) => d,
                None => continue,
            };
            let downloader = downloader_handle.borrow();

            if let Some(change_reporter) = downloader.as_change_report_interface() {
                if target_path.is_symlink() {
                    errors.insert(
                        target_dir.clone(),
                        bconcat!(target_dir, " is a symbolic link."),
                    );
                }

                if let Some(changes) = change_reporter
                    .get_local_changes(package.clone(), &target_path)
                    .await?
                {
                    errors.insert(target_dir.clone(), changes);
                }
            }

            if let Some(vcs_downloader) = downloader.as_vcs_capable_downloader_interface()
                && vcs_downloader
                    .get_vcs_reference(package.clone(), &target_path)?
                    .is_some()
            {
                let previous_ref = match package
                    .get_installation_source()
                    .as_deref()
                    .map(|s| s.as_slice())
                {
                    Some(b"source") => package.get_source_reference(),
                    Some(b"dist") => package.get_dist_reference(),
                    _ => None,
                };

                let current_version =
                    guesser.guess_version(&dumper.dump(package.clone())?, &target_path)?;

                if let (Some(prev_ref), Some(cur_version)) = (&previous_ref, &current_version)
                    && cur_version.commit.as_ref() != Some(prev_ref)
                    && cur_version.pretty_version.as_ref() != Some(prev_ref)
                {
                    let mut previous = IndexMap::new();
                    previous.insert(BString::from("version"), package.get_pretty_version());
                    previous.insert(BString::from("ref"), prev_ref.clone());

                    let mut current = IndexMap::new();
                    current.insert(
                        BString::from("version"),
                        cur_version.pretty_version.clone().unwrap_or_default(),
                    );
                    current.insert(
                        BString::from("ref"),
                        cur_version.commit.clone().unwrap_or_default(),
                    );

                    let mut change = IndexMap::new();
                    change.insert(BString::from("previous"), previous);
                    change.insert(BString::from("current"), current);

                    vcs_version_changes.insert(target_dir.clone(), change);
                }
            }

            if let Some(dvcs_downloader) = downloader.as_dvcs_downloader_interface()
                && let Some(unpushed) =
                    dvcs_downloader.get_unpushed_changes(package.clone(), &target_path)?
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
                    let indented_changes = Self::indent_changes(BStr::new(changes));
                    io.write(bconcat!("<info>", path, "</info>:"));
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
                    let indented_changes = Self::indent_changes(BStr::new(changes));
                    io.write(bconcat!("<info>", path, "</info>:"));
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
                    let current_version = if changes[B("current")][B("version")].is_empty() {
                        changes[B("current")][B("ref")].clone()
                    } else {
                        changes[B("current")][B("version")].clone()
                    };
                    let previous_version = if changes[B("previous")][B("version")].is_empty() {
                        changes[B("previous")][B("ref")].clone()
                    } else {
                        changes[B("previous")][B("version")].clone()
                    };

                    let (current_display, previous_display) = if io.is_very_verbose() {
                        (
                            bconcat!(current_version, " (", changes[B("current")][B("ref")], ")"),
                            bconcat!(
                                previous_version,
                                " (",
                                changes[B("previous")][B("ref")],
                                ")"
                            ),
                        )
                    } else {
                        (current_version, previous_version)
                    };

                    io.write(bconcat!("<info>", path, "</info>:"));
                    io.write(bconcat!(
                        "    From <comment>",
                        previous_display,
                        "</comment> to <comment>",
                        current_display,
                        "</comment>"
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

#[async_trait::async_trait(?Send)]
impl Command for StatusCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.set_name("status".into())?;
        self.set_description("Shows a list of locally modified packages".into());
        self.set_definition(&[InputOption::new(
            "verbose",
            Some("v|vv|vvv".into()),
            Some(InputOption::VALUE_NONE),
            "Show modified files for each directory that contains changes.",
            None,
        )
        .unwrap()
        .into()]);
        self.set_help(
            "The status command displays a list of dependencies that have\nbeen modified locally.\n\nRead more at https://getcomposer.org/doc/03-cli.md#status".into()
        );
        Ok(())
    }

    async fn execute(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let composer_rc = self.require_composer(None, None)?;
        {
            let composer = crate::composer::composer_full(&composer_rc);

            let mut command_event =
                CommandEvent::new(PluginEvents::COMMAND, "status", input.clone(), output);
            let command_event_name = command_event.get_name().to_owned();
            composer
                .get_event_dispatcher()
                .borrow_mut()
                .dispatch(Some(command_event_name.as_ref()), Some(&mut command_event))?;

            composer
                .get_event_dispatcher()
                .borrow_mut()
                .dispatch_script(
                    ScriptEvents::PRE_STATUS_CMD,
                    true,
                    vec![],
                    EventFlags::default(),
                );
        }

        let exit_code = self.do_execute(input).await?;

        {
            let composer = crate::composer::composer_full(&composer_rc);
            composer
                .get_event_dispatcher()
                .borrow_mut()
                .dispatch_script(
                    ScriptEvents::POST_STATUS_CMD,
                    true,
                    vec![],
                    EventFlags::default(),
                );
        }

        Ok(exit_code)
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
        crate::command::base_command::base_command_complete(self, input, suggestions)
    }

    shirabe_symfony_console::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for StatusCommand {
    fn base_command_data(&self) -> &crate::command::BaseCommandData {
        &self.base_command_data
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}
