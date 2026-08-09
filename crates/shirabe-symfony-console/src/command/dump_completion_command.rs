//! ref: composer/vendor/symfony/console/Command/DumpCompletionCommand.php

use crate::command::command::{Command, CommandData};
use crate::completion::completion_input::CompletionInput;
use crate::completion::completion_suggestions::{CompletionSuggestions, StringOrSuggestion};
use crate::input::input_argument::InputArgument;
use crate::input::input_interface::InputInterface;
use crate::input::input_option::InputOption;
use crate::output::output_interface::{self, OutputInterface};
use shirabe_php_shim::{PhpMixed, impl_php_class};
use shirabe_symfony_process::process::Process;
use std::ops::{Deref, DerefMut};

/// __DIR__.'/../Resources/completion.bash', embedded at compile time (this port ships as a
/// single binary and does not install the Resources directory alongside it).
const COMPLETION_BASH: &str = include_str!("../Resources/completion.bash");

/// Dumps the completion script for the current shell.
#[derive(Debug)]
pub struct DumpCompletionCommand {
    inner: CommandData,
}

impl_php_class!(
    DumpCompletionCommand,
    r"Symfony\Component\Console\Command\DumpCompletionCommand"
);

impl Deref for DumpCompletionCommand {
    type Target = CommandData;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for DumpCompletionCommand {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl Default for DumpCompletionCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl DumpCompletionCommand {
    pub const DEFAULT_NAME: &'static str = "completion";
    pub const DEFAULT_DESCRIPTION: &'static str = "Dump the shell completion script";

    pub fn new() -> Self {
        let command = DumpCompletionCommand {
            inner: CommandData::new(None),
        };
        // PHP: static $defaultName = 'completion' / $defaultDescription, applied by the parent
        // constructor before configure().
        command
            .inner
            .apply_default_name(Self::DEFAULT_NAME)
            .expect("DumpCompletionCommand default name is valid");
        command.inner.set_description(Self::DEFAULT_DESCRIPTION);
        command
            .configure()
            .expect("DumpCompletionCommand::configure uses static, valid metadata");
        command
    }

    pub fn complete_impl(
        &self,
        input: &CompletionInput,
        suggestions: &mut CompletionSuggestions,
    ) -> anyhow::Result<()> {
        if input.must_suggest_argument_values_for("shell") {
            suggestions.suggest_values(
                self.get_supported_shells()?
                    .into_iter()
                    .map(StringOrSuggestion::String)
                    .collect(),
            );
        }
        Ok(())
    }

    fn guess_shell() -> String {
        shirabe_php_shim::basename(
            &shirabe_php_shim::PHP_SERVER
                .lock()
                .unwrap()
                .get("SHELL")
                .unwrap_or_default()
                .to_string_lossy(),
        )
    }

    /// The PHP closure captures `$output` by reference; `Process::run` needs a `'static`
    /// callback, so the shared handle is moved into it instead.
    fn tail_debug_log(
        &self,
        command_name: &str,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        let debug_file = format!(
            "{}/sf_{}.log",
            shirabe_php_shim::sys_get_temp_dir(),
            command_name
        );
        if !shirabe_php_shim::file_exists(&debug_file) {
            shirabe_php_shim::touch(&debug_file);
        }
        // new Process(['tail', '-f', $debugFile], null, null, null, 0) — timeout 0 disables it;
        // like PHP, this tails forever until the user interrupts.
        let mut process = Process::new(
            vec!["tail".to_string(), "-f".to_string(), debug_file],
            None,
            None,
            PhpMixed::Null,
            Some(0.0),
        )?;
        process.run(
            Some(Box::new(move |_type: &str, line: &str| {
                output.borrow_mut().write(
                    &[line.to_string()],
                    false,
                    output_interface::OUTPUT_NORMAL,
                );
                false
            })),
            indexmap::IndexMap::new(),
        )?;
        Ok(())
    }

    fn get_supported_shells(&self) -> anyhow::Result<Vec<String>> {
        // Deviation from PHP: the PHP implementation scans __DIR__.'/../Resources/' with a
        // DirectoryIterator at runtime; the resources are embedded at compile time in this
        // port, so the supported shells are a static list.
        Ok(vec!["bash".to_string()])
    }
}

impl Command for DumpCompletionCommand {
    fn configure(&self) -> anyhow::Result<()> {
        let full_command = shirabe_php_shim::PHP_SERVER
            .lock()
            .unwrap()
            .php_self()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let command_name = shirabe_php_shim::basename(&full_command);
        // @realpath($fullCommand) ?: $fullCommand
        let full_command = match shirabe_php_shim::realpath(&full_command) {
            Some(p) if !p.is_empty() => p,
            _ => full_command,
        };

        self.inner.set_help(&format!(
            "The <info>%command.name%</> command dumps the shell completion script required\n\
             to use shell autocompletion (currently only bash completion is supported).\n\
             \n\
             <comment>Static installation\n\
             -------------------</>\n\
             \n\
             Dump the script to a global completion file and restart your shell:\n\
             \n\
             \x20\x20\x20\x20<info>%command.full_name% bash | sudo tee /etc/bash_completion.d/{command_name}</>\n\
             \n\
             Or dump the script to a local file and source it:\n\
             \n\
             \x20\x20\x20\x20<info>%command.full_name% bash > completion.sh</>\n\
             \n\
             \x20\x20\x20\x20<comment># source the file whenever you use the project</>\n\
             \x20\x20\x20\x20<info>source completion.sh</>\n\
             \n\
             \x20\x20\x20\x20<comment># or add this line at the end of your \"~/.bashrc\" file:</>\n\
             \x20\x20\x20\x20<info>source /path/to/completion.sh</>\n\
             \n\
             <comment>Dynamic installation\n\
             --------------------</>\n\
             \n\
             Add this to the end of your shell configuration file (e.g. <info>\"~/.bashrc\"</>):\n\
             \n\
             \x20\x20\x20\x20<info>eval \"$({full_command} completion bash)\"</>",
        ));
        self.inner.add_argument(
            "shell",
            Some(InputArgument::OPTIONAL),
            "The shell type (e.g. \"bash\"), the value of the \"$SHELL\" env var will be used if this is not given",
            PhpMixed::Null,
        )?;
        self.inner.add_option(
            "debug",
            PhpMixed::Null,
            Some(InputOption::VALUE_NONE),
            "Tail the completion debug log",
            PhpMixed::Null,
        )?;

        Ok(())
    }

    fn execute(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let command_name = shirabe_php_shim::basename(
            &shirabe_php_shim::PHP_SERVER
                .lock()
                .unwrap()
                .argv()
                .next()
                .unwrap_or_default()
                .to_string_lossy(),
        );

        if input.borrow().get_option("debug")?.to_bool() {
            self.tail_debug_log(&command_name, output.clone())?;

            return Ok(0);
        }

        let shell = match input.borrow().get_argument("shell")?.as_string() {
            Some(s) => s.to_string(),
            None => Self::guess_shell(),
        };
        // __DIR__.'/../Resources/completion.'.$shell — resolved against the embedded
        // resources; a shell without an embedded script is PHP's !file_exists() branch.
        let completion_file = match shell.as_str() {
            "bash" => Some(COMPLETION_BASH),
            _ => None,
        };
        let Some(completion_file) = completion_file else {
            let supported_shells = self.get_supported_shells()?;

            // if ($output instanceof ConsoleOutputInterface) { $output = $output->getErrorOutput(); }
            let output = {
                let error_output = output
                    .borrow()
                    .as_console_output()
                    .map(|console_output| console_output.get_error_output());
                error_output.unwrap_or_else(|| output.clone())
            };
            if !shell.is_empty() {
                output.borrow_mut().writeln(
                    &[format!(
                        "<error>Detected shell \"{}\", which is not supported by Symfony shell completion (supported shells: \"{}\").</>",
                        shell,
                        supported_shells.join("\", \"")
                    )],
                    output_interface::OUTPUT_NORMAL,
                );
            } else {
                output.borrow_mut().writeln(
                    &[format!(
                        "<error>Shell not detected, Symfony shell completion only supports \"{}\").</>",
                        supported_shells.join("\", \"")
                    )],
                    output_interface::OUTPUT_NORMAL,
                );
            }

            return Ok(2);
        };

        let application = self.get_application().unwrap();
        let version = application.borrow().get_version();
        output.borrow_mut().write(
            &[shirabe_php_shim::str_replace_arrays(
                &[
                    "{{ COMMAND_NAME }}".to_string(),
                    "{{ VERSION }}".to_string(),
                ],
                &[command_name, version],
                completion_file,
            )],
            false,
            output_interface::OUTPUT_NORMAL,
        );

        Ok(0)
    }

    fn complete(
        &self,
        input: &CompletionInput,
        suggestions: &mut CompletionSuggestions,
    ) -> anyhow::Result<()> {
        self.complete_impl(input, suggestions)
    }

    crate::delegate_command_trait_impls_to_inner!(inner);
}
