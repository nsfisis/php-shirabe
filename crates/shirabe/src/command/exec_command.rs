//! ref: composer/src/Composer/Command/ExecCommand.php

use crate::command::BaseCommand;
use crate::command::BaseCommandData;
use crate::command::base_command::base_command_initialize;
use crate::console::Application;
use crate::console::input::InputArgument;
use crate::console::input::InputOption;
use crate::io::IOInterfaceImmutable;
use shirabe_external_packages::symfony::console::command::command::Command;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use shirabe_php_shim::{PhpMixed, RuntimeException, basename, chdir, getcwd, glob};

#[derive(Debug)]
pub struct ExecCommand {
    base_command_data: BaseCommandData,
}

impl Default for ExecCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecCommand {
    pub fn new() -> Self {
        let command = ExecCommand {
            base_command_data: BaseCommandData::new(None),
        };
        command
            .configure()
            .expect("ExecCommand::configure uses static, valid metadata");
        command
    }

    fn get_binaries(&self, for_display: bool) -> anyhow::Result<Vec<String>> {
        let composer = self.require_composer(None, None)?;
        let composer_ref = crate::composer::composer_full(&composer);
        let bin_dir = composer_ref
            .get_config()
            .borrow_mut()
            .get("bin-dir")
            .as_string()
            .unwrap_or("")
            .to_string();
        let bins = glob(&format!("{}/*", bin_dir));
        let local_bins_raw: Vec<String> = composer_ref.get_package().get_binaries();
        let local_bins: Vec<String> = if for_display {
            local_bins_raw
                .into_iter()
                .map(|e| format!("{} (local)", e))
                .collect()
        } else {
            local_bins_raw
        };

        let mut binaries: Vec<String> = Vec::new();
        let mut previous_bin: Option<String> = None;
        for bin in bins.iter().chain(local_bins.iter()) {
            if let Some(prev) = &previous_bin
                && bin == &format!("{}.bat", prev)
            {
                continue;
            }
            previous_bin = Some(bin.clone());
            binaries.push(basename(bin));
        }

        Ok(binaries)
    }
}

impl Command for ExecCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.set_name("exec")?;
        self.set_description("Executes a vendored binary/script");
        self.set_definition(&[
            InputOption::new("list", Some(PhpMixed::String("l".to_string())), Some(InputOption::VALUE_NONE), "", None).unwrap().into(),
            // TODO(cli-completion): suggest installed binary names (via get_binaries) for `binary` argument
            InputArgument::new("binary",
            Some(InputArgument::OPTIONAL),
            "The binary to run, e.g. phpunit",
            None).unwrap().into(),
            InputArgument::new("args",
            Some(InputArgument::IS_ARRAY | InputArgument::OPTIONAL),
            "Arguments to pass to the binary. Use <info>--</info> to separate from composer arguments",
            None).unwrap().into(),
        ]);
        self.set_help(
            "Executes a vendored binary/script.\n\n\
            Read more at https://getcomposer.org/doc/03-cli.md#exec",
        );
        Ok(())
    }

    fn interact(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        _output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) {
        let _ = (|| -> anyhow::Result<()> {
            let binaries = self.get_binaries(false)?;
            if binaries.is_empty() {
                return Ok(());
            }

            if input.borrow().get_argument("binary")?.as_string().is_some()
                || input
                    .borrow()
                    .get_option("list")?
                    .as_bool()
                    .unwrap_or(false)
            {
                return Ok(());
            }

            let io = self.get_io();
            let binary = io.select(
                "Binary to run: ".to_string(),
                PhpMixed::List(
                    binaries
                        .iter()
                        .map(|b| PhpMixed::String(b.clone()))
                        .collect(),
                ),
                PhpMixed::String(String::new()),
                PhpMixed::Int(1),
                "Invalid binary name \"%s\"".to_string(),
                false,
            );

            if let Some(idx) = binary.as_int() {
                input.borrow_mut().set_argument(
                    "binary",
                    shirabe_php_shim::PhpMixed::String(binaries[idx as usize].clone()),
                );
            }

            Ok(())
        })();
    }

    fn execute(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        _output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let composer = self.require_composer(None, None)?;

        if input
            .borrow()
            .get_option("list")?
            .as_bool()
            .unwrap_or(false)
            || input.borrow().get_argument("binary")?.as_string().is_none()
        {
            let bins = self.get_binaries(true)?;
            if bins.is_empty() {
                let bin_dir = crate::composer::composer_full(&composer)
                    .get_config()
                    .borrow_mut()
                    .get("bin-dir")
                    .as_string()
                    .unwrap_or("")
                    .to_string();
                return Err(RuntimeException {
                    message: format!(
                        "No binaries found in composer.json or in bin-dir ({})",
                        bin_dir
                    ),
                    code: 0,
                }
                .into());
            }

            self.get_io()
                .write("<comment>Available binaries:</comment>");
            for bin in &bins {
                self.get_io().write(&format!("<info>- {}</info>", bin));
            }

            return Ok(0);
        }

        let binary = input
            .borrow()
            .get_argument("binary")?
            .as_string()
            .unwrap_or("")
            .to_string();

        let dispatcher = crate::composer::composer_full(&composer)
            .get_event_dispatcher()
            .clone();
        dispatcher.borrow_mut().add_listener(
            "__exec_command",
            crate::event_dispatcher::Callable::String(binary),
            0,
        );

        let initial_working_directory = self.get_application().and_then(|application| {
            let application = application.borrow();
            let app_dyn: &dyn shirabe_external_packages::symfony::console::application::Application =
                &*application;
            app_dyn
                .as_any()
                .downcast_ref::<Application>()
                .expect("a Composer command's application is a shirabe Application")
                .get_initial_working_directory()
        });
        if let Some(ref iwd) = initial_working_directory
            && getcwd().as_deref() != Some(iwd.as_str())
        {
            chdir(iwd).map_err(|e| RuntimeException {
                message: format!("Could not switch back to working directory \"{}\"", iwd),
                code: 0,
            })?;
        }

        let args = input
            .borrow()
            .get_argument("args")?
            .as_list()
            .map(|l| {
                l.iter()
                    .filter_map(|v| v.as_string().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        dispatcher.borrow_mut().dispatch_script(
            "__exec_command",
            true,
            args,
            indexmap::IndexMap::new(),
        )
    }

    fn initialize(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        base_command_initialize(self, input, output)
    }

    shirabe_external_packages::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for ExecCommand {
    fn command_data(
        &self,
    ) -> &shirabe_external_packages::symfony::console::command::command::CommandData {
        self.base_command_data.command_data()
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}
