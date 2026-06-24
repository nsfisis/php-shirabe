//! ref: composer/vendor/symfony/console/Command/CompleteCommand.php

use indexmap::IndexMap;
use shirabe_php_shim::AsAny;
use shirabe_php_shim::PhpMixed;
use std::cell::RefCell;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use crate::symfony::console::command::command::{Command, CommandData};
use crate::symfony::console::completion::completion_input::CompletionInput;
use crate::symfony::console::completion::completion_suggestions::{
    CompletionSuggestions, StringOrSuggestion,
};
use crate::symfony::console::completion::output::completion_output_interface::CompletionOutputInterface;
use crate::symfony::console::input::input_interface::InputInterface;
use crate::symfony::console::input::input_option::InputOption;
use crate::symfony::console::output::output_interface::OutputInterface;

/// Responsible for providing the values to the shell completion.
#[derive(Debug)]
pub struct CompleteCommand {
    inner: CommandData,
    completion_outputs: IndexMap<String, PhpMixed>,
    is_debug: std::cell::Cell<bool>,
}

impl Deref for CompleteCommand {
    type Target = CommandData;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for CompleteCommand {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl CompleteCommand {
    pub const DEFAULT_NAME: &'static str = "|_complete";
    pub const DEFAULT_DESCRIPTION: &'static str =
        "Internal command to provide shell completion suggestions";

    /// @param completion_outputs A list of additional completion outputs, with shell name as
    /// key and FQCN as value
    pub fn new(completion_outputs: IndexMap<String, PhpMixed>) -> anyhow::Result<Self> {
        // must be set before the parent constructor, as the property value is used in configure()
        let mut completion_outputs = completion_outputs;
        // $completionOutputs + ['bash' => BashCompletionOutput::class]
        completion_outputs
            .entry("bash".to_string())
            .or_insert_with(|| {
                PhpMixed::from(
                    "Symfony\\Component\\Console\\Completion\\Output\\BashCompletionOutput"
                        .to_string(),
                )
            });

        let this = Self {
            inner: CommandData::new(None),
            completion_outputs,
            is_debug: std::cell::Cell::new(false),
        };
        // PHP: static $defaultName = '|_complete' / $defaultDescription, applied by the parent
        // constructor before configure().
        this.inner.apply_default_name(Self::DEFAULT_NAME)?;
        this.inner.set_description(Self::DEFAULT_DESCRIPTION);
        this.configure()?;

        Ok(this)
    }

    fn create_completion_input(
        &self,
        input: &dyn InputInterface,
    ) -> anyhow::Result<CompletionInput> {
        let current_index = input.get_option("current")?;
        if !current_index.to_bool() || !shirabe_php_shim::ctype_digit(&current_index.to_string()) {
            anyhow::bail!(shirabe_php_shim::RuntimeException {
                message: "The \"--current\" option must be set and it must be an integer."
                    .to_string(),
                code: 0,
            });
        }

        let tokens: Vec<String> = match input.get_option("input")?.as_list() {
            Some(list) => list.iter().map(|v| v.to_string()).collect(),
            None => Vec::new(),
        };
        let mut completion_input = CompletionInput::from_tokens(
            tokens,
            current_index.to_string().parse::<i64>().unwrap_or(0),
        )?;

        // try { $completionInput->bind(...); } catch (ExceptionInterface $e) {}
        let application = self.get_application().unwrap();
        let definition = application.borrow_mut().get_definition();
        let _ = completion_input.bind(&definition.borrow());

        Ok(completion_input)
    }

    fn find_command(
        &self,
        completion_input: &CompletionInput,
        _output: &dyn OutputInterface,
    ) -> Option<Rc<RefCell<dyn Command>>> {
        // try { ... } catch (CommandNotFoundException $e) {}
        let input_name = completion_input.get_first_argument()?;

        let application = self.get_application().unwrap();
        // CommandNotFoundException is caught and swallowed by returning None.
        application.borrow_mut().find(&input_name).ok()
    }

    fn log(&self, messages: &str) {
        self.log_many(vec![messages.to_string()]);
    }

    fn log_many(&self, messages: Vec<String>) {
        if !self.is_debug.get() {
            return;
        }

        let command_name = shirabe_php_shim::basename(
            &shirabe_php_shim::PHP_SERVER
                .lock()
                .unwrap()
                .argv()
                .next()
                .unwrap_or_default()
                .to_string_lossy(),
        );
        shirabe_php_shim::file_put_contents3(
            &format!(
                "{}/sf_{}.log",
                shirabe_php_shim::sys_get_temp_dir(),
                command_name
            ),
            &(messages.join(shirabe_php_shim::PHP_EOL) + shirabe_php_shim::PHP_EOL),
            shirabe_php_shim::FILE_APPEND,
        );
    }
}

fn get_class_of_command(command: &Rc<RefCell<dyn Command>>) -> String {
    // LazyCommand is intentionally not ported.
    // TODO: get_class() takes a PhpMixed but the command is a `dyn Command`; reflecting the
    // concrete class name of a trait object requires a class-name hook on Command (Phase C).
    todo!()
}

/// $command->getDefinition()->getOptions()
fn get_definition_options(_command: &Rc<RefCell<dyn Command>>) -> Vec<InputOption> {
    // TODO: InputDefinition::get_options() returns `&IndexMap<String, Rc<InputOption>>` but
    // CompletionSuggestions::suggest_options() takes `Vec<InputOption>`; the option ownership
    // model must be reconciled (Phase C).
    todo!()
}

/// new $completionOutput();
fn instantiate_completion_output(_class: &PhpMixed) -> Box<dyn CompletionOutputInterface> {
    todo!()
}

impl Command for CompleteCommand {
    fn configure(&self) -> anyhow::Result<()> {
        let shells = self
            .completion_outputs
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join("\", \"");
        self.inner
            .add_option(
                "shell",
                PhpMixed::from("s".to_string()),
                Some(InputOption::VALUE_REQUIRED),
                &format!("The shell type (\"{}\")", shells),
                PhpMixed::Null,
            )?
            .add_option(
                "input",
                PhpMixed::from("i".to_string()),
                Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY),
                "An array of input tokens (e.g. COMP_WORDS or argv)",
                PhpMixed::Null,
            )?
            .add_option(
                "current",
                PhpMixed::from("c".to_string()),
                Some(InputOption::VALUE_REQUIRED),
                "The index of the \"input\" array that the cursor is in (e.g. COMP_CWORD)",
                PhpMixed::Null,
            )?
            .add_option(
                "symfony",
                PhpMixed::from("S".to_string()),
                Some(InputOption::VALUE_REQUIRED),
                "The version of the completion script",
                PhpMixed::Null,
            )?;

        Ok(())
    }

    fn initialize(
        &self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        let _ = (input, output);
        self.is_debug.set(shirabe_php_shim::filter_var_boolean(
            &shirabe_php_shim::getenv("SYMFONY_COMPLETION_DEBUG")
                .unwrap_or_default()
                .to_string_lossy(),
        ));

        Ok(())
    }

    fn execute(
        &self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        // try { ... } catch (\Throwable $e) { ...; if ($output->isDebug()) { throw $e; } return 2; }
        let result: anyhow::Result<i64> = (|| {
            // uncomment when a bugfix or BC break has been introduced in the shell completion scripts
            // $version = $input->getOption('symfony');
            // if ($version && version_compare($version, 'x.y', '>=')) {
            //    $message = sprintf('Completion script version is not supported ("%s" given, ">=x.y" required).', $version);
            //    $this->log($message);
            //    $output->writeln($message.' Install the Symfony completion script again by using the "completion" command.');
            //    return 126;
            // }

            let shell = input.borrow().get_option("shell")?;
            if !shell.to_bool() {
                anyhow::bail!(shirabe_php_shim::RuntimeException {
                    message: "The \"--shell\" option must be set.".to_string(),
                    code: 0,
                });
            }

            let completion_output = self
                .completion_outputs
                .get(&shell.to_string())
                .cloned()
                .unwrap_or(PhpMixed::Bool(false));
            if !completion_output.to_bool() {
                anyhow::bail!(shirabe_php_shim::RuntimeException {
                    message: format!(
                        "Shell completion is not supported for your shell: \"{}\" (supported: \"{}\").",
                        shell,
                        self.completion_outputs
                            .keys()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join("\", \"")
                    ),
                    code: 0,
                });
            }

            let mut completion_input = self.create_completion_input(&*input.borrow())?;
            let mut suggestions = CompletionSuggestions::new();

            self.log_many(vec![
                String::new(),
                format!(
                    "<comment>{}</>",
                    shirabe_php_shim::date("Y-m-d H:i:s", None)
                ),
                "<info>Input:</> <comment>(\"|\" indicates the cursor position)</>".to_string(),
                format!("  {}", completion_input.to_string()),
                "<info>Command:</>".to_string(),
                format!(
                    "  {}",
                    shirabe_php_shim::PHP_SERVER
                        .lock()
                        .unwrap()
                        .argv()
                        .map(|a| a.to_string_lossy().into_owned())
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
                "<info>Messages:</>".to_string(),
            ]);

            let command = self.find_command(&completion_input, &*output.borrow());
            match command {
                None => {
                    self.log("  No command found, completing using the Application class.");

                    let application = self.get_application().unwrap();
                    application
                        .borrow_mut()
                        .complete(&completion_input, &mut suggestions)?;
                }
                Some(command)
                    if completion_input.must_suggest_argument_values_for("command")
                        && command.borrow().get_name().as_deref()
                            != Some(&completion_input.get_completion_value())
                        && !command
                            .borrow()
                            .get_aliases()
                            .iter()
                            .any(|a| a == &completion_input.get_completion_value()) =>
                {
                    self.log("  No command found, completing using the Application class.");

                    // expand shortcut names ("cache:cl<TAB>") into their full name ("cache:clear")
                    let mut values = vec![command.borrow().get_name()];
                    values.extend(command.borrow().get_aliases().into_iter().map(Some));
                    suggestions.suggest_values(
                        values
                            .into_iter()
                            .flatten()
                            .filter(|v| !v.is_empty())
                            .map(StringOrSuggestion::String)
                            .collect(),
                    );
                }
                Some(command) => {
                    command.borrow().merge_application_definition(false);
                    completion_input.bind(&*command.borrow().get_definition())?;

                    if CompletionInput::TYPE_OPTION_NAME == completion_input.get_completion_type() {
                        self.log(&format!(
                            "  Completing option names for the <comment>{}</> command.",
                            get_class_of_command(&command)
                        ));

                        suggestions.suggest_options(get_definition_options(&command));
                    } else {
                        self.log_many(vec![
                            format!(
                                "  Completing using the <comment>{}</> class.",
                                get_class_of_command(&command)
                            ),
                            format!(
                                "  Completing <comment>{}</> for <comment>{}</>",
                                completion_input.get_completion_type(),
                                completion_input.get_completion_name().unwrap_or_default()
                            ),
                        ]);
                        let compval = completion_input.get_completion_value();
                        if !compval.is_empty() {
                            self.log(&format!("  Current value: <comment>{}</>", compval));
                        }

                        command
                            .borrow()
                            .complete(&completion_input, &mut suggestions);
                    }
                }
            }

            // $completionOutput = new $completionOutput();
            let completion_output: Box<dyn CompletionOutputInterface> =
                instantiate_completion_output(&completion_output);

            self.log("<info>Suggestions:</>");
            let option_suggestions = suggestions.get_option_suggestions();
            if !option_suggestions.is_empty() {
                self.log(&format!(
                    "  --{}",
                    option_suggestions
                        .iter()
                        .map(|o| o.get_name())
                        .collect::<Vec<_>>()
                        .join(" --")
                ));
            } else {
                let value_suggestions: Vec<String> = suggestions
                    .get_value_suggestions()
                    .iter()
                    .map(|s| s.get_value())
                    .collect();
                if !value_suggestions.is_empty() {
                    self.log(&format!("  {}", value_suggestions.join(" ")));
                } else {
                    self.log("  <comment>No suggestions were provided</>");
                }
            }

            completion_output.write(&suggestions, &*output.borrow_mut());

            Ok(0)
        })();

        match result {
            Ok(code) => Ok(code),
            Err(e) => {
                self.log_many(vec!["<error>Error!</error>".to_string(), format!("{}", e)]);

                if output.borrow().is_debug() {
                    return Err(e);
                }

                Ok(2)
            }
        }
    }

    crate::delegate_command_trait_impls_to_inner!(inner);
}
