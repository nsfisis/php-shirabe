use crate::symfony::console::command::command::Command;
use crate::symfony::console::command::complete_command::CompleteCommand;
use crate::symfony::console::command::dump_completion_command::DumpCompletionCommand;
use crate::symfony::console::command::help_command::HelpCommand;
use crate::symfony::console::command::lazy_command::LazyCommand;
use crate::symfony::console::command::list_command::ListCommand;
use crate::symfony::console::command::signalable_command_interface::SignalableCommandInterface;
use crate::symfony::console::command_loader::command_loader_interface::CommandLoaderInterface;
use crate::symfony::console::completion::completion_input::CompletionInput;
use crate::symfony::console::completion::completion_suggestions::CompletionSuggestions;
use crate::symfony::console::console_events::ConsoleEvents;
use crate::symfony::console::event::console_command_event::ConsoleCommandEvent;
use crate::symfony::console::event::console_error_event::ConsoleErrorEvent;
use crate::symfony::console::event::console_signal_event::ConsoleSignalEvent;
use crate::symfony::console::event::console_terminate_event::ConsoleTerminateEvent;
use crate::symfony::console::exception::command_not_found_exception::CommandNotFoundException;
use crate::symfony::console::exception::exception_interface::ExceptionInterface;
use crate::symfony::console::exception::logic_exception::LogicException;
use crate::symfony::console::exception::namespace_not_found_exception::NamespaceNotFoundException;
use crate::symfony::console::exception::runtime_exception::RuntimeException;
use crate::symfony::console::formatter::output_formatter::OutputFormatter;
use crate::symfony::console::helper::debug_formatter_helper::DebugFormatterHelper;
use crate::symfony::console::helper::formatter_helper::{FormatBlockMessages, FormatterHelper};
use crate::symfony::console::helper::helper::Helper;
use crate::symfony::console::helper::helper_set::HelperSet;
use crate::symfony::console::helper::process_helper::ProcessHelper;
use crate::symfony::console::helper::question_helper::QuestionHelper;
use crate::symfony::console::input::argv_input::ArgvInput;
use crate::symfony::console::input::array_input::ArrayInput;
use crate::symfony::console::input::input_argument::InputArgument;
use crate::symfony::console::input::input_aware_interface::InputAwareInterface;
use crate::symfony::console::input::input_definition::InputDefinition;
use crate::symfony::console::input::input_interface::InputInterface;
use crate::symfony::console::input::input_option::InputOption;
use crate::symfony::console::output::console_output::ConsoleOutput;
use crate::symfony::console::output::console_output_interface::ConsoleOutputInterface;
use crate::symfony::console::output::output_interface::{self, OutputInterface};
use crate::symfony::console::signal_registry::signal_registry::SignalRegistry;
use crate::symfony::console::style::style_interface::StyleInterface;
use crate::symfony::console::style::symfony_style::SymfonyStyle;
use crate::symfony::console::terminal::Terminal;
use crate::symfony::contracts::event_dispatcher::event_dispatcher_interface::EventDispatcherInterface;
use crate::symfony::contracts::service::reset_interface::ResetInterface;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;
use std::cell::RefCell;
use std::rc::Rc;

/// An Application is the container for a collection of commands.
///
/// It is the main entry point of a Console application.
///
/// This class is optimized for a standard CLI environment.
#[derive(Debug)]
pub struct Application {
    commands: IndexMap<String, Rc<RefCell<dyn Command>>>,
    want_helps: bool,
    running_command: Option<Rc<RefCell<dyn Command>>>,
    name: String,
    version: String,
    command_loader: Option<Box<dyn CommandLoaderInterface>>,
    catch_exceptions: bool,
    auto_exit: bool,
    definition: Option<Rc<RefCell<InputDefinition>>>,
    helper_set: Option<Rc<RefCell<HelperSet>>>,
    dispatcher: Option<Rc<RefCell<dyn EventDispatcherInterface>>>,
    terminal: Terminal,
    default_command: String,
    single_command: bool,
    initialized: bool,
    signal_registry: Option<SignalRegistry>,
    signals_to_dispatch_event: Vec<i64>,
}

impl Application {
    pub fn __construct(name: &str, version: &str) -> Self {
        let mut this = Application {
            commands: IndexMap::new(),
            want_helps: false,
            running_command: None,
            name: name.to_string(),
            version: version.to_string(),
            command_loader: None,
            catch_exceptions: true,
            auto_exit: true,
            definition: None,
            helper_set: None,
            dispatcher: None,
            terminal: Terminal::new(),
            default_command: "list".to_string(),
            single_command: false,
            initialized: false,
            signal_registry: None,
            signals_to_dispatch_event: Vec::new(),
        };
        if shirabe_php_shim::defined("SIGINT") && SignalRegistry::is_supported() {
            this.signal_registry = Some(SignalRegistry::new());
            this.signals_to_dispatch_event = vec![
                shirabe_php_shim::SIGINT,
                shirabe_php_shim::SIGTERM,
                shirabe_php_shim::SIGUSR1,
                shirabe_php_shim::SIGUSR2,
            ];
        }
        this
    }

    /// @final
    pub fn set_dispatcher(&mut self, dispatcher: Rc<RefCell<dyn EventDispatcherInterface>>) {
        // TODO(plugin): the event dispatcher drives ConsoleEvents listeners (plugins).
        self.dispatcher = Some(dispatcher);
    }

    pub fn set_command_loader(&mut self, command_loader: Box<dyn CommandLoaderInterface>) {
        self.command_loader = Some(command_loader);
    }

    pub fn get_signal_registry(&self) -> anyhow::Result<&SignalRegistry> {
        match &self.signal_registry {
            None => Err(RuntimeException(shirabe_php_shim::RuntimeException {
                message: "Signals are not supported. Make sure that the `pcntl` extension is installed and that \"pcntl_*\" functions are not disabled by your php.ini's \"disable_functions\" directive.".to_string(),
                code: 0,
            })
            .into()),
            Some(signal_registry) => Ok(signal_registry),
        }
    }

    pub fn set_signals_to_dispatch_event(&mut self, signals_to_dispatch_event: Vec<i64>) {
        self.signals_to_dispatch_event = signals_to_dispatch_event;
    }

    /// Runs the current application.
    ///
    /// Returns 0 if everything went fine, or an error code.
    ///
    /// Throws \Exception when running fails. Bypass this when set_catch_exceptions().
    pub fn run(
        &mut self,
        input: Option<Rc<RefCell<dyn InputInterface>>>,
        output: Option<Rc<RefCell<dyn OutputInterface>>>,
    ) -> anyhow::Result<i64> {
        if shirabe_php_shim::function_exists("putenv") {
            shirabe_php_shim::putenv(&format!("LINES={}", self.terminal.get_height()));
            shirabe_php_shim::putenv(&format!("COLUMNS={}", self.terminal.get_width()));
        }

        let input: Rc<RefCell<dyn InputInterface>> = match input {
            None => Rc::new(RefCell::new(ArgvInput::new(None, None)?)),
            Some(input) => input,
        };

        let output: Rc<RefCell<dyn OutputInterface>> = match output {
            None => Rc::new(RefCell::new(ConsoleOutput::new(None, None, None)?)),
            Some(output) => output,
        };

        // TODO: PHP installs a temporary `set_exception_handler($renderException)` and cooperates
        // with Symfony's ErrorHandler to keep/restore it. PHP's process-global exception handler
        // stack has no Rust equivalent; the rendering itself is invoked directly in the catch
        // branch below. Review needed for the handler save/restore dance.
        let render_exception =
            |this: &Application, e: &anyhow::Error, output: &Rc<RefCell<dyn OutputInterface>>| {
                // if ($output instanceof ConsoleOutputInterface) render to its error output
                // TODO(review): downcasting a `dyn OutputInterface` to `ConsoleOutputInterface`
                // is not directly expressible; the ConsoleOutputInterface branch needs design.
                this.render_throwable(e, output.clone());
            };

        let result = (|| -> anyhow::Result<i64> {
            self.configure_io(&input, &output)?;

            let exit_code = self.do_run(input.clone(), output.clone())?;

            Ok(exit_code)
        })();

        let mut exit_code = match result {
            Ok(exit_code) => exit_code,
            Err(e) => {
                if !self.catch_exceptions {
                    return Err(e);
                }

                render_exception(self, &e, &output);

                // $exitCode = $e->getCode();
                // is_numeric($exitCode) ? max(1, (int) $exitCode) : 1
                // TODO(review): anyhow::Error has no PHP-style getCode(); the exit code derived
                // from the exception's `code` field needs the downcast strategy decided.
                let exit_code = shirabe_php_shim::php_exception_get_code(&e);
                if shirabe_php_shim::is_numeric_string(&exit_code.to_string()) {
                    let exit_code = exit_code;
                    if exit_code <= 0 { 1 } else { exit_code }
                } else {
                    1
                }
            }
        };

        // finally: handler restore. See TODO above; no-op here.

        if self.auto_exit {
            if exit_code > 255 {
                exit_code = 255;
            }

            shirabe_php_shim::exit(exit_code);
        }

        Ok(exit_code)
    }

    /// Runs the current application.
    ///
    /// Returns 0 if everything went fine, or an error code.
    pub fn do_run(
        &mut self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        if input.borrow().has_parameter_option(
            PhpMixed::from(vec![
                PhpMixed::from("--version".to_string()),
                PhpMixed::from("-V".to_string()),
            ]),
            true,
        ) {
            output
                .borrow()
                .writeln(&[self.get_long_version()], output_interface::OUTPUT_NORMAL);

            return Ok(0);
        }

        // Makes ArgvInput::getFirstArgument() able to distinguish an option from an argument.
        match input.borrow_mut().bind(&self.get_definition().borrow()) {
            Ok(()) => {}
            Err(e) => {
                // Errors must be ignored, full binding/validation happens later when the command is known.
                if !is_exception_interface(&e) {
                    return Err(e);
                }
            }
        }

        let mut input = input;
        let mut name = self.get_command_name(&*input.borrow());
        if input.borrow().has_parameter_option(
            PhpMixed::from(vec![
                PhpMixed::from("--help".to_string()),
                PhpMixed::from("-h".to_string()),
            ]),
            true,
        ) {
            if name.is_none() {
                name = Some("help".to_string());
                input = Rc::new(RefCell::new(ArrayInput::new(
                    vec![(
                        PhpMixed::from("command_name".to_string()),
                        PhpMixed::from(self.default_command.clone()),
                    )],
                    None,
                )?));
            } else {
                self.want_helps = true;
            }
        }

        let name = match name {
            Some(name) => name,
            None => {
                let name = self.default_command.clone();
                let definition = self.get_definition();
                let command_description = definition
                    .borrow()
                    .get_argument(&PhpMixed::from("command".to_string()))?
                    .get_description()
                    .to_string();
                let _new_command_argument = InputArgument::new(
                    "command".to_string(),
                    Some(InputArgument::OPTIONAL),
                    command_description,
                    PhpMixed::from(name.clone()),
                )?;
                // $definition->setArguments(array_merge($definition->getArguments(),
                //     ['command' => new InputArgument('command', InputArgument::OPTIONAL, ...)]))
                // TODO(review): get_arguments() yields Rc<InputArgument> (shared, non-Clone) while
                // set_arguments() consumes owned InputArgument values. Re-building the merged
                // argument list requires an InputArgument clone/ownership strategy not yet present.
                definition.borrow_mut().set_arguments(todo!(
                    "merge existing arguments with the new 'command' argument"
                ))?;

                name
            }
        };

        let command: Rc<RefCell<dyn Command>>;
        let find_result = (|| -> anyhow::Result<Rc<RefCell<dyn Command>>> {
            self.running_command = None;
            // the command name MUST be the first element of the input
            self.find(&name)
        })();

        match find_result {
            Ok(c) => {
                command = c;
            }
            Err(e) => {
                // if (!($e instanceof CommandNotFoundException && !$e instanceof NamespaceNotFoundException)
                //     || 1 !== count($alternatives = $e->getAlternatives()) || !$input->isInteractive())
                let alternatives: Option<Vec<String>> = downcast_command_not_found(&e)
                    .filter(|_| !is_namespace_not_found(&e))
                    .map(|cnf| cnf.get_alternatives().clone());

                let single_alternative = match &alternatives {
                    Some(alts) if alts.len() == 1 => Some(alts[0].clone()),
                    _ => None,
                };

                if single_alternative.is_none() || !input.borrow().is_interactive() {
                    let mut e = e;
                    if self.dispatcher.is_some() {
                        // TODO(plugin): dispatch ConsoleErrorEvent so listeners can handle/replace the error.
                        let _event = ConsoleErrorEvent::new(
                            todo!("wrap input as Box<dyn InputInterface> for the event"),
                            todo!("wrap output as Box<dyn OutputInterface> for the event"),
                            todo!("wrap anyhow::Error as Box<dyn Error> for the event"),
                            None,
                        );
                        let event: ConsoleErrorEvent = _event;
                        self.dispatcher
                            .as_ref()
                            .unwrap()
                            .borrow_mut()
                            .dispatch(todo!("event object"), ConsoleEvents::ERROR);

                        if event.get_exit_code() == 0 {
                            return Ok(0);
                        }

                        e = todo!("event.get_error() converted back to anyhow::Error");
                    }

                    return Err(e);
                }

                let alternative = single_alternative.unwrap();

                let mut style = SymfonyStyle::new(input.clone(), output.clone());
                output
                    .borrow()
                    .writeln(&["".to_string()], output_interface::OUTPUT_NORMAL);
                let formatted_block = FormatterHelper::default().format_block(
                    FormatBlockMessages::String(shirabe_php_shim::sprintf(
                        "Command \"%s\" is not defined.",
                        &[PhpMixed::from(name.clone())],
                    )),
                    "error",
                    true,
                );
                output
                    .borrow()
                    .writeln(&[formatted_block], output_interface::OUTPUT_NORMAL);
                if !style.confirm(
                    &shirabe_php_shim::sprintf(
                        "Do you want to run \"%s\" instead? ",
                        &[PhpMixed::from(alternative.clone())],
                    ),
                    false,
                ) {
                    if self.dispatcher.is_some() {
                        // TODO(plugin): dispatch ConsoleErrorEvent for the declined-alternative case.
                        let event = ConsoleErrorEvent::new(
                            todo!("wrap input as Box<dyn InputInterface>"),
                            todo!("wrap output as Box<dyn OutputInterface>"),
                            todo!("wrap error as Box<dyn Error>"),
                            None,
                        );
                        self.dispatcher
                            .as_ref()
                            .unwrap()
                            .borrow_mut()
                            .dispatch(todo!("event object"), ConsoleEvents::ERROR);

                        return Ok(event.get_exit_code());
                    }

                    return Ok(1);
                }

                command = self.find(&alternative)?;
            }
        }

        // if ($command instanceof LazyCommand) $command = $command->getCommand();
        // TODO(review): LazyCommand is a distinct type from Command here; PHP unwraps the real
        // command. The `commands` map stores Rc<RefCell<dyn Command>>, so the LazyCommand-unwrap path
        // needs a design decision about how lazy commands are represented.
        let _ = std::marker::PhantomData::<LazyCommand>;

        self.running_command = Some(command.clone());
        let exit_code = self.do_run_command(command.clone(), input.clone(), output.clone())?;
        self.running_command = None;

        Ok(exit_code)
    }

    pub fn reset(&mut self) {}

    pub fn set_helper_set(&mut self, helper_set: Rc<RefCell<HelperSet>>) {
        self.helper_set = Some(helper_set);
    }

    /// Get the helper set associated with the command.
    pub fn get_helper_set(&mut self) -> Rc<RefCell<HelperSet>> {
        if self.helper_set.is_none() {
            self.helper_set = Some(self.get_default_helper_set());
        }

        self.helper_set.as_ref().unwrap().clone()
    }

    pub fn set_definition(&mut self, definition: Rc<RefCell<InputDefinition>>) {
        self.definition = Some(definition);
    }

    /// Gets the InputDefinition related to this Application.
    pub fn get_definition(&mut self) -> Rc<RefCell<InputDefinition>> {
        if self.definition.is_none() {
            self.definition = Some(Rc::new(RefCell::new(self.get_default_input_definition())));
        }

        if self.single_command {
            let input_definition = self.definition.as_ref().unwrap().clone();
            input_definition
                .borrow_mut()
                .set_arguments(Vec::new())
                .unwrap();

            return input_definition;
        }

        self.definition.as_ref().unwrap().clone()
    }

    /// Adds suggestions to `suggestions` for the current completion input (e.g. option or argument).
    pub fn complete(
        &mut self,
        input: &CompletionInput,
        suggestions: &mut CompletionSuggestions,
    ) -> anyhow::Result<()> {
        if CompletionInput::TYPE_ARGUMENT_VALUE == input.get_completion_type()
            && input.get_completion_name().as_deref() == Some("command")
        {
            let mut command_names: Vec<PhpMixed> = Vec::new();
            for (name, command) in self.all(None)? {
                // skip hidden commands and aliased commands as they already get added below
                if command.borrow().is_hidden() || command.borrow().get_name() != Some(name.clone())
                {
                    continue;
                }
                command_names.push(PhpMixed::from(
                    command.borrow().get_name().unwrap_or_default(),
                ));
                for name in command.borrow().get_aliases() {
                    command_names.push(PhpMixed::from(name));
                }
            }
            // array_filter($commandNames)
            let filtered: Vec<crate::symfony::console::completion::completion_suggestions::StringOrSuggestion> =
                command_names
                    .into_iter()
                    .filter(|n| shirabe_php_shim::php_truthy(n))
                    .map(|n| {
                        crate::symfony::console::completion::completion_suggestions::StringOrSuggestion::String(
                            shirabe_php_shim::php_to_string(&n),
                        )
                    })
                    .collect();
            suggestions.suggest_values(filtered);

            return Ok(());
        }

        if CompletionInput::TYPE_OPTION_NAME == input.get_completion_type() {
            // $suggestions->suggestOptions($this->getDefinition()->getOptions());
            // TODO(review): get_options() yields Rc<InputOption> (shared, non-Clone) while
            // suggest_options() consumes owned InputOption values; an ownership/clone strategy
            // for InputOption is needed.
            suggestions.suggest_options(todo!("owned options from get_definition().get_options()"));

            return Ok(());
        }

        Ok(())
    }

    /// Gets the help message.
    pub fn get_help(&self) -> String {
        self.get_long_version()
    }

    /// Gets whether to catch exceptions or not during commands execution.
    pub fn are_exceptions_caught(&self) -> bool {
        self.catch_exceptions
    }

    /// Sets whether to catch exceptions or not during commands execution.
    pub fn set_catch_exceptions(&mut self, boolean: bool) {
        self.catch_exceptions = boolean;
    }

    /// Gets whether to automatically exit after a command execution or not.
    pub fn is_auto_exit_enabled(&self) -> bool {
        self.auto_exit
    }

    /// Sets whether to automatically exit after a command execution or not.
    pub fn set_auto_exit(&mut self, boolean: bool) {
        self.auto_exit = boolean;
    }

    /// Gets the name of the application.
    pub fn get_name(&self) -> String {
        self.name.clone()
    }

    /// Sets the application name.
    pub fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
    }

    /// Gets the application version.
    pub fn get_version(&self) -> String {
        self.version.clone()
    }

    /// Sets the application version.
    pub fn set_version(&mut self, version: &str) {
        self.version = version.to_string();
    }

    /// Returns the long version of the application.
    pub fn get_long_version(&self) -> String {
        if "UNKNOWN" != self.get_name() {
            if "UNKNOWN" != self.get_version() {
                return shirabe_php_shim::sprintf(
                    "%s <info>%s</info>",
                    &[
                        PhpMixed::from(self.get_name()),
                        PhpMixed::from(self.get_version()),
                    ],
                );
            }

            return self.get_name();
        }

        "Console Tool".to_string()
    }

    /// Adds an array of command objects.
    ///
    /// If a Command is not enabled it will not be added.
    pub fn add_commands(&mut self, commands: Vec<Rc<RefCell<dyn Command>>>) -> anyhow::Result<()> {
        for command in commands {
            self.add(command)?;
        }
        Ok(())
    }

    /// Adds a command object.
    ///
    /// If a command with the same name already exists, it will be overridden.
    /// If the command is not enabled it will not be added.
    pub fn add(
        &mut self,
        command: Rc<RefCell<dyn Command>>,
    ) -> anyhow::Result<Option<Rc<RefCell<dyn Command>>>> {
        self.init()?;

        // TODO(review): $command->setApplication($this) needs an Rc<RefCell<Application>> to the
        // current instance. Application is held by value here; the self-reference required to set
        // the command's back-pointer needs the shared-ownership design (Phase C).
        command
            .borrow_mut()
            .set_application(todo!("Rc<RefCell<Application>> of self"));

        if !command.borrow().is_enabled() {
            command.borrow_mut().set_application(None);

            return Ok(None);
        }

        // if (!$command instanceof LazyCommand) { $command->getDefinition(); }
        // TODO(review): LazyCommand vs Command type distinction; eager definition probe omitted
        // pending lazy-command representation decision.
        command.borrow().get_definition();

        if command.borrow().get_name().is_none() {
            return Err(LogicException(shirabe_php_shim::LogicException {
                message: shirabe_php_shim::sprintf(
                    "The command defined in \"%s\" cannot have an empty name.",
                    &[PhpMixed::from(shirabe_php_shim::get_debug_type_obj(
                        &command,
                    ))],
                ),
                code: 0,
            })
            .into());
        }

        let name = command.borrow().get_name().unwrap();
        self.commands.insert(name, command.clone());

        for alias in command.borrow().get_aliases() {
            self.commands.insert(alias, command.clone());
        }

        Ok(Some(command))
    }

    /// Returns a registered command by name or alias.
    ///
    /// Throws CommandNotFoundException when given command name does not exist.
    pub fn get(&mut self, name: &str) -> anyhow::Result<Rc<RefCell<dyn Command>>> {
        self.init()?;

        if !self.has(name) {
            return Err(CommandNotFoundException::new(
                shirabe_php_shim::sprintf(
                    "The command \"%s\" does not exist.",
                    &[PhpMixed::from(name.to_string())],
                ),
                Vec::new(),
                0,
            )
            .into());
        }

        // When the command has a different name than the one used at the command loader level
        if !self.commands.contains_key(name) {
            return Err(CommandNotFoundException::new(
                shirabe_php_shim::sprintf(
                    "The \"%s\" command cannot be found because it is registered under multiple names. Make sure you don't set a different name via constructor or \"setName()\".",
                    &[PhpMixed::from(name.to_string())],
                ),
                Vec::new(),
                0,
            )
            .into());
        }

        let command = self.commands[name].clone();

        if self.want_helps {
            self.want_helps = false;

            let help_command = self.get("help")?;
            // $helpCommand->setCommand($command);
            // TODO(review): setCommand() is defined on HelpCommand, not on the concrete `Command`
            // struct; calling it through the Rc<RefCell<dyn Command>> needs the Command-subclass
            // representation decision (downcast to HelpCommand).
            let _ = &command;
            todo!("help_command.set_command(command)");

            #[allow(unreachable_code)]
            return Ok(help_command);
        }

        Ok(command)
    }

    /// Returns true if the command exists, false otherwise.
    pub fn has(&mut self, name: &str) -> bool {
        self.init().unwrap();

        if self.commands.contains_key(name) {
            return true;
        }

        if let Some(command_loader) = &self.command_loader {
            if command_loader.has(name) {
                let command = command_loader.get(name);
                // $this->add($this->commandLoader->get($name))
                // TODO(review): command_loader.get() returns Box<dyn Command> while add() expects
                // Rc<RefCell<dyn Command>>; the loader return type needs reconciliation.
                let _ = command;
                return self
                    .add(todo!(
                        "Rc<RefCell<dyn Command>> from command_loader.get(name)"
                    ))
                    .map(|c| c.is_some())
                    .unwrap_or(false);
            }
        }

        false
    }

    /// Returns an array of all unique namespaces used by currently registered commands.
    ///
    /// It does not return the global namespace which always exists.
    pub fn get_namespaces(&mut self) -> anyhow::Result<Vec<String>> {
        let mut namespaces: Vec<Vec<String>> = Vec::new();
        for command in self.all(None)?.values() {
            if command.borrow().is_hidden() {
                continue;
            }

            namespaces.push(
                self.extract_all_namespaces(&command.borrow().get_name().unwrap_or_default()),
            );

            for alias in command.borrow().get_aliases() {
                namespaces.push(self.extract_all_namespaces(&alias));
            }
        }

        // array_values(array_unique(array_filter(array_merge([], ...$namespaces))))
        let mut merged: Vec<String> = Vec::new();
        for ns in namespaces {
            merged.extend(ns);
        }
        let merged: Vec<String> = merged.into_iter().filter(|s| !s.is_empty()).collect();
        let mut seen = std::collections::HashSet::new();
        let unique: Vec<String> = merged
            .into_iter()
            .filter(|s| seen.insert(s.clone()))
            .collect();

        Ok(unique)
    }

    /// Finds a registered namespace by a name or an abbreviation.
    ///
    /// Throws NamespaceNotFoundException when namespace is incorrect or ambiguous.
    pub fn find_namespace(&mut self, namespace: &str) -> anyhow::Result<String> {
        let all_namespaces = self.get_namespaces()?;
        // implode('[^:]*:', array_map('preg_quote', explode(':', $namespace))).'[^:]*'
        let parts: Vec<String> = shirabe_php_shim::explode(":", namespace)
            .into_iter()
            .map(|p| shirabe_php_shim::preg_quote(&p, None))
            .collect();
        let expr = format!("{}{}", shirabe_php_shim::implode("[^:]*:", &parts), "[^:]*");
        let namespaces = shirabe_php_shim::preg_grep(&format!("{{^{}}}", expr), &all_namespaces);

        if namespaces.is_empty() {
            let mut message = shirabe_php_shim::sprintf(
                "There are no commands defined in the \"%s\" namespace.",
                &[PhpMixed::from(namespace.to_string())],
            );

            let alternatives = self.find_alternatives(namespace, &all_namespaces);
            if !alternatives.is_empty() {
                if alternatives.len() == 1 {
                    message.push_str("\n\nDid you mean this?\n    ");
                } else {
                    message.push_str("\n\nDid you mean one of these?\n    ");
                }

                message.push_str(&shirabe_php_shim::implode("\n    ", &alternatives));
            }

            return Err(NamespaceNotFoundException(CommandNotFoundException::new(
                message,
                alternatives,
                0,
            ))
            .into());
        }

        let exact = namespaces.iter().any(|n| n == namespace);
        if namespaces.len() > 1 && !exact {
            return Err(NamespaceNotFoundException(CommandNotFoundException::new(
                shirabe_php_shim::sprintf(
                    "The namespace \"%s\" is ambiguous.\nDid you mean one of these?\n%s.",
                    &[
                        PhpMixed::from(namespace.to_string()),
                        PhpMixed::from(self.get_abbreviation_suggestions(&namespaces)),
                    ],
                ),
                namespaces.clone(),
                0,
            ))
            .into());
        }

        // $exact ? $namespace : reset($namespaces)
        if exact {
            Ok(namespace.to_string())
        } else {
            Ok(namespaces[0].clone())
        }
    }

    /// Finds a command by name or alias.
    ///
    /// Contrary to get, this command tries to find the best match if you give it an
    /// abbreviation of a name or alias.
    ///
    /// Throws CommandNotFoundException when command name is incorrect or ambiguous.
    pub fn find(&mut self, name: &str) -> anyhow::Result<Rc<RefCell<dyn Command>>> {
        self.init()?;

        let mut aliases: IndexMap<String, String> = IndexMap::new();

        let commands_snapshot: Vec<Rc<RefCell<dyn Command>>> =
            self.commands.values().cloned().collect();
        for command in &commands_snapshot {
            for alias in command.borrow().get_aliases() {
                if !self.has(&alias) {
                    self.commands.insert(alias, command.clone());
                }
            }
        }

        if self.has(name) {
            return self.get(name);
        }

        // $allCommands = commandLoader ? array_merge(loader->getNames(), array_keys(commands)) : array_keys(commands)
        let all_commands: Vec<String> = match &self.command_loader {
            Some(command_loader) => {
                let mut all = command_loader.get_names();
                all.extend(self.commands.keys().cloned());
                all
            }
            None => self.commands.keys().cloned().collect(),
        };

        let parts: Vec<String> = shirabe_php_shim::explode(":", name)
            .into_iter()
            .map(|p| shirabe_php_shim::preg_quote(&p, None))
            .collect();
        let expr = format!("{}{}", shirabe_php_shim::implode("[^:]*:", &parts), "[^:]*");
        let mut commands = shirabe_php_shim::preg_grep(&format!("{{^{}}}", expr), &all_commands);

        if commands.is_empty() {
            commands = shirabe_php_shim::preg_grep(&format!("{{^{}}}i", expr), &all_commands);
        }

        // if no commands matched or we just matched namespaces
        if commands.is_empty()
            || shirabe_php_shim::preg_grep(&format!("{{^{}$}}i", expr), &commands).len() < 1
        {
            if let Some(pos) = shirabe_php_shim::strrpos(name, ":") {
                // check if a namespace exists and contains commands
                self.find_namespace(&name[..pos as usize])?;
            }

            let mut message = shirabe_php_shim::sprintf(
                "Command \"%s\" is not defined.",
                &[PhpMixed::from(name.to_string())],
            );

            let mut alternatives = self.find_alternatives(name, &all_commands);
            if !alternatives.is_empty() {
                // remove hidden commands
                let mut filtered: Vec<String> = Vec::new();
                for alt in alternatives {
                    if !self.get(&alt)?.borrow().is_hidden() {
                        filtered.push(alt);
                    }
                }
                alternatives = filtered;

                if alternatives.len() == 1 {
                    message.push_str("\n\nDid you mean this?\n    ");
                } else {
                    message.push_str("\n\nDid you mean one of these?\n    ");
                }
                message.push_str(&shirabe_php_shim::implode("\n    ", &alternatives));
            }

            return Err(CommandNotFoundException::new(message, alternatives, 0).into());
        }

        // filter out aliases for commands which are already on the list
        if commands.len() > 1 {
            // $commandList = commandLoader ? array_merge(array_flip(loader->getNames()), commands) : commands
            // TODO(review): $commandList mixes flipped loader names (string => int) with Command
            // instances; this heterogeneous PHP array needs a typed representation. The alias
            // de-duplication and the loader->get() lazy materialization are left to design.
            let mut command_list: IndexMap<String, Rc<RefCell<dyn Command>>> =
                self.commands.clone();

            let commands_clone = commands.clone();
            let mut new_commands: Vec<String> = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for name_or_alias in commands {
                if !command_list.contains_key(&name_or_alias) {
                    let loaded = self.command_loader.as_ref().unwrap().get(&name_or_alias);
                    let _ = loaded;
                    command_list.insert(
                        name_or_alias.clone(),
                        todo!("Rc<RefCell<dyn Command>> from command_loader.get(name_or_alias)"),
                    );
                }

                let command_name = command_list[&name_or_alias]
                    .borrow()
                    .get_name()
                    .unwrap_or_default();

                aliases.insert(name_or_alias.clone(), command_name.clone());

                let keep = command_name == name_or_alias || !commands_clone.contains(&command_name);
                if keep && seen.insert(name_or_alias.clone()) {
                    new_commands.push(name_or_alias);
                }
            }
            commands = new_commands;

            if commands.len() > 1 {
                let usable_width = self.terminal.get_width() - 10;
                let abbrevs: Vec<String> = commands.clone();
                let mut max_len: i64 = 0;
                for abbrev in &abbrevs {
                    max_len = std::cmp::max(Helper::width(abbrev), max_len);
                }
                let mut formatted_abbrevs: Vec<PhpMixed> = Vec::new();
                for cmd in commands.clone() {
                    if command_list[&cmd].borrow().is_hidden() {
                        // unset($commands[array_search($cmd, $commands)])
                        if let Some(idx) = commands.iter().position(|c| *c == cmd) {
                            commands.remove(idx);
                        }
                        formatted_abbrevs.push(PhpMixed::Bool(false));
                        continue;
                    }

                    let abbrev = format!(
                        "{} {}",
                        shirabe_php_shim::str_pad(
                            &cmd,
                            max_len as usize,
                            " ",
                            shirabe_php_shim::STR_PAD_LEFT
                        ),
                        command_list[&cmd].borrow().get_description()
                    );

                    if Helper::width(&abbrev) > usable_width {
                        formatted_abbrevs.push(PhpMixed::from(format!(
                            "{}...",
                            Helper::substr(&abbrev, 0, Some(usable_width - 3))
                        )));
                    } else {
                        formatted_abbrevs.push(PhpMixed::from(abbrev));
                    }
                }

                if commands.len() > 1 {
                    let filtered: Vec<String> = formatted_abbrevs
                        .iter()
                        .filter(|a| shirabe_php_shim::php_truthy(a))
                        .map(|a| shirabe_php_shim::php_to_string(a))
                        .collect();
                    let suggestions = self.get_abbreviation_suggestions(&filtered);

                    return Err(CommandNotFoundException::new(
                        shirabe_php_shim::sprintf(
                            "Command \"%s\" is ambiguous.\nDid you mean one of these?\n%s.",
                            &[
                                PhpMixed::from(name.to_string()),
                                PhpMixed::from(suggestions),
                            ],
                        ),
                        commands.clone(),
                        0,
                    )
                    .into());
                }
            }
        }

        // $command = $this->get(reset($commands));
        let command = self.get(&commands[0])?;

        if command.borrow().is_hidden() {
            return Err(CommandNotFoundException::new(
                shirabe_php_shim::sprintf(
                    "The command \"%s\" does not exist.",
                    &[PhpMixed::from(name.to_string())],
                ),
                Vec::new(),
                0,
            )
            .into());
        }

        Ok(command)
    }

    /// Gets the commands (registered in the given namespace if provided).
    ///
    /// The array keys are the full names and the values the command instances.
    pub fn all(
        &mut self,
        namespace: Option<&str>,
    ) -> anyhow::Result<IndexMap<String, Rc<RefCell<dyn Command>>>> {
        self.init()?;

        if namespace.is_none() {
            if self.command_loader.is_none() {
                return Ok(self.commands.clone());
            }

            let mut commands = self.commands.clone();
            let names = self.command_loader.as_ref().unwrap().get_names();
            for name in names {
                if !commands.contains_key(&name) && self.has(&name) {
                    commands.insert(name.clone(), self.get(&name)?);
                }
            }

            return Ok(commands);
        }

        let namespace = namespace.unwrap();
        let mut commands: IndexMap<String, Rc<RefCell<dyn Command>>> = IndexMap::new();
        let entries: Vec<(String, Rc<RefCell<dyn Command>>)> = self
            .commands
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (name, command) in entries {
            if namespace
                == self.extract_namespace(
                    &name,
                    Some(shirabe_php_shim::substr_count(namespace, ":") + 1),
                )
            {
                commands.insert(name, command);
            }
        }

        if self.command_loader.is_some() {
            let names = self.command_loader.as_ref().unwrap().get_names();
            for name in names {
                if !commands.contains_key(&name)
                    && namespace
                        == self.extract_namespace(
                            &name,
                            Some(shirabe_php_shim::substr_count(namespace, ":") + 1),
                        )
                    && self.has(&name)
                {
                    commands.insert(name.clone(), self.get(&name)?);
                }
            }
        }

        Ok(commands)
    }

    /// Returns an array of possible abbreviations given a set of names.
    pub fn get_abbreviations(names: Vec<String>) -> IndexMap<String, Vec<String>> {
        let mut abbrevs: IndexMap<String, Vec<String>> = IndexMap::new();
        for name in names {
            let mut len = shirabe_php_shim::strlen(&name);
            while len > 0 {
                let abbrev = shirabe_php_shim::substr(&name, 0, Some(len));
                abbrevs.entry(abbrev).or_default().push(name.clone());
                len -= 1;
            }
        }

        abbrevs
    }

    pub fn render_throwable(&self, e: &anyhow::Error, output: Rc<RefCell<dyn OutputInterface>>) {
        output
            .borrow()
            .writeln(&["".to_string()], output_interface::VERBOSITY_QUIET);

        self.do_render_throwable(e, output.clone());

        if let Some(running_command) = &self.running_command {
            output.borrow().writeln(
                &[shirabe_php_shim::sprintf(
                    "<info>%s</info>",
                    &[PhpMixed::from(
                        OutputFormatter::escape(&shirabe_php_shim::sprintf(
                            &running_command.borrow_mut().get_synopsis(false),
                            &[PhpMixed::from(self.get_name())],
                        ))
                        .unwrap(),
                    )],
                )],
                output_interface::VERBOSITY_QUIET,
            );
            output
                .borrow()
                .writeln(&["".to_string()], output_interface::VERBOSITY_QUIET);
        }
    }

    pub fn do_render_throwable(&self, e: &anyhow::Error, output: Rc<RefCell<dyn OutputInterface>>) {
        // do { ... } while ($e = $e->getPrevious());
        // TODO(review): PHP walks the exception chain via getPrevious() and reads getMessage(),
        // getCode(), getFile(), getLine(), getTrace(). anyhow::Error exposes a source() chain but
        // not file/line/trace; faithful rendering of the trace needs a Throwable-equivalent.
        let _ = output;
        let _ = e;
        todo!("render exception chain (getMessage/getCode/getFile/getLine/getTrace/getPrevious)")
    }

    /// Configures the input and output instances based on the user arguments and options.
    pub fn configure_io(
        &self,
        input: &Rc<RefCell<dyn InputInterface>>,
        output: &Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        if input.borrow().has_parameter_option(
            PhpMixed::from(vec![PhpMixed::from("--ansi".to_string())]),
            true,
        ) {
            output.borrow().set_decorated(true);
        } else if input.borrow().has_parameter_option(
            PhpMixed::from(vec![PhpMixed::from("--no-ansi".to_string())]),
            true,
        ) {
            output.borrow().set_decorated(false);
        }

        if input.borrow().has_parameter_option(
            PhpMixed::from(vec![
                PhpMixed::from("--no-interaction".to_string()),
                PhpMixed::from("-n".to_string()),
            ]),
            true,
        ) {
            input.borrow_mut().set_interactive(false);
        }

        let mut shell_verbosity = shirabe_php_shim::getenv("SHELL_VERBOSITY").unwrap_or_default();
        let shell_verbosity_int: i64 = shell_verbosity.parse().unwrap_or(0);
        let mut shell_verbosity: i64 = shell_verbosity_int;
        match shell_verbosity_int {
            -1 => {
                output
                    .borrow()
                    .set_verbosity(output_interface::VERBOSITY_QUIET);
            }
            1 => {
                output
                    .borrow()
                    .set_verbosity(output_interface::VERBOSITY_VERBOSE);
            }
            2 => {
                output
                    .borrow()
                    .set_verbosity(output_interface::VERBOSITY_VERY_VERBOSE);
            }
            3 => {
                output
                    .borrow()
                    .set_verbosity(output_interface::VERBOSITY_DEBUG);
            }
            _ => {
                shell_verbosity = 0;
            }
        }

        if input.borrow().has_parameter_option(
            PhpMixed::from(vec![
                PhpMixed::from("--quiet".to_string()),
                PhpMixed::from("-q".to_string()),
            ]),
            true,
        ) {
            output
                .borrow()
                .set_verbosity(output_interface::VERBOSITY_QUIET);
            shell_verbosity = -1;
        } else if input
            .borrow()
            .has_parameter_option(PhpMixed::from("-vvv".to_string()), true)
            || input
                .borrow()
                .has_parameter_option(PhpMixed::from("--verbose=3".to_string()), true)
            || input.borrow().get_parameter_option(
                PhpMixed::from("--verbose".to_string()),
                PhpMixed::Bool(false),
                true,
            ) == PhpMixed::from(3i64)
        {
            output
                .borrow()
                .set_verbosity(output_interface::VERBOSITY_DEBUG);
            shell_verbosity = 3;
        } else if input
            .borrow()
            .has_parameter_option(PhpMixed::from("-vv".to_string()), true)
            || input
                .borrow()
                .has_parameter_option(PhpMixed::from("--verbose=2".to_string()), true)
            || input.borrow().get_parameter_option(
                PhpMixed::from("--verbose".to_string()),
                PhpMixed::Bool(false),
                true,
            ) == PhpMixed::from(2i64)
        {
            output
                .borrow()
                .set_verbosity(output_interface::VERBOSITY_VERY_VERBOSE);
            shell_verbosity = 2;
        } else if input
            .borrow()
            .has_parameter_option(PhpMixed::from("-v".to_string()), true)
            || input
                .borrow()
                .has_parameter_option(PhpMixed::from("--verbose=1".to_string()), true)
            || input
                .borrow()
                .has_parameter_option(PhpMixed::from("--verbose".to_string()), true)
            || shirabe_php_shim::php_truthy(&input.borrow().get_parameter_option(
                PhpMixed::from("--verbose".to_string()),
                PhpMixed::Bool(false),
                true,
            ))
        {
            output
                .borrow()
                .set_verbosity(output_interface::VERBOSITY_VERBOSE);
            shell_verbosity = 1;
        }

        if shell_verbosity == -1 {
            input.borrow_mut().set_interactive(false);
        }

        if shirabe_php_shim::function_exists("putenv") {
            shirabe_php_shim::putenv(&format!("SHELL_VERBOSITY={}", shell_verbosity));
        }
        shirabe_php_shim::env_set("SHELL_VERBOSITY", shell_verbosity.to_string());
        shirabe_php_shim::server_set("SHELL_VERBOSITY", shell_verbosity.to_string());

        let _ = &mut shell_verbosity;

        Ok(())
    }

    /// Runs the current command.
    ///
    /// If an event dispatcher has been attached to the application, events are also
    /// dispatched during the life-cycle of the command.
    ///
    /// Returns 0 if everything went fine, or an error code.
    pub fn do_run_command(
        &mut self,
        command: Rc<RefCell<dyn Command>>,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        if let Some(helper_set) = command.borrow().get_helper_set() {
            for (_alias, helper) in helper_set.borrow().get_iterator() {
                // if ($helper instanceof InputAwareInterface) $helper->setInput($input);
                // TODO(review): downcasting a HelperInterface to InputAwareInterface is not
                // expressible without a typed mechanism; needs design.
                let _ = helper;
                let _ = std::marker::PhantomData::<dyn InputAwareInterface>;
            }
        }

        if !self.signals_to_dispatch_event.is_empty() {
            // $commandSignals = $command instanceof SignalableCommandInterface ? $command->getSubscribedSignals() : []
            // TODO(review): Command is not a SignalableCommandInterface here; downcast needed.
            let command_signals: Vec<i64> = Vec::new();
            let _ = std::marker::PhantomData::<dyn SignalableCommandInterface>;

            if !command_signals.is_empty() || self.dispatcher.is_some() {
                if self.signal_registry.is_none() {
                    return Err(RuntimeException(shirabe_php_shim::RuntimeException {
                        message: "Unable to subscribe to signal events. Make sure that the `pcntl` extension is installed and that \"pcntl_*\" functions are not disabled by your php.ini's \"disable_functions\" directive.".to_string(),
                        code: 0,
                    })
                    .into());
                }

                if Terminal::has_stty_available() {
                    // TODO: registers SIGINT/SIGTERM handlers that restore the stty mode via
                    // shell_exec('stty ...'). pcntl signal handlers have no faithful Rust
                    // equivalent in Phase A.
                    let _stty_mode = shirabe_php_shim::shell_exec("stty -g");
                    for _signal in [shirabe_php_shim::SIGINT, shirabe_php_shim::SIGTERM] {
                        todo!("register signal handler to restore stty mode");
                    }
                }
            }

            if self.dispatcher.is_some() {
                // TODO(plugin): for each signal, register a handler that dispatches ConsoleSignalEvent.
                for &signal in &self.signals_to_dispatch_event.clone() {
                    let _event = ConsoleSignalEvent::new(
                        todo!("Box<dyn Command>"),
                        todo!("Box<dyn InputInterface>"),
                        todo!("Box<dyn OutputInterface>"),
                        signal,
                    );
                    todo!("register signal handler dispatching ConsoleEvents::SIGNAL");
                }
            }

            for _signal in command_signals {
                // $this->signalRegistry->register($signal, [$command, 'handleSignal']);
                todo!("register command->handle_signal as signal handler");
            }
        }

        if self.dispatcher.is_none() {
            return command.borrow_mut().run(
                &mut *borrow_input_mut(&input),
                &mut *borrow_output_mut(&output),
            );
        }

        // bind before the console.command event, so the listeners have access to input options/arguments
        match (|| -> anyhow::Result<()> {
            command.borrow_mut().merge_application_definition(true);
            input.borrow_mut().bind(command.borrow().get_definition())?;
            Ok(())
        })() {
            Ok(()) => {}
            Err(e) => {
                // ignore invalid options/arguments for now, to allow the event listeners to customize the InputDefinition
                if !is_exception_interface(&e) {
                    return Err(e);
                }
            }
        }

        // TODO(plugin): the whole dispatcher block below drives ConsoleCommandEvent /
        // ConsoleErrorEvent / ConsoleTerminateEvent. The event objects require Box<dyn ...>
        // wrappers for input/output/command and the dispatcher's dispatch() contract; their
        // construction is left to the plugin/event design.
        let _ = ConsoleCommandEvent::RETURN_CODE_DISABLED;
        let _ = std::marker::PhantomData::<(
            ConsoleCommandEvent,
            ConsoleErrorEvent,
            ConsoleTerminateEvent,
        )>;
        todo!("dispatcher-driven command run (console.command / console.error / console.terminate)")
    }

    /// Gets the name of the command based on input.
    pub fn get_command_name(&self, input: &dyn InputInterface) -> Option<String> {
        if self.single_command {
            Some(self.default_command.clone())
        } else {
            input.get_first_argument()
        }
    }

    /// Gets the default input definition.
    pub fn get_default_input_definition(&self) -> InputDefinition {
        use crate::symfony::console::input::input_definition::DefinitionItem;
        InputDefinition::new(vec![
            DefinitionItem::InputArgument(
                InputArgument::new(
                    "command".to_string(),
                    Some(InputArgument::REQUIRED),
                    "The command to execute".to_string(),
                    PhpMixed::Null,
                )
                .unwrap(),
            ),
            DefinitionItem::InputOption(
                InputOption::new(
                    "--help",
                    PhpMixed::from("-h".to_string()),
                    Some(InputOption::VALUE_NONE),
                    format!(
                        "Display help for the given command. When no command is given display help for the <info>{}</info> command",
                        self.default_command
                    ),
                    PhpMixed::Null,
                )
                .unwrap(),
            ),
            DefinitionItem::InputOption(
                InputOption::new(
                    "--quiet",
                    PhpMixed::from("-q".to_string()),
                    Some(InputOption::VALUE_NONE),
                    "Do not output any message".to_string(),
                    PhpMixed::Null,
                )
                .unwrap(),
            ),
            DefinitionItem::InputOption(
                InputOption::new(
                    "--verbose",
                    PhpMixed::from("-v|vv|vvv".to_string()),
                    Some(InputOption::VALUE_NONE),
                    "Increase the verbosity of messages: 1 for normal output, 2 for more verbose output and 3 for debug".to_string(),
                    PhpMixed::Null,
                )
                .unwrap(),
            ),
            DefinitionItem::InputOption(
                InputOption::new(
                    "--version",
                    PhpMixed::from("-V".to_string()),
                    Some(InputOption::VALUE_NONE),
                    "Display this application version".to_string(),
                    PhpMixed::Null,
                )
                .unwrap(),
            ),
            DefinitionItem::InputOption(
                InputOption::new(
                    "--ansi",
                    PhpMixed::from("".to_string()),
                    Some(InputOption::VALUE_NEGATABLE),
                    "Force (or disable --no-ansi) ANSI output".to_string(),
                    PhpMixed::Null,
                )
                .unwrap(),
            ),
            DefinitionItem::InputOption(
                InputOption::new(
                    "--no-interaction",
                    PhpMixed::from("-n".to_string()),
                    Some(InputOption::VALUE_NONE),
                    "Do not ask any interactive question".to_string(),
                    PhpMixed::Null,
                )
                .unwrap(),
            ),
        ])
        .unwrap()
    }

    /// Gets the default commands that should always be available.
    pub fn get_default_commands(&self) -> Vec<Rc<RefCell<dyn Command>>> {
        // return [new HelpCommand(), new ListCommand(), new CompleteCommand(), new DumpCompletionCommand()];
        // TODO(review): HelpCommand/ListCommand/CompleteCommand/DumpCompletionCommand are ported as
        // distinct structs (not subtypes of the concrete `Command` struct), so they cannot populate
        // a Vec<Rc<RefCell<dyn Command>>>. Reconciling Command subclassing with Rust requires the
        // command-hierarchy design decision (see also `add`/`find`/LazyCommand handling).
        let _ = std::marker::PhantomData::<(
            HelpCommand,
            ListCommand,
            CompleteCommand,
            DumpCompletionCommand,
        )>;
        todo!("construct default commands once Command-subclass representation is decided")
    }

    /// Gets the default helper set with the helpers that should always be available.
    pub fn get_default_helper_set(&self) -> Rc<RefCell<HelperSet>> {
        use crate::symfony::console::helper::helper_interface::HelperInterface;
        let helper_set = Rc::new(RefCell::new(HelperSet::default()));
        let helpers: IndexMap<
            crate::symfony::console::helper::helper_set::HelperSetKey,
            Rc<RefCell<dyn HelperInterface>>,
        > = {
            let mut m: IndexMap<
                crate::symfony::console::helper::helper_set::HelperSetKey,
                Rc<RefCell<dyn HelperInterface>>,
            > = IndexMap::new();
            m.insert(
                crate::symfony::console::helper::helper_set::HelperSetKey::Int(0),
                Rc::new(RefCell::new(FormatterHelper::default())),
            );
            m.insert(
                crate::symfony::console::helper::helper_set::HelperSetKey::Int(1),
                Rc::new(RefCell::new(DebugFormatterHelper::default())),
            );
            m.insert(
                crate::symfony::console::helper::helper_set::HelperSetKey::Int(2),
                Rc::new(RefCell::new(ProcessHelper::default())),
            );
            m.insert(
                crate::symfony::console::helper::helper_set::HelperSetKey::Int(3),
                Rc::new(RefCell::new(QuestionHelper::default())),
            );
            m
        };
        HelperSet::new(&helper_set, helpers);
        helper_set
    }

    /// Returns abbreviated suggestions in string format.
    fn get_abbreviation_suggestions(&self, abbrevs: &[String]) -> String {
        format!("    {}", shirabe_php_shim::implode("\n    ", abbrevs))
    }

    /// Returns the namespace part of the command name.
    ///
    /// This method is not part of public API and should not be used directly.
    pub fn extract_namespace(&self, name: &str, limit: Option<i64>) -> String {
        // $parts = explode(':', $name, -1);
        let parts = shirabe_php_shim::explode_limit(":", name, -1);

        // implode(':', null === $limit ? $parts : array_slice($parts, 0, $limit))
        match limit {
            None => shirabe_php_shim::implode(":", &parts),
            Some(limit) => {
                let sliced: Vec<String> = parts.into_iter().take(limit.max(0) as usize).collect();
                shirabe_php_shim::implode(":", &sliced)
            }
        }
    }

    /// Finds alternative of $name among $collection, if nothing is found in
    /// $collection, try in $abbrevs.
    fn find_alternatives(&self, name: &str, collection: &[String]) -> Vec<String> {
        let threshold = 1e3;
        let mut alternatives: IndexMap<String, f64> = IndexMap::new();

        let mut collection_parts: IndexMap<String, Vec<String>> = IndexMap::new();
        for item in collection {
            collection_parts.insert(item.clone(), shirabe_php_shim::explode(":", item));
        }

        for (i, subname) in shirabe_php_shim::explode(":", name).into_iter().enumerate() {
            for (collection_name, parts) in &collection_parts {
                let exists = alternatives.contains_key(collection_name);
                if parts.get(i).is_none() && exists {
                    *alternatives.get_mut(collection_name).unwrap() += threshold;
                    continue;
                } else if parts.get(i).is_none() {
                    continue;
                }

                let lev = shirabe_php_shim::levenshtein(&subname, &parts[i]) as f64;
                if lev <= shirabe_php_shim::strlen(&subname) as f64 / 3.0
                    || (!subname.is_empty() && parts[i].contains(&subname))
                {
                    let v = if exists {
                        alternatives[collection_name] + lev
                    } else {
                        lev
                    };
                    alternatives.insert(collection_name.clone(), v);
                } else if exists {
                    *alternatives.get_mut(collection_name).unwrap() += threshold;
                }
            }
        }

        for item in collection {
            let lev = shirabe_php_shim::levenshtein(name, item) as f64;
            if lev <= shirabe_php_shim::strlen(name) as f64 / 3.0 || item.contains(name) {
                let v = if alternatives.contains_key(item) {
                    alternatives[item] - lev
                } else {
                    lev
                };
                alternatives.insert(item.clone(), v);
            }
        }

        // array_filter($alternatives, fn($lev) => $lev < 2 * $threshold)
        alternatives.retain(|_, lev| *lev < 2.0 * threshold);
        // ksort($alternatives, SORT_NATURAL | SORT_FLAG_CASE)
        let mut keys: Vec<String> = alternatives.keys().cloned().collect();
        shirabe_php_shim::sort_natural_flag_case(&mut keys);

        keys
    }

    /// Sets the default Command name.
    pub fn set_default_command(
        &mut self,
        command_name: &str,
        is_single_command: bool,
    ) -> anyhow::Result<&mut Self> {
        // $this->defaultCommand = explode('|', ltrim($commandName, '|'))[0];
        let trimmed = shirabe_php_shim::ltrim(command_name, Some("|"));
        self.default_command = shirabe_php_shim::explode("|", &trimmed)
            .into_iter()
            .next()
            .unwrap_or_default();

        if is_single_command {
            // Ensure the command exist
            self.find(command_name)?;

            self.single_command = true;
        }

        Ok(self)
    }

    /// @internal
    pub fn is_single_command(&self) -> bool {
        self.single_command
    }

    fn split_string_by_width(&self, string: &str, width: i64) -> Vec<String> {
        // str_split is not suitable for multi-byte characters, we should use preg_split to get char array properly.
        let encoding = match shirabe_php_shim::mb_detect_encoding(string, None, true) {
            None => return shirabe_php_shim::str_split(string, width),
            Some(encoding) => encoding,
        };

        let utf8_string = shirabe_php_shim::mb_convert_encoding(string.into(), "utf8", &encoding);
        let mut lines: Vec<String> = Vec::new();
        let mut line = String::new();

        let mut offset = 0i64;
        let mut m: Vec<String> = Vec::new();
        while shirabe_php_shim::preg_match_offset(r"/.{1,10000}/u", &utf8_string, &mut m, 0, offset)
        {
            offset += shirabe_php_shim::strlen(&m[0]);

            for char in shirabe_php_shim::preg_split_chars(r"//u", &m[0]) {
                // test if $char could be appended to current line
                if shirabe_php_shim::mb_strwidth(&format!("{}{}", line, char), Some("utf8"))
                    <= width
                {
                    line.push_str(&char);
                    continue;
                }
                // if not, push current line to array and make new line
                lines.push(shirabe_php_shim::str_pad(
                    &line,
                    width as usize,
                    " ",
                    shirabe_php_shim::STR_PAD_LEFT,
                ));
                line = char;
            }
        }

        lines.push(if !lines.is_empty() {
            shirabe_php_shim::str_pad(&line, width as usize, " ", shirabe_php_shim::STR_PAD_LEFT)
        } else {
            line.clone()
        });

        shirabe_php_shim::mb_convert_variables(&encoding, "utf8", &mut lines);

        lines
    }

    /// Returns all namespaces of the command name.
    fn extract_all_namespaces(&self, name: &str) -> Vec<String> {
        // -1 as third argument is needed to skip the command short name when exploding
        let parts = shirabe_php_shim::explode_limit(":", name, -1);
        let mut namespaces: Vec<String> = Vec::new();

        for part in parts {
            if !namespaces.is_empty() {
                let last = namespaces.last().unwrap().clone();
                namespaces.push(format!("{}:{}", last, part));
            } else {
                namespaces.push(part);
            }
        }

        namespaces
    }

    fn init(&mut self) -> anyhow::Result<()> {
        if self.initialized {
            return Ok(());
        }
        self.initialized = true;

        for command in self.get_default_commands() {
            self.add(command)?;
        }

        Ok(())
    }
}

impl ResetInterface for Application {
    fn reset(&mut self) {
        Application::reset(self)
    }
}

/// Helper mirroring PHP's `$e instanceof ExceptionInterface`.
fn is_exception_interface(e: &anyhow::Error) -> bool {
    // anyhow::Error stores concrete error types; enumerate the console exceptions
    // that implement ExceptionInterface (PHP's `$e instanceof ExceptionInterface`).
    e.downcast_ref::<CommandNotFoundException>().is_some()
        || e.downcast_ref::<NamespaceNotFoundException>().is_some()
        || e.downcast_ref::<LogicException>().is_some()
        || e.downcast_ref::<RuntimeException>().is_some()
}

/// Helper mirroring PHP's `$e instanceof CommandNotFoundException`.
fn downcast_command_not_found(e: &anyhow::Error) -> Option<&CommandNotFoundException> {
    if let Some(cnf) = e.downcast_ref::<CommandNotFoundException>() {
        return Some(cnf);
    }
    e.downcast_ref::<NamespaceNotFoundException>().map(|n| &n.0)
}

/// Helper mirroring PHP's `$e instanceof NamespaceNotFoundException`.
fn is_namespace_not_found(e: &anyhow::Error) -> bool {
    e.downcast_ref::<NamespaceNotFoundException>().is_some()
}

/// Borrows the shared input as a mutable `dyn InputInterface` for passing to
/// `Command::run`, which takes `&mut dyn InputInterface`.
fn borrow_input_mut(
    input: &Rc<RefCell<dyn InputInterface>>,
) -> std::cell::RefMut<'_, dyn InputInterface> {
    input.borrow_mut()
}

/// Borrows the shared output as a mutable `dyn OutputInterface` for passing to
/// `Command::run`, which takes `&mut dyn OutputInterface`.
fn borrow_output_mut(
    output: &Rc<RefCell<dyn OutputInterface>>,
) -> std::cell::RefMut<'_, dyn OutputInterface> {
    output.borrow_mut()
}
