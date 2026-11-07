//! ref: composer/vendor/symfony/console/Command/CompleteCommand.php sha1:78febd02a74ab09d930d8a6c220c13a8b69641d2

use crate::command::{Command, CommandData};
use crate::completion::output::{BashCompletionOutput, CompletionOutputInterface};
use crate::completion::{CompletionInput, CompletionSuggestions, StringOrSuggestion};
use crate::delegate_command_trait_impls_to_inner;
use crate::input::{InputInterface, InputInterfaceByName, InputOption, InputValue};
use crate::output::OutputInterface;
use bstr::{B, BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{
    FILE_APPEND, PHP_EOL, PHP_SERVER, PhpMixed, RuntimeException, Stringable, basename, bconcat,
    ctype_digit, date_utc, error_message, file_put_contents_with_flags, filter_var_boolean, getenv,
    impl_php_class, implode, intval, path_from_bytes, php_str, sys_get_temp_dir,
};
use std::ops::{Deref, DerefMut};

/// Responsible for providing the values to the shell completion.
#[derive(Debug)]
pub struct CompleteCommand {
    inner: CommandData,
    completion_outputs: IndexMap<BString, PhpMixed>,
    is_debug: std::cell::Cell<bool>,
}

impl_php_class!(
    CompleteCommand,
    r"Symfony\Component\Console\Command\CompleteCommand"
);

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
    pub const DEFAULT_NAME: &'static BStr = php_str(b"|_complete");
    pub const DEFAULT_DESCRIPTION: &'static BStr =
        php_str(b"Internal command to provide shell completion suggestions");

    pub fn new(completion_outputs: IndexMap<BString, PhpMixed>) -> anyhow::Result<Self> {
        // must be set before the parent constructor, as the property value is used in configure()
        let mut completion_outputs = completion_outputs;
        // $completionOutputs + ['bash' => BashCompletionOutput::class]
        completion_outputs.entry("bash".into()).or_insert_with(|| {
            PhpMixed::from(BString::from(
                "Symfony\\Component\\Console\\Completion\\Output\\BashCompletionOutput",
            ))
        });

        let this = Self {
            inner: CommandData::new(None),
            completion_outputs,
            is_debug: std::cell::Cell::new(false),
        };
        // PHP: static $defaultName = '|_complete' / $defaultDescription, applied by the parent
        // constructor before configure().
        this.inner.apply_default_name(Self::DEFAULT_NAME)?;
        this.inner
            .set_description(bstr::BStr::new(Self::DEFAULT_DESCRIPTION));
        this.configure()?;

        Ok(this)
    }

    fn create_completion_input(
        &self,
        input: &dyn InputInterface,
    ) -> anyhow::Result<CompletionInput> {
        let current_index = input.get_option("current")?;
        if !current_index.to_bool() || !ctype_digit(current_index.as_string().unwrap_or_default()) {
            anyhow::bail!(RuntimeException::new(
                "The \"--current\" option must be set and it must be an integer."
            ));
        }

        let tokens: Vec<bstr::BString> = input
            .get_option("input")?
            .as_array()
            .map(<[bstr::BString]>::to_vec)
            .unwrap_or_default();
        let mut completion_input =
            CompletionInput::from_tokens(tokens, intval(&current_index.to_php_mixed()))?;

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
    ) -> Option<std::rc::Rc<std::cell::RefCell<dyn Command>>> {
        // try { ... } catch (CommandNotFoundException $e) {}
        let input_name = completion_input.get_first_argument()?;

        let application = self.get_application().unwrap();
        // CommandNotFoundException is caught and swallowed by returning None.
        application.borrow_mut().find(input_name.as_ref()).ok()
    }

    fn log(&self, messages: &BStr) {
        self.log_many(vec![BString::from(messages)]);
    }

    fn log_many(&self, messages: Vec<BString>) {
        if !self.is_debug.get() {
            return;
        }

        let command_name = basename(
            PHP_SERVER
                .lock()
                .unwrap()
                .argv()
                .next()
                .unwrap_or_default()
                .as_encoded_bytes(),
        );
        let tmp_dir = sys_get_temp_dir();
        file_put_contents_with_flags(
            path_from_bytes(&bconcat!(tmp_dir, "/sf_", command_name, ".log")),
            &bconcat!(implode(PHP_EOL, &messages), PHP_EOL),
            FILE_APPEND,
        );
    }
}

fn get_class_of_command(command: &std::rc::Rc<std::cell::RefCell<dyn Command>>) -> BString {
    // LazyCommand is intentionally not ported.
    command.borrow().php_class_name().to_owned()
}

fn get_definition_options(
    command: &std::rc::Rc<std::cell::RefCell<dyn Command>>,
) -> Vec<std::rc::Rc<InputOption>> {
    command
        .borrow()
        .get_definition()
        .get_options()
        .values()
        .cloned()
        .collect()
}

/// new $completionOutput();
fn instantiate_completion_output(class: &PhpMixed) -> Box<dyn CompletionOutputInterface> {
    let class = class
        .as_string()
        .expect("completion outputs are registered as class names");
    match class.as_bytes() {
        b"Symfony\\Component\\Console\\Completion\\Output\\BashCompletionOutput" => {
            Box::new(BashCompletionOutput)
        }
        // completion_outputs only ever registers the bash output (Composer registers no extra
        // ones), so any other FQCN is a programming error.
        other => panic!("unknown completion output class: {:?}", other.as_bstr()),
    }
}

#[async_trait::async_trait(?Send)]
impl Command for CompleteCommand {
    fn configure(&self) -> anyhow::Result<()> {
        let shells = implode(
            "\", \"",
            &self
                .completion_outputs
                .keys()
                .map(|shell| shell.to_owned())
                .collect::<Vec<_>>(),
        );
        self.inner
            .add_option(
                "shell",
                Some("s".into()),
                Some(InputOption::VALUE_REQUIRED),
                bconcat!("The shell type (\"", shells, "\")"),
                InputValue::Null,
            )?
            .add_option(
                "input",
                Some("i".into()),
                Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY),
                "An array of input tokens (e.g. COMP_WORDS or argv)",
                InputValue::Null,
            )?
            .add_option(
                "current",
                Some("c".into()),
                Some(InputOption::VALUE_REQUIRED),
                "The index of the \"input\" array that the cursor is in (e.g. COMP_CWORD)",
                InputValue::Null,
            )?
            .add_option(
                "symfony",
                Some("S".into()),
                Some(InputOption::VALUE_REQUIRED),
                "The version of the completion script",
                InputValue::Null,
            )?;

        Ok(())
    }

    fn initialize(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        let _ = (input, output);
        self.is_debug.set(filter_var_boolean(
            getenv("SYMFONY_COMPLETION_DEBUG")
                .unwrap_or_default()
                .as_encoded_bytes(),
        ));

        Ok(())
    }

    async fn execute(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
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
                anyhow::bail!(RuntimeException::new("The \"--shell\" option must be set."));
            }

            let completion_output = self
                .completion_outputs
                .get(B(shell.as_string().unwrap_or_default()))
                .cloned()
                .unwrap_or(PhpMixed::Bool(false));
            if !completion_output.to_bool() {
                anyhow::bail!(RuntimeException::new(bconcat!(
                    "Shell completion is not supported for your shell: \"",
                    shell.as_string().unwrap_or_default(),
                    "\" (supported: \"",
                    implode(
                        "\", \"",
                        &self
                            .completion_outputs
                            .keys()
                            .map(|shell| shell.to_owned())
                            .collect::<Vec<_>>()
                    ),
                    "\")."
                )));
            }

            let mut completion_input = self.create_completion_input(&*input.borrow())?;
            let mut suggestions = CompletionSuggestions::new();

            self.log_many(vec![
                BString::default(),
                bconcat!("<comment>", date_utc("Y-m-d H:i:s", None), "</>"),
                BString::from("<info>Input:</> <comment>(\"|\" indicates the cursor position)</>"),
                bconcat!("  ", completion_input.to_php_string()),
                BString::from("<info>Command:</>"),
                bconcat!(
                    "  ",
                    implode(
                        " ",
                        &PHP_SERVER
                            .lock()
                            .unwrap()
                            .argv()
                            .map(|a| a.as_encoded_bytes().as_bstr().to_owned())
                            .collect::<Vec<_>>()
                    )
                ),
                BString::from("<info>Messages:</>"),
            ]);

            let command = self.find_command(&completion_input, &*output.borrow());
            match command {
                None => {
                    self.log(BStr::new(
                        "  No command found, completing using the Application class.",
                    ));

                    let application = self.get_application().unwrap();
                    application
                        .borrow_mut()
                        .complete(&completion_input, &mut suggestions)?;
                }
                Some(command)
                    if completion_input.must_suggest_argument_values_for("command")
                        && command.borrow().get_name()
                            != Some(completion_input.get_completion_value())
                        && !command
                            .borrow()
                            .get_aliases()
                            .iter()
                            .any(|a| completion_input.get_completion_value() == *a) =>
                {
                    self.log(BStr::new(
                        "  No command found, completing using the Application class.",
                    ));

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
                    // PHP: $command->mergeApplicationDefinition() — $mergeArgs defaults to true.
                    command.borrow().merge_application_definition(true);
                    completion_input.bind(&command.borrow().get_definition())?;

                    if CompletionInput::TYPE_OPTION_NAME == completion_input.get_completion_type() {
                        self.log(
                            bconcat!(
                                "  Completing option names for the <comment>",
                                get_class_of_command(&command),
                                "</> command."
                            )
                            .as_bstr(),
                        );

                        suggestions.suggest_options(get_definition_options(&command));
                    } else {
                        self.log_many(vec![
                            bconcat!(
                                "  Completing using the <comment>",
                                get_class_of_command(&command),
                                "</> class."
                            ),
                            bconcat!(
                                "  Completing <comment>",
                                completion_input.get_completion_type(),
                                "</> for <comment>",
                                completion_input.get_completion_name().unwrap_or_default(),
                                "</>"
                            ),
                        ]);
                        let compval = completion_input.get_completion_value();
                        if !compval.is_empty() {
                            self.log(
                                bconcat!("  Current value: <comment>", compval, "</>").as_bstr(),
                            );
                        }

                        command
                            .borrow()
                            .complete(&completion_input, &mut suggestions)?;
                    }
                }
            }

            // $completionOutput = new $completionOutput();
            let completion_output: Box<dyn CompletionOutputInterface> =
                instantiate_completion_output(&completion_output);

            self.log(BStr::new("<info>Suggestions:</>"));
            let option_suggestions = suggestions.get_option_suggestions();
            if !option_suggestions.is_empty() {
                self.log(
                    bconcat!(
                        "  --",
                        implode(
                            " --",
                            &option_suggestions
                                .iter()
                                .map(|o| o.get_name().to_owned())
                                .collect::<Vec<_>>()
                        )
                    )
                    .as_bstr(),
                );
            } else {
                let value_suggestions: Vec<BString> = suggestions
                    .get_value_suggestions()
                    .iter()
                    .map(|s| s.get_value().to_owned())
                    .collect();
                if !value_suggestions.is_empty() {
                    self.log(bconcat!("  ", implode(" ", &value_suggestions)).as_bstr());
                } else {
                    self.log(BStr::new("  <comment>No suggestions were provided</>"));
                }
            }

            completion_output.write(&suggestions, &*output.borrow_mut());

            Ok(0)
        })();

        match result {
            Ok(code) => Ok(code),
            Err(e) => {
                self.log_many(vec![
                    BString::from("<error>Error!</error>"),
                    error_message(&e),
                ]);

                if output.borrow().is_debug() {
                    return Err(e);
                }

                Ok(2)
            }
        }
    }

    delegate_command_trait_impls_to_inner!(inner);
}
