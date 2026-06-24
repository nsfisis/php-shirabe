//! ref: composer/vendor/symfony/console/Command/Command.php

use crate::symfony::console::application::Application;
use crate::symfony::console::completion::completion_input::CompletionInput;
use crate::symfony::console::completion::completion_suggestions::CompletionSuggestions;
use crate::symfony::console::exception::invalid_argument_exception::InvalidArgumentException;
use crate::symfony::console::helper::helper_set::HelperSet;
use crate::symfony::console::input::input_argument::InputArgument;
use crate::symfony::console::input::input_definition::InputDefinition;
use crate::symfony::console::input::input_interface::InputInterface;
use crate::symfony::console::input::input_option::InputOption;
use crate::symfony::console::output::output_interface::{self, OutputInterface};
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;
use std::cell::{Cell, Ref, RefCell};
use std::rc::Rc;

/// The base-class state of the PHP `Command` class.
///
/// The PHP `Command` class is split into the polymorphic [`Command`] trait (the
/// methods callers invoke on a command of unknown concrete type) and this struct,
/// which holds the base-class fields and provides their canonical behavior via
/// `impl Command for CommandData`. Subclasses embed a `CommandData` (directly, or
/// transitively through `BaseCommandData`) and forward the state methods to it.
///
/// The mutable fields use interior mutability (`Cell`/`RefCell`) so that the `Command`
/// trait methods take `&self`, mirroring PHP's reference semantics: calling a method on a
/// command does not lock the object, so a command can be re-entered (e.g. the help command
/// describing itself) without the borrow conflicts a `&mut self` design would cause.
pub struct CommandData {
    application: RefCell<Option<Rc<RefCell<dyn Application>>>>,
    name: RefCell<Option<String>>,
    process_title: RefCell<Option<String>>,
    aliases: RefCell<Vec<String>>,
    definition: RefCell<Option<InputDefinition>>,
    hidden: Cell<bool>,
    help: RefCell<String>,
    description: RefCell<String>,
    full_definition: RefCell<Option<InputDefinition>>,
    ignore_validation_errors: Cell<bool>,
    // A callable(InputInterface, OutputInterface) -> i64.
    code:
        RefCell<Option<Box<dyn Fn(&mut dyn InputInterface, &mut dyn OutputInterface) -> PhpMixed>>>,
    synopsis: RefCell<IndexMap<String, String>>,
    usages: RefCell<Vec<String>>,
    helper_set: RefCell<Option<Rc<RefCell<HelperSet>>>>,
}

impl CommandData {
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

    /// Builds the base-class state. `name` is the name of the command; passing None
    /// means it must be set in the subclass `configure()`.
    ///
    /// Unlike PHP's `__construct`, this does not call `configure()` — the concrete
    /// command's `new()` calls `configure()` after embedding the data, mirroring the
    /// virtual dispatch of `$this->configure()` from the parent constructor.
    pub fn new(name: Option<String>) -> Self {
        let this = CommandData {
            application: RefCell::new(None),
            name: RefCell::new(None),
            process_title: RefCell::new(None),
            aliases: RefCell::new(Vec::new()),
            definition: RefCell::new(Some(
                InputDefinition::new(Vec::new()).expect("an empty InputDefinition cannot fail"),
            )),
            hidden: Cell::new(false),
            help: RefCell::new(String::new()),
            description: RefCell::new(String::new()),
            full_definition: RefCell::new(None),
            ignore_validation_errors: Cell::new(false),
            code: RefCell::new(None),
            synopsis: RefCell::new(IndexMap::new()),
            usages: RefCell::new(Vec::new()),
            helper_set: RefCell::new(None),
        };

        // PHP's __construct also derives the name from getDefaultName() when null and
        // sets the default description; both rely on Reflection late-static-binding
        // (get_default_name/get_default_description are todo!()), and concrete commands
        // always set their name in configure(), so only an explicit name is honored here.
        if let Some(name) = name {
            *this.name.borrow_mut() = Some(name);
        }

        this
    }

    /// Applies a `$defaultName`-style name (PHP `Command::__construct` when `$name` is null and a
    /// `static $defaultName` exists). The string is `|`-separated; a leading empty segment marks the
    /// command hidden, the next segment is the name, and the rest are aliases.
    pub fn apply_default_name(&self, default_name: &str) -> anyhow::Result<()> {
        let mut aliases: Vec<String> = default_name.split('|').map(|s| s.to_string()).collect();
        let mut name = aliases.remove(0);
        if name.is_empty() {
            self.set_hidden(true);
            name = if aliases.is_empty() {
                String::new()
            } else {
                aliases.remove(0)
            };
        }
        self.set_name(&name)?;
        self.set_aliases(aliases)?;
        Ok(())
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

    /// Sets an array of argument and option instances (the Symfony-typed entry point;
    /// `BaseCommand::set_definition` adapts the Composer-typed arguments to this).
    pub fn set_definition(&self, definition: SetDefinitionArg) -> &Self {
        match definition {
            SetDefinitionArg::Definition(definition) => {
                *self.definition.borrow_mut() = Some(definition);
            }
            SetDefinitionArg::Array(definition) => {
                let _ = self
                    .definition
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .set_definition(definition);
            }
        }

        *self.full_definition.borrow_mut() = None;

        self
    }

    /// Adds an argument (Symfony-typed entry point).
    ///
    /// Throws InvalidArgumentException when argument mode is not valid.
    pub fn add_argument(
        &self,
        name: &str,
        mode: Option<i64>,
        description: &str,
        default: PhpMixed,
    ) -> anyhow::Result<&Self> {
        self.definition
            .borrow_mut()
            .as_mut()
            .unwrap()
            .add_argument(InputArgument::new(
                name.to_string(),
                mode,
                description.to_string(),
                default.clone(),
            )?)?;
        if let Some(full_definition) = self.full_definition.borrow_mut().as_mut() {
            full_definition.add_argument(InputArgument::new(
                name.to_string(),
                mode,
                description.to_string(),
                default,
            )?)?;
        }

        Ok(self)
    }

    /// Adds an option (Symfony-typed entry point).
    ///
    /// Throws InvalidArgumentException if option mode is invalid or incompatible.
    pub fn add_option(
        &self,
        name: &str,
        shortcut: PhpMixed,
        mode: Option<i64>,
        description: &str,
        default: PhpMixed,
    ) -> anyhow::Result<&Self> {
        self.definition
            .borrow_mut()
            .as_mut()
            .unwrap()
            .add_option(InputOption::new(
                name,
                shortcut.clone(),
                mode,
                description.to_string(),
                default.clone(),
            )?)?;
        if let Some(full_definition) = self.full_definition.borrow_mut().as_mut() {
            full_definition.add_option(InputOption::new(
                name,
                shortcut,
                mode,
                description.to_string(),
                default,
            )?)?;
        }

        Ok(self)
    }
}

/// The argument of `CommandData::set_definition()`, which accepts either an array of
/// argument/option instances or an InputDefinition.
#[derive(Debug)]
pub enum SetDefinitionArg {
    Array(Vec<crate::symfony::console::input::input_definition::DefinitionItem>),
    Definition(InputDefinition),
}

/// Forwards a single trait method to an embedded field that already implements the
/// method (the "inner" command-state holder).
///
/// Each `Command`/`BaseCommand` implementer spells out the methods it delegates, one
/// `delegate_to_inner!` per method, alongside the few methods it overrides by hand.
/// The first argument names the field to forward to; the second is the method's
/// signature. Every method takes `&self` (the command state is interior-mutable);
/// fluent setters returning `&Self` (optionally wrapped in `anyhow::Result`) are handled
/// specially so the returned reference is re-rooted at the outer `self` rather than the
/// inner field.
#[macro_export]
macro_rules! delegate_to_inner {
    // fluent fallible: -> anyhow::Result<&Self>
    ($field:ident, fn $name:ident(&self $(, $arg:ident : $ty:ty )* $(,)?) -> anyhow::Result<&Self>) => {
        fn $name(&self $(, $arg: $ty)*) -> anyhow::Result<&Self> {
            self.$field.$name($($arg),*)?;
            Ok(self)
        }
    };
    // fluent infallible: -> &Self
    ($field:ident, fn $name:ident(&self $(, $arg:ident : $ty:ty )* $(,)?) -> &Self) => {
        fn $name(&self $(, $arg: $ty)*) -> &Self {
            self.$field.$name($($arg),*);
            self
        }
    };
    // &self with a return type
    ($field:ident, fn $name:ident(&self $(, $arg:ident : $ty:ty )* $(,)?) -> $ret:ty) => {
        fn $name(&self $(, $arg: $ty)*) -> $ret {
            self.$field.$name($($arg),*)
        }
    };
    // &self without a return type
    ($field:ident, fn $name:ident(&self $(, $arg:ident : $ty:ty )* $(,)?)) => {
        fn $name(&self $(, $arg: $ty)*) {
            self.$field.$name($($arg),*)
        }
    };
}

/// Forwards every `Command` state method (the setters/getters whose canonical impl lives on
/// `CommandData` and which no subclass overrides) to an embedded field. Each command invokes
/// this once inside its `impl Command` block and spells out by hand only the behavior hooks it
/// overrides (`configure`/`execute`/`initialize`/...). The single argument names the field to
/// forward to (`inner` for Symfony commands, `base_command_data` for Composer commands).
#[macro_export]
macro_rules! delegate_command_trait_impls_to_inner {
    ($field:ident) => {
        $crate::delegate_to_inner!($field, fn is_enabled(&self) -> bool);
        $crate::delegate_to_inner!($field, fn set_application(&self, application: Option<std::rc::Rc<std::cell::RefCell<dyn $crate::symfony::console::application::Application>>>));
        $crate::delegate_to_inner!($field, fn get_application(&self) -> Option<std::rc::Rc<std::cell::RefCell<dyn $crate::symfony::console::application::Application>>>);
        $crate::delegate_to_inner!($field, fn set_helper_set(&self, helper_set: std::rc::Rc<std::cell::RefCell<$crate::symfony::console::helper::helper_set::HelperSet>>));
        $crate::delegate_to_inner!($field, fn get_helper_set(&self) -> Option<std::rc::Rc<std::cell::RefCell<$crate::symfony::console::helper::helper_set::HelperSet>>>);
        $crate::delegate_to_inner!($field, fn merge_application_definition(&self, merge_args: bool));
        $crate::delegate_to_inner!($field, fn get_definition(&self) -> std::cell::Ref<'_, $crate::symfony::console::input::input_definition::InputDefinition>);
        $crate::delegate_to_inner!($field, fn get_native_definition(&self) -> std::cell::Ref<'_, $crate::symfony::console::input::input_definition::InputDefinition>);
        $crate::delegate_to_inner!($field, fn set_name(&self, name: &str) -> anyhow::Result<()>);
        $crate::delegate_to_inner!($field, fn get_name(&self) -> Option<String>);
        $crate::delegate_to_inner!($field, fn set_process_title(&self, title: &str));
        $crate::delegate_to_inner!($field, fn get_process_title(&self) -> Option<String>);
        $crate::delegate_to_inner!($field, fn set_hidden(&self, hidden: bool));
        $crate::delegate_to_inner!($field, fn is_hidden(&self) -> bool);
        $crate::delegate_to_inner!($field, fn set_description(&self, description: &str));
        $crate::delegate_to_inner!($field, fn get_description(&self) -> String);
        $crate::delegate_to_inner!($field, fn set_help(&self, help: &str));
        $crate::delegate_to_inner!($field, fn get_help(&self) -> String);
        $crate::delegate_to_inner!($field, fn get_processed_help(&self) -> String);
        $crate::delegate_to_inner!($field, fn set_aliases(&self, aliases: Vec<String>) -> anyhow::Result<()>);
        $crate::delegate_to_inner!($field, fn get_aliases(&self) -> Vec<String>);
        $crate::delegate_to_inner!($field, fn get_synopsis(&self, short: bool) -> String);
        $crate::delegate_to_inner!($field, fn add_usage(&self, usage: &str));
        $crate::delegate_to_inner!($field, fn get_usages(&self) -> Vec<String>);
        $crate::delegate_to_inner!($field, fn get_helper(&self, name: &str) -> anyhow::Result<Result<shirabe_php_shim::PhpMixed, $crate::symfony::console::exception::logic_exception::LogicException>>);
        $crate::delegate_to_inner!($field, fn set_code(&self, code: Box<dyn Fn(&mut dyn $crate::symfony::console::input::InputInterface, &mut dyn $crate::symfony::console::output::OutputInterface) -> shirabe_php_shim::PhpMixed>));
        $crate::delegate_to_inner!($field, fn get_code(&self) -> std::cell::Ref<'_, Option<Box<dyn Fn(&mut dyn $crate::symfony::console::input::InputInterface, &mut dyn $crate::symfony::console::output::OutputInterface) -> shirabe_php_shim::PhpMixed>>>);
        $crate::delegate_to_inner!($field, fn ignore_validation_errors(&self));
        $crate::delegate_to_inner!($field, fn get_ignore_validation_errors(&self) -> bool);
    };
}

/// Polymorphic interface for all commands (PHP's `Command` base class as seen by
/// callers that hold a command of unknown concrete type).
///
/// The canonical behavior lives in `impl Command for CommandData`; subclasses forward
/// the state methods there and override the behavior hooks (`configure`/`execute`/...).
/// Object-safe so `dyn Command` works. All methods take `&self`; the command's mutable
/// state is interior-mutable (see [`CommandData`]).
pub trait Command: std::fmt::Debug + shirabe_php_shim::AsAny {
    /// Configures the current command.
    fn configure(&self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Executes the current command, returning 0 or an exit code.
    ///
    /// Concrete commands override this; reaching the default means a command class
    /// forgot to implement it (PHP throws LogicException — a programming error here).
    fn execute(
        &self,
        _input: Rc<RefCell<dyn InputInterface>>,
        _output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        panic!("You must override the execute() method in the concrete command class.");
    }

    /// Interacts with the user before the InputDefinition is validated.
    fn interact(
        &self,
        _input: Rc<RefCell<dyn InputInterface>>,
        _output: Rc<RefCell<dyn OutputInterface>>,
    ) {
    }

    /// Initializes the command after the input has been bound and before it is validated.
    fn initialize(
        &self,
        _input: Rc<RefCell<dyn InputInterface>>,
        _output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    /// Adds suggestions to `suggestions` for the current completion input.
    fn complete(&self, _input: &CompletionInput, _suggestions: &mut CompletionSuggestions) {}

    /// Whether this command proxies to another application/command (Composer's
    /// `BaseCommand::isProxyCommand`). Exposed here so the `dyn Command` registry can detect proxy
    /// commands without downcasting to the Composer `BaseCommand` trait; defaults to `false` and is
    /// overridden by Composer proxy commands such as `GlobalCommand`.
    fn is_proxy_command(&self) -> bool {
        false
    }

    /// Runs the command.
    ///
    /// Template method: it calls `self.initialize()`, `self.interact()` and
    /// `self.execute()`, which dispatch to the concrete command's overrides. It must
    /// not be overridden (except by proxy commands like `GlobalCommand`) nor delegated,
    /// or that late binding breaks.
    fn run(
        &self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        self.base_run(input, output)
    }

    /// The base-class (`Command`) body of `run`, as PHP's `Command::run`. Proxy commands such as
    /// `GlobalCommand` override `run` but still call `base_run` to delegate to the base behavior,
    /// matching PHP's `parent::run($input, $output)`. It must not be overridden, or the late
    /// binding of `initialize`/`interact`/`execute` to the concrete command breaks.
    fn base_run(
        &self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        // add the application arguments and options
        self.merge_application_definition(true);

        // bind the input against the command specific arguments/options
        match input.borrow_mut().bind(&*self.get_definition()) {
            Ok(()) => {}
            Err(e) => {
                if !self.get_ignore_validation_errors() {
                    return Err(e);
                }
            }
        }

        self.initialize(input.clone(), output.clone())?;

        if let Some(process_title) = self.get_process_title() {
            // TODO: PHP probes for cli_set_process_title / setproctitle availability.
            if shirabe_php_shim::function_exists("cli_set_process_title") {
                if !shirabe_php_shim::cli_set_process_title(&process_title) {
                    if shirabe_php_shim::PHP_OS == "Darwin" {
                        output.borrow_mut().writeln(
                            &["<comment>Running \"cli_set_process_title\" as an unprivileged user is not supported on MacOS.</comment>".to_string()],
                            output_interface::VERBOSITY_VERY_VERBOSE,
                        );
                    } else {
                        shirabe_php_shim::cli_set_process_title(&process_title);
                    }
                }
            } else if shirabe_php_shim::function_exists("setproctitle") {
                shirabe_php_shim::setproctitle(&process_title);
            } else if output.borrow().get_verbosity() == output_interface::VERBOSITY_VERY_VERBOSE {
                output.borrow_mut().writeln(
                    &["<comment>Install the proctitle PECL to be able to change the process title.</comment>".to_string()],
                    output_interface::OUTPUT_NORMAL,
                );
            }
        }

        if input.borrow().is_interactive() {
            self.interact(input.clone(), output.clone());
        }

        // The command name argument is often omitted when a command is executed directly with its run() method.
        // It would fail the validation if we didn't make sure the command argument is present,
        // since it's required by the application.
        if input.borrow().has_argument("command")
            && matches!(input.borrow().get_argument("command")?, PhpMixed::Null)
        {
            let name = self.get_name();
            input
                .borrow_mut()
                .set_argument("command", PhpMixed::from(name))?;
        }

        input.borrow_mut().validate()?;

        let status_code: PhpMixed;
        if self.get_code().is_some() {
            let code = self.get_code();
            let code = code.as_ref().unwrap();
            status_code = code(&mut *input.borrow_mut(), &mut *output.borrow_mut());
        } else {
            let executed = self.execute(input.clone(), output.clone())?;
            status_code = PhpMixed::from(executed);
            // PHP also raises \TypeError when execute() does not return int; in this
            // strongly-typed port execute() already returns an int, so the check is moot.
        }

        // is_numeric($statusCode) ? (int) $statusCode : 0
        Ok(shirabe_php_shim::is_numeric_to_int(&status_code))
    }

    // --- state methods (canonical impl on `CommandData`; subclasses forward there) ---

    fn is_enabled(&self) -> bool;

    fn set_application(&self, application: Option<Rc<RefCell<dyn Application>>>);

    fn get_application(&self) -> Option<Rc<RefCell<dyn Application>>>;

    fn set_helper_set(&self, helper_set: Rc<RefCell<HelperSet>>);

    fn get_helper_set(&self) -> Option<Rc<RefCell<HelperSet>>>;

    fn merge_application_definition(&self, merge_args: bool);

    fn get_definition(&self) -> Ref<'_, InputDefinition>;

    fn get_native_definition(&self) -> Ref<'_, InputDefinition>;

    fn set_name(&self, name: &str) -> anyhow::Result<()>;

    fn get_name(&self) -> Option<String>;

    fn set_process_title(&self, title: &str);

    fn get_process_title(&self) -> Option<String>;

    fn set_hidden(&self, hidden: bool);

    fn is_hidden(&self) -> bool;

    fn set_description(&self, description: &str);

    fn get_description(&self) -> String;

    fn set_help(&self, help: &str);

    fn get_help(&self) -> String;

    fn get_processed_help(&self) -> String;

    fn set_aliases(&self, aliases: Vec<String>) -> anyhow::Result<()>;

    fn get_aliases(&self) -> Vec<String>;

    fn get_synopsis(&self, short: bool) -> String;

    fn add_usage(&self, usage: &str);

    fn get_usages(&self) -> Vec<String>;

    fn get_helper(
        &self,
        name: &str,
    ) -> anyhow::Result<
        Result<PhpMixed, crate::symfony::console::exception::logic_exception::LogicException>,
    >;

    fn set_code(
        &self,
        code: Box<dyn Fn(&mut dyn InputInterface, &mut dyn OutputInterface) -> PhpMixed>,
    );

    fn get_code(
        &self,
    ) -> Ref<'_, Option<Box<dyn Fn(&mut dyn InputInterface, &mut dyn OutputInterface) -> PhpMixed>>>;

    fn ignore_validation_errors(&self);

    fn get_ignore_validation_errors(&self) -> bool;
}

impl Command for CommandData {
    fn is_enabled(&self) -> bool {
        true
    }

    fn set_application(&self, application: Option<Rc<RefCell<dyn Application>>>) {
        *self.application.borrow_mut() = application.clone();
        if let Some(application) = application {
            self.set_helper_set(application.borrow_mut().get_helper_set());
        } else {
            *self.helper_set.borrow_mut() = None;
        }

        *self.full_definition.borrow_mut() = None;
    }

    fn get_application(&self) -> Option<Rc<RefCell<dyn Application>>> {
        self.application.borrow().clone()
    }

    fn set_helper_set(&self, helper_set: Rc<RefCell<HelperSet>>) {
        *self.helper_set.borrow_mut() = Some(helper_set);
    }

    fn get_helper_set(&self) -> Option<Rc<RefCell<HelperSet>>> {
        self.helper_set.borrow().clone()
    }

    /// Merges the application definition with the command definition.
    fn merge_application_definition(&self, merge_args: bool) {
        let application = match &*self.application.borrow() {
            None => return,
            Some(application) => application.clone(),
        };

        // InputDefinition stores its entries as `Rc<InputArgument>` / `Rc<InputOption>` while the
        // setters take owned values, so the shared entries are cloned out (both types derive Clone).
        let app_definition = application.borrow_mut().get_definition();

        let mut full_definition =
            InputDefinition::new(Vec::new()).expect("an empty InputDefinition cannot fail");

        let own_options: Vec<InputOption> = self
            .definition
            .borrow()
            .as_ref()
            .unwrap()
            .get_options()
            .values()
            .map(|option| (**option).clone())
            .collect();
        full_definition
            .set_options(own_options)
            .expect("the command's own options are already valid");

        let app_options: Vec<InputOption> = app_definition
            .borrow()
            .get_options()
            .values()
            .map(|option| (**option).clone())
            .collect();
        full_definition
            .add_options(app_options)
            .expect("merging the application options cannot conflict here");

        if merge_args {
            let app_arguments: Vec<InputArgument> = app_definition
                .borrow()
                .get_arguments()
                .values()
                .map(|argument| (**argument).clone())
                .collect();
            full_definition
                .set_arguments(app_arguments)
                .expect("the application arguments are already valid");

            let own_arguments: Vec<InputArgument> = self
                .definition
                .borrow()
                .as_ref()
                .unwrap()
                .get_arguments()
                .values()
                .map(|argument| (**argument).clone())
                .collect();
            full_definition
                .add_arguments(Some(own_arguments))
                .expect("merging the command's own arguments cannot conflict here");
        } else {
            let own_arguments: Vec<InputArgument> = self
                .definition
                .borrow()
                .as_ref()
                .unwrap()
                .get_arguments()
                .values()
                .map(|argument| (**argument).clone())
                .collect();
            full_definition
                .set_arguments(own_arguments)
                .expect("the command's own arguments are already valid");
        }

        *self.full_definition.borrow_mut() = Some(full_definition);
    }

    fn get_definition(&self) -> Ref<'_, InputDefinition> {
        if self.full_definition.borrow().is_some() {
            Ref::map(self.full_definition.borrow(), |full_definition| {
                full_definition.as_ref().unwrap()
            })
        } else {
            self.get_native_definition()
        }
    }

    fn get_native_definition(&self) -> Ref<'_, InputDefinition> {
        Ref::map(self.definition.borrow(), |definition| match definition {
            Some(definition) => definition,
            None => {
                // PHP throws LogicException; `definition` is set in `new()`, so None is a
                // programming error (forgot to call the parent constructor).
                panic!(
                    "Command class is not correctly initialized. You probably forgot to call the parent constructor."
                );
            }
        })
    }

    fn set_name(&self, name: &str) -> anyhow::Result<()> {
        if let Err(e) = self.validate_name(name)? {
            return Err(e.into());
        }

        *self.name.borrow_mut() = Some(name.to_string());

        Ok(())
    }

    fn get_name(&self) -> Option<String> {
        self.name.borrow().clone()
    }

    fn set_process_title(&self, title: &str) {
        *self.process_title.borrow_mut() = Some(title.to_string());
    }

    fn get_process_title(&self) -> Option<String> {
        self.process_title.borrow().clone()
    }

    fn set_hidden(&self, hidden: bool) {
        self.hidden.set(hidden);
    }

    fn is_hidden(&self) -> bool {
        self.hidden.get()
    }

    fn set_description(&self, description: &str) {
        *self.description.borrow_mut() = description.to_string();
    }

    fn get_description(&self) -> String {
        self.description.borrow().clone()
    }

    fn set_help(&self, help: &str) {
        *self.help.borrow_mut() = help.to_string();
    }

    fn get_help(&self) -> String {
        self.help.borrow().clone()
    }

    fn get_processed_help(&self) -> String {
        let name = self.name.borrow().clone();
        let is_single_command = match &*self.application.borrow() {
            Some(application) => application.borrow().is_single_command(),
            None => false,
        };

        let placeholders = [
            "%command.name%".to_string(),
            "%command.full_name%".to_string(),
        ];
        let php_self = shirabe_php_shim::PHP_SERVER
            .lock()
            .unwrap()
            .php_self()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
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

    fn set_aliases(&self, aliases: Vec<String>) -> anyhow::Result<()> {
        let mut list = Vec::new();

        for alias in &aliases {
            if let Err(e) = self.validate_name(alias)? {
                return Err(e.into());
            }
            list.push(alias.clone());
        }

        // PHP: `\is_array($aliases) ? $aliases : $list`. Here `aliases` is always an
        // array (Vec), so the result is `aliases`; `list` mirrors the validation loop.
        *self.aliases.borrow_mut() = aliases;

        Ok(())
    }

    fn get_aliases(&self) -> Vec<String> {
        self.aliases.borrow().clone()
    }

    fn get_synopsis(&self, short: bool) -> String {
        let key = if short { "short" } else { "long" }.to_string();

        if !self.synopsis.borrow().contains_key(&key) {
            let value = format!(
                "{} {}",
                self.name.borrow().clone().unwrap_or_default(),
                self.definition
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .get_synopsis(short)
            )
            .trim()
            .to_string();
            self.synopsis.borrow_mut().insert(key.clone(), value);
        }

        self.synopsis.borrow()[&key].clone()
    }

    fn add_usage(&self, usage: &str) {
        let mut usage = usage.to_string();
        let name = self.name.borrow().clone().unwrap_or_default();
        if !usage.starts_with(&name) {
            usage = format!("{} {}", name, usage);
        }

        self.usages.borrow_mut().push(usage);
    }

    fn get_usages(&self) -> Vec<String> {
        self.usages.borrow().clone()
    }

    fn get_helper(
        &self,
        name: &str,
    ) -> anyhow::Result<
        Result<PhpMixed, crate::symfony::console::exception::logic_exception::LogicException>,
    > {
        let helper_set_ref = self.helper_set.borrow();
        let helper_set = match &*helper_set_ref {
            None => {
                return Ok(Err(
                    crate::symfony::console::exception::logic_exception::LogicException(
                        shirabe_php_shim::LogicException {
                            message: format!(
                                "Cannot retrieve helper \"{}\" because there is no HelperSet defined. Did you forget to add your command to the application or to set the application on the command using the setApplication() method? You can also set the HelperSet directly using the setHelperSet() method.",
                                name
                            ),
                            code: 0,
                        },
                    ),
                ));
            }
            Some(helper_set) => helper_set,
        };

        // TODO(plugin): PHP's Command::getHelper($name) looks a helper up by string via
        // HelperSet::get($name). The HelperSet is now a closed set exposing only typed getters
        // (get_formatter/get_question/...), so a string-keyed lookup no longer exists. Callers
        // should use the typed getters on the HelperSet directly; restoring name-based lookup is
        // deferred until the plugin API (which is the only source of dynamically named helpers).
        let _ = helper_set;
        todo!()
    }

    fn set_code(
        &self,
        code: Box<dyn Fn(&mut dyn InputInterface, &mut dyn OutputInterface) -> PhpMixed>,
    ) {
        // TODO: PHP rebinds an unbound Closure's $this to the command instance via
        // ReflectionFunction/Closure::bind. Rust closures have no `$this` rebinding;
        // the closure is stored as-is.
        *self.code.borrow_mut() = Some(code);
    }

    fn get_code(
        &self,
    ) -> Ref<'_, Option<Box<dyn Fn(&mut dyn InputInterface, &mut dyn OutputInterface) -> PhpMixed>>>
    {
        self.code.borrow()
    }

    fn ignore_validation_errors(&self) {
        self.ignore_validation_errors.set(true);
    }

    fn get_ignore_validation_errors(&self) -> bool {
        self.ignore_validation_errors.get()
    }
}

impl std::fmt::Debug for CommandData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandData")
            .field("name", &self.name.borrow())
            .field("aliases", &self.aliases.borrow())
            .field("hidden", &self.hidden.get())
            .field("description", &self.description.borrow())
            .finish_non_exhaustive()
    }
}
