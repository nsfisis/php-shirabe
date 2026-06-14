//! ref: composer/vendor/symfony/console/Command/Command.php

use crate::symfony::console::application::Application;
use crate::symfony::console::completion::completion_input::CompletionInput;
use crate::symfony::console::completion::completion_suggestions::CompletionSuggestions;
use crate::symfony::console::exception::invalid_argument_exception::InvalidArgumentException;
use crate::symfony::console::exception::logic_exception::LogicException;
use crate::symfony::console::helper::helper_set::HelperSet;
use crate::symfony::console::input::input_argument::InputArgument;
use crate::symfony::console::input::input_definition::InputDefinition;
use crate::symfony::console::input::input_interface::InputInterface;
use crate::symfony::console::input::input_option::InputOption;
use crate::symfony::console::output::output_interface::{self, OutputInterface};
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;
use std::cell::RefCell;
use std::rc::Rc;

/// Base class for all commands.
///
/// Phase B: the PHP `Command` class is split into the polymorphic `Command` trait
/// (defined below) and this concrete `BaseCommand` struct holding the base-class
/// state and behavior. Subclasses embed a `BaseCommand` and implement `Command`.
pub struct BaseCommand {
    application: Option<Rc<RefCell<dyn Application>>>,
    name: Option<String>,
    process_title: Option<String>,
    aliases: Vec<String>,
    definition: Option<InputDefinition>,
    hidden: bool,
    help: String,
    description: String,
    full_definition: Option<InputDefinition>,
    ignore_validation_errors: bool,
    // A callable(InputInterface, OutputInterface) -> i64.
    code: Option<Box<dyn Fn(&mut dyn InputInterface, &mut dyn OutputInterface) -> PhpMixed>>,
    synopsis: IndexMap<String, String>,
    usages: Vec<String>,
    helper_set: Option<Rc<RefCell<HelperSet>>>,
}

impl BaseCommand {
    // see https://tldp.org/LDP/abs/html/exitcodes.html
    pub const SUCCESS: i64 = 0;
    pub const FAILURE: i64 = 1;
    pub const INVALID: i64 = 2;

    /// The default command name.
    // NOTE: PHP `protected static $defaultName`; static late-binding property.
    pub const DEFAULT_NAME: Option<&'static str> = None;

    /// The default command description.
    // NOTE: PHP `protected static $defaultDescription`; static late-binding property.
    pub const DEFAULT_DESCRIPTION: Option<&'static str> = None;

    pub fn get_default_name() -> Option<String> {
        // TODO(review): PHP uses ReflectionClass to read the #[AsCommand] attribute
        // and ReflectionProperty to check that `$defaultName` is declared on the late-static
        // class itself (not inherited). Reflection-based late static binding cannot be
        // reproduced in Phase A; human review needed for the porting strategy.
        todo!()
    }

    pub fn get_default_description() -> Option<String> {
        // TODO(review): same Reflection/late-static-binding concern as get_default_name().
        todo!()
    }

    /// `$name` is the name of the command; passing None means it must be set in configure().
    ///
    /// Throws LogicException when the command name is empty.
    pub fn __construct(name: Option<String>) -> anyhow::Result<Self> {
        let mut this = BaseCommand {
            application: None,
            name: None,
            process_title: None,
            aliases: Vec::new(),
            definition: Some(InputDefinition::new(Vec::new())?),
            hidden: false,
            help: String::new(),
            description: String::new(),
            full_definition: None,
            ignore_validation_errors: false,
            code: None,
            synopsis: IndexMap::new(),
            usages: Vec::new(),
            helper_set: None,
        };

        let mut name = name;
        if name.is_none() {
            name = Self::get_default_name();
            if let Some(n) = name.clone() {
                let mut aliases: Vec<String> = n.split('|').map(|s| s.to_string()).collect();

                let first = if aliases.is_empty() {
                    None
                } else {
                    Some(aliases.remove(0))
                };
                name = first;
                if name.as_deref() == Some("") {
                    this.set_hidden(true);
                    name = if aliases.is_empty() {
                        None
                    } else {
                        Some(aliases.remove(0))
                    };
                }

                this.set_aliases(aliases)?;
            }
        }

        if let Some(n) = name {
            this.set_name(&n)?;
        }

        if this.description.is_empty() {
            this.set_description(&Self::get_default_description().unwrap_or_default());
        }

        this.configure();

        Ok(this)
    }

    /// Ignores validation errors.
    ///
    /// This is mainly useful for the help command.
    pub fn ignore_validation_errors(&mut self) {
        self.ignore_validation_errors = true;
    }

    pub fn set_application(&mut self, application: Option<Rc<RefCell<dyn Application>>>) {
        self.application = application.clone();
        if let Some(application) = application {
            self.set_helper_set(application.borrow_mut().get_helper_set());
        } else {
            self.helper_set = None;
        }

        self.full_definition = None;
    }

    pub fn set_helper_set(&mut self, helper_set: Rc<RefCell<HelperSet>>) {
        self.helper_set = Some(helper_set);
    }

    /// Gets the helper set.
    pub fn get_helper_set(&self) -> Option<Rc<RefCell<HelperSet>>> {
        self.helper_set.clone()
    }

    /// Gets the application instance for this command.
    pub fn get_application(&self) -> Option<Rc<RefCell<dyn Application>>> {
        self.application.clone()
    }

    /// Checks whether the command is enabled or not in the current environment.
    ///
    /// Override this to check for x or y and return false if the command cannot
    /// run properly under the current conditions.
    pub fn is_enabled(&self) -> bool {
        true
    }

    /// Configures the current command.
    pub fn configure(&mut self) {}

    /// Executes the current command.
    ///
    /// This method is not abstract because you can use this class
    /// as a concrete class. In this case, instead of defining the
    /// execute() method, you set the code to execute by passing
    /// a Closure to the set_code() method.
    ///
    /// Returns 0 if everything went fine, or an exit code.
    ///
    /// Throws LogicException when this abstract method is not implemented.
    pub fn execute(
        &mut self,
        _input: &mut dyn InputInterface,
        _output: &mut dyn OutputInterface,
    ) -> anyhow::Result<Result<i64, LogicException>> {
        Ok(Err(LogicException(shirabe_php_shim::LogicException {
            message: "You must override the execute() method in the concrete command class."
                .to_string(),
            code: 0,
        })))
    }

    /// Interacts with the user.
    ///
    /// This method is executed before the InputDefinition is validated.
    /// This means that this is the only place where the command can
    /// interactively ask for values of missing required arguments.
    pub fn interact(&mut self, _input: &mut dyn InputInterface, _output: &mut dyn OutputInterface) {
    }

    /// Initializes the command after the input has been bound and before the input
    /// is validated.
    ///
    /// This is mainly useful when a lot of commands extends one main command
    /// where some things need to be initialized based on the input arguments and options.
    pub fn initialize(
        &mut self,
        _input: &mut dyn InputInterface,
        _output: &mut dyn OutputInterface,
    ) {
    }

    /// Runs the command.
    ///
    /// The code to execute is either defined directly with the
    /// set_code() method or by overriding the execute() method
    /// in a sub-class.
    ///
    /// Returns the command exit code.
    ///
    /// Throws ExceptionInterface when input binding fails. Bypass this by calling ignore_validation_errors().
    pub fn run(
        &mut self,
        input: &mut dyn InputInterface,
        output: &mut dyn OutputInterface,
    ) -> anyhow::Result<i64> {
        // add the application arguments and options
        self.merge_application_definition(true);

        // bind the input against the command specific arguments/options
        match input.bind(self.get_definition()) {
            Ok(()) => {}
            Err(e) => {
                if !self.ignore_validation_errors {
                    return Err(e);
                }
            }
        }

        self.initialize(input, output);

        if let Some(process_title) = &self.process_title {
            // TODO: PHP probes for cli_set_process_title / setproctitle availability.
            if shirabe_php_shim::function_exists("cli_set_process_title") {
                if !shirabe_php_shim::cli_set_process_title(process_title) {
                    if shirabe_php_shim::PHP_OS == "Darwin" {
                        output.writeln(
                            &["<comment>Running \"cli_set_process_title\" as an unprivileged user is not supported on MacOS.</comment>".to_string()],
                            output_interface::VERBOSITY_VERY_VERBOSE,
                        );
                    } else {
                        shirabe_php_shim::cli_set_process_title(process_title);
                    }
                }
            } else if shirabe_php_shim::function_exists("setproctitle") {
                shirabe_php_shim::setproctitle(process_title);
            } else if output.get_verbosity() == output_interface::VERBOSITY_VERY_VERBOSE {
                output.writeln(
                    &["<comment>Install the proctitle PECL to be able to change the process title.</comment>".to_string()],
                    output_interface::OUTPUT_NORMAL,
                );
            }
        }

        if input.is_interactive() {
            self.interact(input, output);
        }

        // The command name argument is often omitted when a command is executed directly with its run() method.
        // It would fail the validation if we didn't make sure the command argument is present,
        // since it's required by the application.
        if input.has_argument("command") && matches!(input.get_argument("command")?, PhpMixed::Null)
        {
            input.set_argument("command", PhpMixed::from(self.get_name()))?;
        }

        input.validate()?;

        let status_code: PhpMixed;
        if let Some(code) = &self.code {
            status_code = code(input, output);
        } else {
            let executed = self.execute(input, output)?;
            let executed = match executed {
                Ok(v) => v,
                Err(e) => return Err(anyhow::Error::new(e)),
            };
            status_code = PhpMixed::from(executed);
            // PHP also raises \TypeError when execute() does not return int; in this
            // strongly-typed port execute() already returns an int, so the check is moot.
        }

        // is_numeric($statusCode) ? (int) $statusCode : 0
        Ok(shirabe_php_shim::is_numeric_to_int(&status_code))
    }

    /// Adds suggestions to `suggestions` for the current completion input (e.g. option or argument).
    pub fn complete(&self, _input: &CompletionInput, _suggestions: &mut CompletionSuggestions) {}

    /// Sets the code to execute when running this command.
    ///
    /// If this method is used, it overrides the code defined
    /// in the execute() method.
    ///
    /// `$code` is a callable(InputInterface, OutputInterface).
    ///
    /// Throws InvalidArgumentException.
    pub fn set_code(
        &mut self,
        code: Box<dyn Fn(&mut dyn InputInterface, &mut dyn OutputInterface) -> PhpMixed>,
    ) -> &mut Self {
        // TODO: PHP rebinds an unbound Closure's $this to the command instance via
        // ReflectionFunction/Closure::bind. Rust closures have no `$this` rebinding;
        // the closure is stored as-is.
        self.code = Some(code);

        self
    }

    /// Merges the application definition with the command definition.
    ///
    /// This method is not part of public API and should not be used directly.
    ///
    /// `$mergeArgs` is whether to merge or not the Application definition arguments to Command definition arguments.
    pub fn merge_application_definition(&mut self, merge_args: bool) {
        let _application = match &self.application {
            None => return,
            Some(application) => application.clone(),
        };

        // TODO: InputDefinition stores options/arguments as `Rc<InputOption>` /
        // `Rc<InputArgument>` but its setters (`set_options`/`set_arguments`) take owned
        // `Vec<InputOption>` / `Vec<InputArgument>`. Merging the application and command
        // definitions therefore requires an agreed-upon ownership model for the
        // definition entries (Phase C). Left as todo!() pending that design.
        let _ = merge_args;
        todo!()
    }

    /// Sets an array of argument and option instances.
    ///
    /// `$definition` is an array of argument and option instances or a definition instance.
    pub fn set_definition(&mut self, definition: SetDefinitionArg) -> &mut Self {
        match definition {
            SetDefinitionArg::Definition(definition) => {
                self.definition = Some(definition);
            }
            SetDefinitionArg::Array(definition) => {
                let _ = self.definition.as_mut().unwrap().set_definition(definition);
            }
        }

        self.full_definition = None;

        self
    }

    /// Gets the InputDefinition attached to this Command.
    pub fn get_definition(&self) -> &InputDefinition {
        match &self.full_definition {
            Some(full_definition) => full_definition,
            None => self.get_native_definition(),
        }
    }

    /// Gets the InputDefinition to be used to create representations of this Command.
    ///
    /// Can be overridden to provide the original command representation when it would otherwise
    /// be changed by merging with the application InputDefinition.
    ///
    /// This method is not part of public API and should not be used directly.
    pub fn get_native_definition(&self) -> &InputDefinition {
        match &self.definition {
            None => {
                // TODO(review): PHP throws LogicException here, but get_native_definition()
                // returns InputDefinition (no Result). In this port `definition` is set in
                // the constructor, so None should not occur; treated as a programming error.
                panic!(
                    "Command class is not correctly initialized. You probably forgot to call the parent constructor."
                );
            }
            Some(definition) => definition,
        }
    }

    /// Adds an argument.
    ///
    /// `$mode` is the argument mode: InputArgument::REQUIRED or InputArgument::OPTIONAL.
    /// `$default` is the default value (for InputArgument::OPTIONAL mode only).
    ///
    /// Throws InvalidArgumentException when argument mode is not valid.
    pub fn add_argument(
        &mut self,
        name: &str,
        mode: Option<i64>,
        description: &str,
        default: PhpMixed,
    ) -> anyhow::Result<&mut Self> {
        self.definition
            .as_mut()
            .unwrap()
            .add_argument(InputArgument::new(
                name.to_string(),
                mode,
                description.to_string(),
                default.clone(),
            )?)?;
        if self.full_definition.is_some() {
            self.full_definition
                .as_mut()
                .unwrap()
                .add_argument(InputArgument::new(
                    name.to_string(),
                    mode,
                    description.to_string(),
                    default,
                )?)?;
        }

        Ok(self)
    }

    /// Adds an option.
    ///
    /// `$shortcut` is the shortcuts, can be null, a string of shortcuts delimited by | or an array of shortcuts.
    /// `$mode` is the option mode: One of the InputOption::VALUE_* constants.
    /// `$default` is the default value (must be null for InputOption::VALUE_NONE).
    ///
    /// Throws InvalidArgumentException if option mode is invalid or incompatible.
    pub fn add_option(
        &mut self,
        name: &str,
        shortcut: PhpMixed,
        mode: Option<i64>,
        description: &str,
        default: PhpMixed,
    ) -> anyhow::Result<&mut Self> {
        self.definition
            .as_mut()
            .unwrap()
            .add_option(InputOption::new(
                name,
                shortcut.clone(),
                mode,
                description.to_string(),
                default.clone(),
            )?)?;
        if self.full_definition.is_some() {
            self.full_definition
                .as_mut()
                .unwrap()
                .add_option(InputOption::new(
                    name,
                    shortcut,
                    mode,
                    description.to_string(),
                    default,
                )?)?;
        }

        Ok(self)
    }

    /// Sets the name of the command.
    ///
    /// This method can set both the namespace and the name if
    /// you separate them by a colon (:)
    ///
    ///     command.set_name("foo:bar");
    ///
    /// Throws InvalidArgumentException when the name is invalid.
    pub fn set_name(&mut self, name: &str) -> anyhow::Result<&mut Self> {
        if let Err(e) = self.validate_name(name)? {
            return Err(e.into());
        }

        self.name = Some(name.to_string());

        Ok(self)
    }

    /// Sets the process title of the command.
    ///
    /// This feature should be used only when creating a long process command,
    /// like a daemon.
    pub fn set_process_title(&mut self, title: &str) -> &mut Self {
        self.process_title = Some(title.to_string());

        self
    }

    /// Returns the command name.
    pub fn get_name(&self) -> Option<String> {
        self.name.clone()
    }

    /// `$hidden` is whether or not the command should be hidden from the list of commands.
    pub fn set_hidden(&mut self, hidden: bool) -> &mut Self {
        self.hidden = hidden;

        self
    }

    /// Returns whether the command should be publicly shown or not.
    pub fn is_hidden(&self) -> bool {
        self.hidden
    }

    /// Sets the description for the command.
    pub fn set_description(&mut self, description: &str) -> &mut Self {
        self.description = description.to_string();

        self
    }

    /// Returns the description for the command.
    pub fn get_description(&self) -> String {
        self.description.clone()
    }

    /// Sets the help for the command.
    pub fn set_help(&mut self, help: &str) -> &mut Self {
        self.help = help.to_string();

        self
    }

    /// Returns the help for the command.
    pub fn get_help(&self) -> String {
        self.help.clone()
    }

    /// Returns the processed help for the command replacing the %command.name% and
    /// %command.full_name% patterns with the real values dynamically.
    pub fn get_processed_help(&self) -> String {
        let name = self.name.clone();
        let is_single_command = match &self.application {
            Some(application) => application.borrow().is_single_command(),
            None => false,
        };

        let placeholders = [
            "%command.name%".to_string(),
            "%command.full_name%".to_string(),
        ];
        let php_self = shirabe_php_shim::server("PHP_SELF");
        let replacements = [
            name.clone().unwrap_or_default(),
            if is_single_command {
                php_self.clone()
            } else {
                format!("{} {}", php_self, name.unwrap_or_default())
            },
        ];

        let help = self.get_help();
        let subject = if help.is_empty() {
            self.get_description()
        } else {
            help
        };

        shirabe_php_shim::str_replace_array(&placeholders, &replacements, &subject)
    }

    /// Sets the aliases for the command.
    ///
    /// `$aliases` is an array of aliases for the command.
    ///
    /// Throws InvalidArgumentException when an alias is invalid.
    pub fn set_aliases(&mut self, aliases: Vec<String>) -> anyhow::Result<&mut Self> {
        let mut list = Vec::new();

        for alias in &aliases {
            if let Err(e) = self.validate_name(alias)? {
                return Err(e.into());
            }
            list.push(alias.clone());
        }

        // PHP: `\is_array($aliases) ? $aliases : $list`. Here `aliases` is always an
        // array (Vec), so the result is `aliases`; `list` mirrors the validation loop.
        self.aliases = aliases;

        Ok(self)
    }

    /// Returns the aliases for the command.
    pub fn get_aliases(&self) -> Vec<String> {
        self.aliases.clone()
    }

    /// Returns the synopsis for the command.
    ///
    /// `$short` is whether to show the short version of the synopsis (with options folded) or not.
    pub fn get_synopsis(&mut self, short: bool) -> String {
        let key = if short { "short" } else { "long" }.to_string();

        if !self.synopsis.contains_key(&key) {
            let value = format!(
                "{} {}",
                self.name.clone().unwrap_or_default(),
                self.definition.as_ref().unwrap().get_synopsis(short)
            )
            .trim()
            .to_string();
            self.synopsis.insert(key.clone(), value);
        }

        self.synopsis[&key].clone()
    }

    /// Add a command usage example, it'll be prefixed with the command name.
    pub fn add_usage(&mut self, usage: &str) -> &mut Self {
        let mut usage = usage.to_string();
        let name = self.name.clone().unwrap_or_default();
        if !usage.starts_with(&name) {
            usage = format!("{} {}", name, usage);
        }

        self.usages.push(usage);

        self
    }

    /// Returns alternative usages of the command.
    pub fn get_usages(&self) -> Vec<String> {
        self.usages.clone()
    }

    /// Gets a helper instance by name.
    ///
    /// Throws LogicException if no HelperSet is defined.
    /// Throws InvalidArgumentException if the helper is not defined.
    pub fn get_helper(&self, name: &str) -> anyhow::Result<Result<PhpMixed, LogicException>> {
        let helper_set = match &self.helper_set {
            None => {
                return Ok(Err(LogicException(shirabe_php_shim::LogicException {
                    message: format!(
                        "Cannot retrieve helper \"{}\" because there is no HelperSet defined. Did you forget to add your command to the application or to set the application on the command using the setApplication() method? You can also set the HelperSet directly using the setHelperSet() method.",
                        name
                    ),
                    code: 0,
                })));
            }
            Some(helper_set) => helper_set,
        };

        // TODO(review): HelperSet::get() returns `Rc<RefCell<dyn HelperInterface>>`, but
        // Command::getHelper() is typed `mixed` (PhpMixed) here and PhpMixed cannot hold a
        // helper instance. The helper return modelling needs a dedicated type (Phase C).
        let _ = helper_set;
        todo!()
    }

    /// Validates a command name.
    ///
    /// It must be non-empty and parts can optionally be separated by ":".
    ///
    /// Throws InvalidArgumentException when the name is invalid.
    fn validate_name(&self, name: &str) -> anyhow::Result<Result<(), InvalidArgumentException>> {
        let mut matches: Vec<Option<String>> = Vec::new();
        if !shirabe_php_shim::preg_match(r"/^[^\:]++(\:[^\:]++)*$/", name, &mut matches) {
            return Ok(Err(InvalidArgumentException(
                shirabe_php_shim::InvalidArgumentException {
                    message: format!("Command name \"{}\" is invalid.", name),
                    code: 0,
                },
            )));
        }

        Ok(Ok(()))
    }
}

/// The argument of Command::set_definition(), which accepts either an array of
/// argument/option instances or an InputDefinition.
#[derive(Debug)]
pub enum SetDefinitionArg {
    Array(Vec<crate::symfony::console::input::input_definition::DefinitionItem>),
    Definition(InputDefinition),
}

/// Polymorphic interface for all commands (PHP's `Command` base class as seen by
/// callers that hold a command of unknown concrete type).
///
/// Phase B: default methods are `todo!()`; the concrete behavior lives on
/// `BaseCommand`'s inherent methods. Object-safe so `dyn Command` works.
pub trait Command: std::fmt::Debug + shirabe_php_shim::AsAny {
    fn clone_box(&self) -> Box<dyn Command> {
        todo!()
    }

    fn configure(&mut self) {
        todo!()
    }

    fn run(
        &mut self,
        _input: &mut dyn InputInterface,
        _output: &mut dyn OutputInterface,
    ) -> anyhow::Result<i64> {
        todo!()
    }

    fn complete(&self, _input: &CompletionInput, _suggestions: &mut CompletionSuggestions) {
        todo!()
    }

    fn is_enabled(&self) -> bool {
        todo!()
    }

    fn set_application(&mut self, _application: Option<Rc<RefCell<dyn Application>>>) {
        todo!()
    }

    fn get_application(&self) -> Option<Rc<RefCell<dyn Application>>> {
        todo!()
    }

    fn set_helper_set(&mut self, _helper_set: Rc<RefCell<HelperSet>>) {
        todo!()
    }

    fn get_helper_set(&self) -> Option<Rc<RefCell<HelperSet>>> {
        todo!()
    }

    fn merge_application_definition(&mut self, _merge_args: bool) {
        todo!()
    }

    fn get_definition(&self) -> &InputDefinition {
        todo!()
    }

    fn get_native_definition(&self) -> &InputDefinition {
        todo!()
    }

    fn set_name(&mut self, _name: &str) -> anyhow::Result<()> {
        todo!()
    }

    fn get_name(&self) -> Option<String> {
        todo!()
    }

    fn set_hidden(&mut self, _hidden: bool) {
        todo!()
    }

    fn is_hidden(&self) -> bool {
        todo!()
    }

    fn set_description(&mut self, _description: &str) {
        todo!()
    }

    fn get_description(&self) -> String {
        todo!()
    }

    fn set_help(&mut self, _help: &str) {
        todo!()
    }

    fn get_help(&self) -> String {
        todo!()
    }

    fn get_processed_help(&self) -> String {
        todo!()
    }

    fn set_aliases(&mut self, _aliases: Vec<String>) -> anyhow::Result<()> {
        todo!()
    }

    fn get_aliases(&self) -> Vec<String> {
        todo!()
    }

    fn get_synopsis(&mut self, _short: bool) -> String {
        todo!()
    }

    fn get_usages(&self) -> Vec<String> {
        todo!()
    }

    fn get_helper(&self, _name: &str) -> anyhow::Result<Result<PhpMixed, LogicException>> {
        todo!()
    }

    fn ignore_validation_errors(&mut self) {
        todo!()
    }
}

impl Command for BaseCommand {}

impl std::fmt::Debug for BaseCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BaseCommand")
            .field("name", &self.name)
            .field("aliases", &self.aliases)
            .field("hidden", &self.hidden)
            .field("description", &self.description)
            .finish_non_exhaustive()
    }
}
