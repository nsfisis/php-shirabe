//! ref: composer/vendor/symfony/console/Command/DumpCompletionCommand.php

use crate::symfony::console::command::command::{Command, CommandData};
use crate::symfony::console::completion::completion_input::CompletionInput;
use crate::symfony::console::completion::completion_suggestions::{
    CompletionSuggestions, StringOrSuggestion,
};
use crate::symfony::console::input::input_argument::InputArgument;
use crate::symfony::console::input::input_interface::InputInterface;
use crate::symfony::console::input::input_option::InputOption;
use crate::symfony::console::output::output_interface::{self, OutputInterface};
use shirabe_php_shim::PhpMixed;
use std::cell::RefCell;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

/// Dumps the completion script for the current shell.
#[derive(Debug)]
pub struct DumpCompletionCommand {
    inner: CommandData,
}

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

impl DumpCompletionCommand {
    pub const DEFAULT_NAME: &'static str = "completion";
    pub const DEFAULT_DESCRIPTION: &'static str = "Dump the shell completion script";

    pub fn new() -> Self {
        let mut command = DumpCompletionCommand {
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

    pub fn complete_impl(&self, input: &CompletionInput, suggestions: &mut CompletionSuggestions) {
        if input.must_suggest_argument_values_for("shell") {
            suggestions.suggest_values(
                self.get_supported_shells()
                    .into_iter()
                    .map(StringOrSuggestion::String)
                    .collect(),
            );
        }
    }

    fn guess_shell() -> String {
        shirabe_php_shim::basename(&shirabe_php_shim::server_shell().unwrap_or_default())
    }

    fn tail_debug_log(&self, command_name: &str, _output: &dyn OutputInterface) {
        let debug_file = format!(
            "{}/sf_{}.log",
            shirabe_php_shim::sys_get_temp_dir(),
            command_name
        );
        if !shirabe_php_shim::file_exists(&debug_file) {
            shirabe_php_shim::touch(&debug_file);
        }
        // TODO: Process::run() expects a `'static` callback, but the PHP closure captures
        // `$output` by reference and writes each line to it. Bridging the borrowed `output`
        // into a `'static` callback requires shared ownership of the output (Phase C).
        todo!()
    }

    fn get_supported_shells(&self) -> Vec<String> {
        let mut shells = vec![];

        // foreach (new \DirectoryIterator(__DIR__.'/../Resources/') as $file)
        for file in shirabe_php_shim::directory_iterator(&format!(
            "{}/../Resources/",
            shirabe_php_shim::dir()
        )) {
            if shirabe_php_shim::str_starts_with(&file.get_basename(), "completion.")
                && file.is_file()
            {
                shells.push(file.get_extension());
            }
        }

        shells
    }
}

impl Command for DumpCompletionCommand {
    fn configure(&mut self) -> anyhow::Result<()> {
        let full_command = shirabe_php_shim::server_php_self();
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
        &mut self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let command_name = shirabe_php_shim::basename(&shirabe_php_shim::server_argv()[0]);

        if input.borrow().get_option("debug")?.to_bool() {
            self.tail_debug_log(&command_name, &*output.borrow());

            return Ok(0);
        }

        let shell = match input.borrow().get_argument("shell")?.as_string() {
            Some(s) => s.to_string(),
            None => Self::guess_shell(),
        };
        let completion_file = format!(
            "{}/../Resources/completion.{}",
            shirabe_php_shim::dir(),
            shell
        );
        if !shirabe_php_shim::file_exists(&completion_file) {
            let supported_shells = self.get_supported_shells();

            // TODO: PHP does `$output instanceof ConsoleOutputInterface ? $output->getErrorOutput()
            // : $output`. There is no way to test trait membership through `&dyn OutputInterface`
            // here; OutputInterface would need a downcast hook (Phase C). Writing to `output`.
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
        }

        let application = self.get_application().unwrap();
        let version = application.borrow().get_version();
        output.borrow_mut().write(
            &[shirabe_php_shim::str_replace_arrays(
                &[
                    "{{ COMMAND_NAME }}".to_string(),
                    "{{ VERSION }}".to_string(),
                ],
                &[command_name, version],
                &shirabe_php_shim::file_get_contents(&completion_file).unwrap_or_default(),
            )],
            false,
            output_interface::OUTPUT_NORMAL,
        );

        Ok(0)
    }

    fn complete(&self, input: &CompletionInput, suggestions: &mut CompletionSuggestions) {
        self.complete_impl(input, suggestions);
    }

    crate::delegate_command_trait_impls_to_inner!(inner);
}
