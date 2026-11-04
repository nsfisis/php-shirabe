//! ref: composer/src/Composer/Command/ExecCommand.php

use crate::command::base_command::base_command_initialize;
use crate::command::{BaseCommand, BaseCommandData};
use crate::console::Application;
use crate::console::input::{InputArgument, InputOption};
use crate::event_dispatcher::EventFlags;
use crate::io::IOInterfaceWrite;
use bstr::BString;
use shirabe_php_shim::{
    RuntimeException, basename, bconcat, canonical_int_key, chdir, getcwd, glob, impl_php_class,
};
use shirabe_symfony_console::command::Command;
use shirabe_symfony_console::input::{InputInterface, InputInterfaceByName, InputValue};
use shirabe_symfony_console::output::OutputInterface;

#[derive(Debug)]
pub struct ExecCommand {
    base_command_data: BaseCommandData,
}

impl_php_class!(ExecCommand, r"Composer\Command\ExecCommand");

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

    fn get_binaries(&self, for_display: bool) -> anyhow::Result<Vec<BString>> {
        let composer = self.require_composer(None, None)?;
        let composer_ref = crate::composer::composer_full(&composer);
        let bin_dir = composer_ref.get_config().borrow().get_path("bin-dir")?;
        let bins = glob(bconcat!(bin_dir, "/*"));
        let local_bins_raw = composer_ref.get_package().get_binaries();
        let local_bins: Vec<BString> = if for_display {
            local_bins_raw
                .into_iter()
                .map(|e| bconcat!(e, " (local)"))
                .collect()
        } else {
            local_bins_raw
        };

        let mut binaries: Vec<BString> = Vec::new();
        let mut previous_bin: Option<&BString> = None;
        for bin in bins.iter().chain(local_bins.iter()) {
            if let Some(prev) = previous_bin
                && *bin == bconcat!(prev, ".bat")
            {
                continue;
            }
            previous_bin = Some(bin);
            binaries.push(basename(bin));
        }

        Ok(binaries)
    }
}

#[async_trait::async_trait(?Send)]
impl Command for ExecCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.set_name("exec".into())?;
        self.set_description("Executes a vendored binary/script".into());
        self.set_definition(&[
            InputOption::new("list", Some("l".into()), Some(InputOption::VALUE_NONE), "", None).unwrap().into(),
            // PHP passes an inline closure here (it takes no arguments; PHP tolerates the
            // extra ones the caller passes).
            InputArgument::new5("binary",
            Some(InputArgument::OPTIONAL),
            "The binary to run, e.g. phpunit",
            None,
            crate::console::input::SuggestedValues::Closure(Box::new(|this, _input, _suggestions| {
                let this = this
                    .as_any()
                    .downcast_ref::<ExecCommand>()
                    .expect("the binary suggestions are bound to ExecCommand");
                this.get_binaries(false)
            }))).unwrap().into(),
            InputArgument::new("args",
            Some(InputArgument::IS_ARRAY | InputArgument::OPTIONAL),
            "Arguments to pass to the binary. Use <info>--</info> to separate from composer arguments",
            None).unwrap().into(),
        ]);
        self.set_help(
            "Executes a vendored binary/script.\n\n\
            Read more at https://getcomposer.org/doc/03-cli.md#exec"
                .into(),
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
                "Binary to run: ",
                binaries
                    .iter()
                    .enumerate()
                    .map(|(index, binary)| (bconcat!(index), binary.clone()))
                    .collect(),
                InputValue::from(""),
                Some(1),
                "Invalid binary name \"%s\"",
                false,
            )?;

            // $binaries[$binary]
            let index = binary
                .as_string()
                .and_then(|index| canonical_int_key(index))
                .expect("select() resolves a list choice to its index");
            input.borrow_mut().set_argument(
                "binary",
                InputValue::String(binaries[index as usize].clone()),
            );

            Ok(())
        })();
    }

    async fn execute(
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
                    .borrow()
                    .get_path("bin-dir")?;
                return Err(RuntimeException::new(bconcat!(
                    "No binaries found in composer.json or in bin-dir (",
                    bin_dir,
                    ")"
                ))
                .into());
            }

            self.get_io()
                .write("<comment>Available binaries:</comment>");
            for bin in &bins {
                self.get_io().write(bconcat!("<info>- ", bin, "</info>"));
            }

            return Ok(0);
        }

        let binary = BString::from(
            input
                .borrow()
                .get_argument("binary")?
                .as_string()
                .unwrap_or_default(),
        );

        let dispatcher = crate::composer::composer_full(&composer)
            .get_event_dispatcher()
            .clone();
        dispatcher.borrow_mut().add_listener(
            "__exec_command".into(),
            crate::event_dispatcher::Callable::String(binary),
            0,
        );

        let initial_working_directory = self.get_application().and_then(|application| {
            let application = application.borrow();
            let app_dyn: &dyn shirabe_symfony_console::application::Application = &*application;
            app_dyn
                .as_any()
                .downcast_ref::<Application>()
                .expect("a Composer command's application is a shirabe Application")
                .get_initial_working_directory()
        });
        if let Some(ref iwd) = initial_working_directory
            && getcwd().as_deref() != Some(iwd.as_path())
        {
            chdir(iwd).map_err(|e| {
                RuntimeException::with_code_and_previous(
                    bconcat!("Could not switch back to working directory \"", iwd, "\""),
                    0,
                    Some(std::sync::Arc::new(e)),
                )
            })?;
        }

        let args = input
            .borrow()
            .get_argument("args")?
            .as_array()
            .map(<[bstr::BString]>::to_vec)
            .unwrap_or_default();

        dispatcher.borrow_mut().dispatch_script(
            "__exec_command".into(),
            true,
            args,
            EventFlags::default(),
        )
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

impl BaseCommand for ExecCommand {
    fn base_command_data(&self) -> &crate::command::BaseCommandData {
        &self.base_command_data
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}
