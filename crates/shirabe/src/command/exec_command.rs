//! ref: composer/src/Composer/Command/ExecCommand.php

use anyhow::Result;
use shirabe_external_packages::symfony::component::console::input::InputInterface;
use shirabe_external_packages::symfony::component::console::output::OutputInterface;
use shirabe_php_shim::{PhpMixed, RuntimeException, basename, chdir, getcwd, glob};

use crate::command::{BaseCommand, BaseCommandData, HasBaseCommandData};
use crate::console::input::InputArgument;
use crate::console::input::InputOption;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;

#[derive(Debug)]
pub struct ExecCommand {
    base_command_data: BaseCommandData,
}

impl ExecCommand {
    pub fn configure(&mut self) {
        self
            .set_name("exec")
            .set_description("Executes a vendored binary/script")
            .set_definition(&[
                InputOption::new("list", Some(PhpMixed::String("l".to_string())), Some(InputOption::VALUE_NONE), "", None).unwrap().into(),
                // TODO(cli-completion): suggest installed binary names (via get_binaries) for `binary` argument
                InputArgument::new("binary",
                Some(InputArgument::OPTIONAL),
                "The binary to run, e.g. phpunit",
                None,).unwrap().into(),
                InputArgument::new("args",
                Some(InputArgument::IS_ARRAY | InputArgument::OPTIONAL),
                "Arguments to pass to the binary. Use <info>--</info> to separate from composer arguments",
                None,).unwrap().into(),
            ])
            .set_help(
                "Executes a vendored binary/script.\n\n\
                Read more at https://getcomposer.org/doc/03-cli.md#exec"
            );
    }

    pub fn interact(
        &mut self,
        input: &mut dyn InputInterface,
        _output: &dyn OutputInterface,
    ) -> Result<()> {
        let binaries = self.get_binaries(false)?;
        if binaries.is_empty() {
            return Ok(());
        }

        if input.get_argument("binary").as_string().is_some()
            || input.get_option("list").as_bool().unwrap_or(false)
        {
            return Ok(());
        }

        let io = self.get_io();
        let binary = io.select(
            "Binary to run: ".to_string(),
            binaries.clone(),
            PhpMixed::String(String::new()),
            PhpMixed::Int(1),
            "Invalid binary name \"%s\"".to_string(),
            false,
        );

        if let Some(idx) = binary.as_int() {
            input.set_argument(
                "binary",
                shirabe_php_shim::PhpMixed::String(binaries[idx as usize].clone()),
            );
        }

        Ok(())
    }

    pub fn execute(
        &mut self,
        input: &dyn InputInterface,
        _output: &dyn OutputInterface,
    ) -> Result<i64> {
        let composer = self.require_composer(None, None)?;

        if input.get_option("list").as_bool().unwrap_or(false)
            || input.get_argument("binary").as_string().is_none()
        {
            let bins = self.get_binaries(true)?;
            if bins.is_empty() {
                let bin_dir = crate::command::composer_full_mut(&composer)
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
            .get_argument("binary")
            .as_string()
            .unwrap_or("")
            .to_string();

        let dispatcher = crate::command::composer_full(&composer)
            .get_event_dispatcher()
            .clone();
        dispatcher.borrow_mut().add_listener(
            "__exec_command",
            crate::event_dispatcher::Callable::String(binary),
            0,
        );

        let initial_working_directory = self.get_application()?.get_initial_working_directory();
        if let Some(ref iwd) = initial_working_directory {
            if getcwd().as_deref() != Some(iwd.as_str()) {
                chdir(iwd).map_err(|e| RuntimeException {
                    message: format!("Could not switch back to working directory \"{}\"", iwd),
                    code: 0,
                })?;
            }
        }

        let args = input
            .get_argument("args")
            .as_list()
            .map(|l| {
                l.iter()
                    .filter_map(|v| v.as_string().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        Ok(dispatcher.borrow_mut().dispatch_script(
            "__exec_command",
            true,
            args,
            indexmap::IndexMap::new(),
        )?)
    }

    fn get_binaries(&mut self, for_display: bool) -> Result<Vec<String>> {
        let composer = self.require_composer(None, None)?;
        let composer_ref = crate::command::composer_full_mut(&composer);
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
            if let Some(prev) = &previous_bin {
                if bin == &format!("{}.bat", prev) {
                    continue;
                }
            }
            previous_bin = Some(bin.clone());
            binaries.push(basename(bin));
        }

        Ok(binaries)
    }
}

impl HasBaseCommandData for ExecCommand {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    fn base_command_data_mut(&mut self) -> &mut BaseCommandData {
        &mut self.base_command_data
    }
}
