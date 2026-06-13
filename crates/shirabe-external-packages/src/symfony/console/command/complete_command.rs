use indexmap::IndexMap;
use shirabe_php_shim::AsAny;
use shirabe_php_shim::PhpMixed;
use std::cell::RefCell;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use crate::symfony::console::command::command::{BaseCommand, Command};
use crate::symfony::console::command::lazy_command::LazyCommand;
use crate::symfony::console::completion::completion_input::CompletionInput;
use crate::symfony::console::completion::completion_suggestions::{
    CompletionSuggestions, StringOrSuggestion,
};
use crate::symfony::console::completion::output::completion_output_interface::CompletionOutputInterface;
use crate::symfony::console::input::input_interface::InputInterface;
use crate::symfony::console::input::input_option::InputOption;
use crate::symfony::console::output::output_interface::{self, OutputInterface};

/// Responsible for providing the values to the shell completion.
#[derive(Debug)]
pub struct CompleteCommand {
    inner: BaseCommand,
    completion_outputs: IndexMap<String, PhpMixed>,
    is_debug: bool,
}

impl Deref for CompleteCommand {
    type Target = BaseCommand;

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
            inner: BaseCommand::__construct(None)?,
            completion_outputs,
            is_debug: false,
        };

        Ok(this)
    }

    fn configure(&mut self) -> anyhow::Result<()> {
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

    fn initialize(&mut self, _input: &dyn InputInterface, _output: &dyn OutputInterface) {
        self.is_debug = shirabe_php_shim::filter_var(
            &shirabe_php_shim::getenv("SYMFONY_COMPLETION_DEBUG").unwrap_or_default(),
            shirabe_php_shim::FILTER_VALIDATE_BOOLEAN,
        );
    }

    fn execute(
        &mut self,
        input: &mut dyn InputInterface,
        output: &mut dyn OutputInterface,
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

            let shell = input.get_option("shell")?;
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

            let mut completion_input = self.create_completion_input(input)?;
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
                format!("  {}", shirabe_php_shim::server_argv().join(" ")),
                "<info>Messages:</>".to_string(),
            ]);

            let command = self.find_command(&completion_input, output);
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
                    command.borrow_mut().merge_application_definition(false);
                    completion_input.bind(command.borrow().get_definition())?;

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

            completion_output.write(&suggestions, output);

            Ok(0)
        })();

        match result {
            Ok(code) => Ok(code),
            Err(e) => {
                self.log_many(vec!["<error>Error!</error>".to_string(), format!("{}", e)]);

                if output.is_debug() {
                    return Err(e);
                }

                Ok(2)
            }
        }
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
        if !self.is_debug {
            return;
        }

        let command_name = shirabe_php_shim::basename(&shirabe_php_shim::server_argv()[0]);
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

/// \get_class($command instanceof LazyCommand ? $command->getCommand() : $command)
fn get_class_of_command(command: &Rc<RefCell<dyn Command>>) -> String {
    let borrowed = command.borrow();
    let _is_lazy = (*borrowed).as_any().downcast_ref::<LazyCommand>().is_some();
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
    fn configure(&mut self) {
        let _ = CompleteCommand::configure(self);
    }

    fn run(
        &mut self,
        input: &mut dyn InputInterface,
        output: &mut dyn OutputInterface,
    ) -> anyhow::Result<i64> {
        self.inner.run(input, output)
    }

    fn is_enabled(&self) -> bool {
        self.inner.is_enabled()
    }

    fn set_application(
        &mut self,
        application: Option<Rc<RefCell<dyn crate::symfony::console::application::Application>>>,
    ) {
        self.inner.set_application(application);
    }

    fn get_application(
        &self,
    ) -> Option<Rc<RefCell<dyn crate::symfony::console::application::Application>>> {
        self.inner.get_application()
    }

    fn set_helper_set(
        &mut self,
        helper_set: Rc<RefCell<crate::symfony::console::helper::helper_set::HelperSet>>,
    ) {
        self.inner.set_helper_set(helper_set);
    }

    fn get_helper_set(
        &self,
    ) -> Option<Rc<RefCell<crate::symfony::console::helper::helper_set::HelperSet>>> {
        self.inner.get_helper_set()
    }

    fn merge_application_definition(&mut self, merge_args: bool) {
        self.inner.merge_application_definition(merge_args);
    }

    fn get_definition(&self) -> &crate::symfony::console::input::input_definition::InputDefinition {
        self.inner.get_definition()
    }

    fn get_native_definition(
        &self,
    ) -> &crate::symfony::console::input::input_definition::InputDefinition {
        self.inner.get_native_definition()
    }

    fn set_name(&mut self, name: &str) -> anyhow::Result<()> {
        self.inner.set_name(name)?;
        Ok(())
    }

    fn get_name(&self) -> Option<String> {
        self.inner.get_name()
    }

    fn set_hidden(&mut self, hidden: bool) {
        self.inner.set_hidden(hidden);
    }

    fn is_hidden(&self) -> bool {
        self.inner.is_hidden()
    }

    fn set_description(&mut self, description: &str) {
        self.inner.set_description(description);
    }

    fn get_description(&self) -> String {
        self.inner.get_description()
    }

    fn set_help(&mut self, help: &str) {
        self.inner.set_help(help);
    }

    fn get_help(&self) -> String {
        self.inner.get_help()
    }

    fn get_processed_help(&self) -> String {
        self.inner.get_processed_help()
    }

    fn set_aliases(&mut self, aliases: Vec<String>) -> anyhow::Result<()> {
        self.inner.set_aliases(aliases)?;
        Ok(())
    }

    fn get_aliases(&self) -> Vec<String> {
        self.inner.get_aliases()
    }

    fn get_synopsis(&mut self, short: bool) -> String {
        self.inner.get_synopsis(short)
    }

    fn get_usages(&self) -> Vec<String> {
        self.inner.get_usages()
    }

    fn get_helper(
        &self,
        name: &str,
    ) -> anyhow::Result<
        Result<PhpMixed, crate::symfony::console::exception::logic_exception::LogicException>,
    > {
        self.inner.get_helper(name)
    }

    fn ignore_validation_errors(&mut self) {
        self.inner.ignore_validation_errors();
    }
}
