//! ref: composer/vendor/symfony/console/Command/Command.php sha1:ff5e9f0b654e88fe02d8b81db7b1c7d6f1be8330

use crate::Application;
use crate::completion::{CompletionInput, CompletionSuggestions};
use crate::exception::{InvalidArgumentException, LogicException};
use crate::helper::{HelperInterface, HelperSet};
use crate::input::{
    DefinitionItem, InputArgument, InputDefinition, InputInterface, InputInterfaceByName,
    InputOption, InputValue,
};
use crate::output::OutputInterface;
use bstr::{BStr, BString, ByteSlice as _};
use indexmap::IndexMap;
use shirabe_php_shim::{
    AsAny, PHP_SERVER, PhpClass, PhpMixed, bformat, explode, is_numeric_to_int, path_to_bytes,
    php_regex, preg_is_match, str_replace_array, trim,
};

/// The base-class state of the PHP `Command` class.
///
/// The PHP `Command` class is split into the polymorphic [`Command`] trait (the
/// methods callers invoke on a command of unknown concrete type) and this struct,
/// which holds the base-class fields and provides their canonical behavior via
/// `impl Command for CommandData`. Subclasses embed a `CommandData` (directly, or
/// transitively through `BaseCommandData`) and forward the state methods to it.
pub struct CommandData {
    application: std::cell::RefCell<Option<std::rc::Rc<std::cell::RefCell<dyn Application>>>>,
    name: std::cell::RefCell<Option<BString>>,
    aliases: std::cell::RefCell<Vec<BString>>,
    definition: std::cell::RefCell<Option<InputDefinition>>,
    hidden: std::cell::Cell<bool>,
    help: std::cell::RefCell<BString>,
    description: std::cell::RefCell<BString>,
    full_definition: std::cell::RefCell<Option<InputDefinition>>,
    ignore_validation_errors: std::cell::Cell<bool>,
    // A callable(InputInterface, OutputInterface) -> i64.
    code: std::cell::RefCell<
        Option<Box<dyn Fn(&mut dyn InputInterface, &mut dyn OutputInterface) -> PhpMixed>>,
    >,
    synopsis: std::cell::RefCell<IndexMap<BString, bstr::BString>>,
    usages: std::cell::RefCell<Vec<BString>>,
    helper_set: std::cell::RefCell<Option<std::rc::Rc<std::cell::RefCell<HelperSet>>>>,
}

impl CommandData {
    // see https://tldp.org/LDP/abs/html/exitcodes.html
    pub const SUCCESS: i64 = 0;
    pub const FAILURE: i64 = 1;
    pub const INVALID: i64 = 2;

    /// Builds the base-class state. `name` is the name of the command; passing None
    /// means it must be set in the subclass `configure()`.
    ///
    /// Unlike PHP's `__construct`, this does not call `configure()` — the concrete
    /// command's `new()` calls `configure()` after embedding the data, mirroring the
    /// virtual dispatch of `$this->configure()` from the parent constructor.
    pub fn new(name: Option<BString>) -> Self {
        let this = CommandData {
            application: std::cell::RefCell::new(None),
            name: std::cell::RefCell::new(None),
            aliases: std::cell::RefCell::new(Vec::new()),
            definition: std::cell::RefCell::new(Some(
                InputDefinition::new(Vec::new()).expect("an empty InputDefinition cannot fail"),
            )),
            hidden: std::cell::Cell::new(false),
            help: std::cell::RefCell::new(BString::default()),
            description: std::cell::RefCell::new(BString::default()),
            full_definition: std::cell::RefCell::new(None),
            ignore_validation_errors: std::cell::Cell::new(false),
            code: std::cell::RefCell::new(None),
            synopsis: std::cell::RefCell::new(IndexMap::new()),
            usages: std::cell::RefCell::new(Vec::new()),
            helper_set: std::cell::RefCell::new(None),
        };

        if let Some(name) = name {
            *this.name.borrow_mut() = Some(name);
        }

        this
    }

    /// Applies a `$defaultName`-style name (PHP `Command::__construct` when `$name` is null and a
    /// `static $defaultName` exists). The string is `|`-separated; a leading empty segment marks the
    /// command hidden, the next segment is the name, and the rest are aliases.
    pub fn apply_default_name(&self, default_name: impl AsRef<[u8]>) -> anyhow::Result<()> {
        let mut aliases = explode("|", default_name);
        let mut name = aliases.remove(0);
        if name.is_empty() {
            self.set_hidden(true);
            name = if aliases.is_empty() {
                BString::default()
            } else {
                aliases.remove(0)
            };
        }
        self.set_name(name.as_bstr())?;
        self.set_aliases(aliases)?;
        Ok(())
    }

    /// Validates a command name.
    ///
    /// It must be non-empty and parts can optionally be separated by ":".
    ///
    /// Throws InvalidArgumentException when the name is invalid.
    fn validate_name(&self, name: &BStr) -> anyhow::Result<Result<(), InvalidArgumentException>> {
        if !preg_is_match(php_regex!(r"/^[^\:]++(\:[^\:]++)*$/"), name) {
            return Ok(Err(InvalidArgumentException::new(bformat!(
                "Command name \"{name}\" is invalid."
            ))));
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
        name: impl AsRef<[u8]>,
        mode: Option<i64>,
        description: impl AsRef<[u8]>,
        default: InputValue,
    ) -> anyhow::Result<&Self> {
        let name = name.as_ref();
        let description = description.as_ref();
        self.definition
            .borrow_mut()
            .as_mut()
            .unwrap()
            .add_argument(InputArgument::new(
                name,
                mode,
                description,
                default.clone(),
            )?)?;
        if let Some(full_definition) = self.full_definition.borrow_mut().as_mut() {
            full_definition.add_argument(InputArgument::new(name, mode, description, default)?)?;
        }

        Ok(self)
    }

    /// Adds an option (Symfony-typed entry point).
    ///
    /// Throws InvalidArgumentException if option mode is invalid or incompatible.
    pub fn add_option(
        &self,
        name: impl AsRef<[u8]>,
        shortcut: Option<&bstr::BStr>,
        mode: Option<i64>,
        description: impl AsRef<[u8]>,
        default: InputValue,
    ) -> anyhow::Result<&Self> {
        let name = name.as_ref();
        let description = description.as_ref();
        self.definition
            .borrow_mut()
            .as_mut()
            .unwrap()
            .add_option(InputOption::new(
                name,
                shortcut,
                mode,
                description,
                default.clone(),
            )?)?;
        if let Some(full_definition) = self.full_definition.borrow_mut().as_mut() {
            full_definition.add_option(InputOption::new(
                name,
                shortcut,
                mode,
                description,
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
    Array(Vec<DefinitionItem>),
    Definition(InputDefinition),
}

/// Forwards a single trait method to an embedded field that already implements the
/// method (the "inner" command-state holder).
///
/// Each `Command`/`BaseCommand` implementer spells out the methods it delegates, one
/// `delegate_to_inner!` per method, alongside the few methods it overrides by hand.
/// The first argument names the field to forward to; the second is the method's
/// signature. Fluent setters returning `&Self` (optionally wrapped in `anyhow::Result`) are handled
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
        $crate::delegate_to_inner!($field, fn set_application(&self, application: Option<std::rc::Rc<std::cell::RefCell<dyn $crate::application::Application>>>));
        $crate::delegate_to_inner!($field, fn get_application(&self) -> Option<std::rc::Rc<std::cell::RefCell<dyn $crate::application::Application>>>);
        $crate::delegate_to_inner!($field, fn set_helper_set(&self, helper_set: std::rc::Rc<std::cell::RefCell<$crate::helper::HelperSet>>));
        $crate::delegate_to_inner!($field, fn get_helper_set(&self) -> Option<std::rc::Rc<std::cell::RefCell<$crate::helper::HelperSet>>>);
        $crate::delegate_to_inner!($field, fn merge_application_definition(&self, merge_args: bool));
        $crate::delegate_to_inner!($field, fn get_definition(&self) -> std::cell::Ref<'_, $crate::input::InputDefinition>);
        $crate::delegate_to_inner!($field, fn get_native_definition(&self) -> std::cell::Ref<'_, $crate::input::InputDefinition>);
        $crate::delegate_to_inner!($field, fn set_name(&self, name: &bstr::BStr) -> anyhow::Result<()>);
        $crate::delegate_to_inner!($field, fn get_name(&self) -> Option<bstr::BString>);
        $crate::delegate_to_inner!($field, fn set_hidden(&self, hidden: bool));
        $crate::delegate_to_inner!($field, fn is_hidden(&self) -> bool);
        $crate::delegate_to_inner!($field, fn set_description(&self, description: &bstr::BStr));
        $crate::delegate_to_inner!($field, fn get_description(&self) -> bstr::BString);
        $crate::delegate_to_inner!($field, fn set_help(&self, help: &bstr::BStr));
        $crate::delegate_to_inner!($field, fn get_help(&self) -> bstr::BString);
        $crate::delegate_to_inner!($field, fn get_processed_help(&self) -> bstr::BString);
        $crate::delegate_to_inner!($field, fn set_aliases(&self, aliases: Vec<bstr::BString>) -> anyhow::Result<()>);
        $crate::delegate_to_inner!($field, fn get_aliases(&self) -> Vec<bstr::BString>);
        $crate::delegate_to_inner!($field, fn get_synopsis(&self, short: bool) -> bstr::BString);
        $crate::delegate_to_inner!($field, fn add_usage(&self, usage: &bstr::BStr));
        $crate::delegate_to_inner!($field, fn get_usages(&self) -> Vec<bstr::BString>);
        $crate::delegate_to_inner!($field, fn get_helper(&self, name: &bstr::BStr) -> anyhow::Result<std::rc::Rc<std::cell::RefCell<dyn $crate::helper::HelperInterface>>>);
        $crate::delegate_to_inner!($field, fn set_code(&self, code: Box<dyn Fn(&mut dyn $crate::input::InputInterface, &mut dyn $crate::output::OutputInterface) -> shirabe_php_shim::PhpMixed>));
        $crate::delegate_to_inner!($field, fn get_code(&self) -> std::cell::Ref<'_, Option<Box<dyn Fn(&mut dyn $crate::input::InputInterface, &mut dyn $crate::output::OutputInterface) -> shirabe_php_shim::PhpMixed>>>);
        $crate::delegate_to_inner!($field, fn ignore_validation_errors(&self));
        $crate::delegate_to_inner!($field, fn get_ignore_validation_errors(&self) -> bool);
    };
}

/// Polymorphic interface for all commands (PHP's `Command` base class as seen by
/// callers that hold a command of unknown concrete type).
///
/// The canonical behavior lives in `impl Command for CommandData`; subclasses forward
/// the state methods there and override the behavior hooks (`configure`/`execute`/...).
#[async_trait::async_trait(?Send)]
pub trait Command: std::fmt::Debug + AsAny + PhpClass {
    /// Configures the current command.
    fn configure(&self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Executes the current command, returning 0 or an exit code.
    ///
    /// Concrete commands override this; reaching the default means a command class
    /// forgot to implement it (PHP throws LogicException — a programming error here).
    async fn execute(
        &self,
        _input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        _output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        panic!("You must override the execute() method in the concrete command class.");
    }

    /// Interacts with the user before the InputDefinition is validated.
    fn interact(
        &self,
        _input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        _output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) {
    }

    /// Initializes the command after the input has been bound and before it is validated.
    fn initialize(
        &self,
        _input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        _output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    /// Adds suggestions to `suggestions` for the current completion input.
    ///
    /// PHP's `complete` is `void` but can throw; errors are surfaced through `anyhow::Result`
    /// so they propagate to `CompleteCommand::execute`'s catch-all (which turns them into
    /// exit code 2), matching the PHP exception flow.
    fn complete(
        &self,
        _input: &CompletionInput,
        _suggestions: &mut CompletionSuggestions,
    ) -> anyhow::Result<()> {
        Ok(())
    }

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
    async fn run(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        self.base_run(input, output).await
    }

    /// The base-class (`Command`) body of `run`, as PHP's `Command::run`. Proxy commands such as
    /// `GlobalCommand` override `run` but still call `base_run` to delegate to the base behavior,
    /// matching PHP's `parent::run($input, $output)`. It must not be overridden, or the late
    /// binding of `initialize`/`interact`/`execute` to the concrete command breaks.
    async fn base_run(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        // add the application arguments and options
        self.merge_application_definition(true);

        // bind the input against the command specific arguments/options
        match input.borrow_mut().bind(&self.get_definition()) {
            Ok(()) => {}
            Err(e) => {
                if !self.get_ignore_validation_errors() {
                    return Err(e);
                }
            }
        }

        self.initialize(input.clone(), output.clone())?;

        if input.borrow().is_interactive() {
            self.interact(input.clone(), output.clone());
        }

        // The command name argument is often omitted when a command is executed directly with its run() method.
        // It would fail the validation if we didn't make sure the command argument is present,
        // since it's required by the application.
        if input.borrow().has_argument("command")
            && input.borrow().get_argument("command")?.is_null()
        {
            let name = self.get_name();
            input
                .borrow_mut()
                .set_argument("command", InputValue::from(name))?;
        }

        input.borrow_mut().validate()?;

        let status_code: PhpMixed = if self.get_code().is_some() {
            let code = self.get_code();
            let code = code.as_ref().unwrap();
            code(&mut *input.borrow_mut(), &mut *output.borrow_mut())
        } else {
            let executed = self.execute(input.clone(), output.clone()).await?;
            // PHP also raises \TypeError when execute() does not return int; in this
            // strongly-typed port execute() already returns an int, so the check is moot.
            PhpMixed::from(executed)
        };

        // is_numeric($statusCode) ? (int) $statusCode : 0
        Ok(is_numeric_to_int(&status_code))
    }

    // --- state methods (canonical impl on `CommandData`; subclasses forward there) ---

    fn is_enabled(&self) -> bool;

    fn set_application(
        &self,
        application: Option<std::rc::Rc<std::cell::RefCell<dyn Application>>>,
    );

    fn get_application(&self) -> Option<std::rc::Rc<std::cell::RefCell<dyn Application>>>;

    fn set_helper_set(&self, helper_set: std::rc::Rc<std::cell::RefCell<HelperSet>>);

    fn get_helper_set(&self) -> Option<std::rc::Rc<std::cell::RefCell<HelperSet>>>;

    fn merge_application_definition(&self, merge_args: bool);

    fn get_definition(&self) -> std::cell::Ref<'_, InputDefinition>;

    fn get_native_definition(&self) -> std::cell::Ref<'_, InputDefinition>;

    fn set_name(&self, name: &BStr) -> anyhow::Result<()>;

    fn get_name(&self) -> Option<BString>;

    fn set_hidden(&self, hidden: bool);

    fn is_hidden(&self) -> bool;

    fn set_description(&self, description: &BStr);

    fn get_description(&self) -> BString;

    fn set_help(&self, help: &BStr);

    fn get_help(&self) -> BString;

    fn get_processed_help(&self) -> bstr::BString;

    fn set_aliases(&self, aliases: Vec<BString>) -> anyhow::Result<()>;

    fn get_aliases(&self) -> Vec<BString>;

    fn get_synopsis(&self, short: bool) -> bstr::BString;

    fn add_usage(&self, usage: &BStr);

    fn get_usages(&self) -> Vec<BString>;

    fn get_helper(
        &self,
        name: &BStr,
    ) -> anyhow::Result<std::rc::Rc<std::cell::RefCell<dyn HelperInterface>>>;

    fn set_code(
        &self,
        code: Box<dyn Fn(&mut dyn InputInterface, &mut dyn OutputInterface) -> PhpMixed>,
    );

    fn get_code(
        &self,
    ) -> std::cell::Ref<
        '_,
        Option<Box<dyn Fn(&mut dyn InputInterface, &mut dyn OutputInterface) -> PhpMixed>>,
    >;

    fn ignore_validation_errors(&self);

    fn get_ignore_validation_errors(&self) -> bool;
}

impl PhpClass for CommandData {
    fn php_class_name(&self) -> &BStr {
        panic!(
            "php_class_name called on the base command state; concrete commands supply their class name"
        );
    }
}

#[async_trait::async_trait(?Send)]
impl Command for CommandData {
    fn is_enabled(&self) -> bool {
        true
    }

    fn set_application(
        &self,
        application: Option<std::rc::Rc<std::cell::RefCell<dyn Application>>>,
    ) {
        *self.application.borrow_mut() = application.clone();
        if let Some(application) = application {
            self.set_helper_set(application.borrow_mut().get_helper_set());
        } else {
            *self.helper_set.borrow_mut() = None;
        }

        *self.full_definition.borrow_mut() = None;
    }

    fn get_application(&self) -> Option<std::rc::Rc<std::cell::RefCell<dyn Application>>> {
        self.application.borrow().clone()
    }

    fn set_helper_set(&self, helper_set: std::rc::Rc<std::cell::RefCell<HelperSet>>) {
        *self.helper_set.borrow_mut() = Some(helper_set);
    }

    fn get_helper_set(&self) -> Option<std::rc::Rc<std::cell::RefCell<HelperSet>>> {
        self.helper_set.borrow().clone()
    }

    /// Merges the application definition with the command definition.
    fn merge_application_definition(&self, merge_args: bool) {
        let application = match &*self.application.borrow() {
            None => return,
            Some(application) => application.clone(),
        };

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

    fn get_definition(&self) -> std::cell::Ref<'_, InputDefinition> {
        if self.full_definition.borrow().is_some() {
            std::cell::Ref::map(self.full_definition.borrow(), |full_definition| {
                full_definition.as_ref().unwrap()
            })
        } else {
            self.get_native_definition()
        }
    }

    fn get_native_definition(&self) -> std::cell::Ref<'_, InputDefinition> {
        std::cell::Ref::map(self.definition.borrow(), |definition| match definition {
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

    fn set_name(&self, name: &BStr) -> anyhow::Result<()> {
        if let Err(e) = self.validate_name(name)? {
            return Err(e.into());
        }

        *self.name.borrow_mut() = Some(name.into());

        Ok(())
    }

    fn get_name(&self) -> Option<BString> {
        self.name.borrow().clone()
    }

    fn set_hidden(&self, hidden: bool) {
        self.hidden.set(hidden);
    }

    fn is_hidden(&self) -> bool {
        self.hidden.get()
    }

    fn set_description(&self, description: &BStr) {
        *self.description.borrow_mut() = description.into();
    }

    fn get_description(&self) -> BString {
        self.description.borrow().clone()
    }

    fn set_help(&self, help: &BStr) {
        *self.help.borrow_mut() = help.into();
    }

    fn get_help(&self) -> BString {
        self.help.borrow().clone()
    }

    fn get_processed_help(&self) -> bstr::BString {
        let name = self.name.borrow().clone();
        let is_single_command = match &*self.application.borrow() {
            Some(application) => application.borrow().is_single_command(),
            None => false,
        };

        let placeholders = ["%command.name%", "%command.full_name%"];
        let php_self = PHP_SERVER.lock().unwrap().php_self().unwrap_or_default();
        let php_self = path_to_bytes(&php_self);
        let replacements = [
            name.clone().unwrap_or_default(),
            if is_single_command {
                php_self.into()
            } else {
                bformat!("{php_self} {}", name.unwrap_or_default())
            },
        ];

        let help = self.get_help();
        let subject = if help.is_empty() {
            self.get_description()
        } else {
            help
        };

        str_replace_array(&placeholders, &replacements, &subject)
    }

    fn set_aliases(&self, aliases: Vec<BString>) -> anyhow::Result<()> {
        let mut list = Vec::new();

        for alias in &aliases {
            if let Err(e) = self.validate_name(alias.as_bstr())? {
                return Err(e.into());
            }
            list.push(alias.clone());
        }

        // PHP: `\is_array($aliases) ? $aliases : $list`. Here `aliases` is always an
        // array (Vec), so the result is `aliases`; `list` mirrors the validation loop.
        *self.aliases.borrow_mut() = aliases;

        Ok(())
    }

    fn get_aliases(&self) -> Vec<BString> {
        self.aliases.borrow().clone()
    }

    fn get_synopsis(&self, short: bool) -> bstr::BString {
        let key = BString::from(if short { "short" } else { "long" });

        if !self.synopsis.borrow().contains_key(&key) {
            let value = trim(bformat!(
                "{} {}",
                self.name.borrow().clone().unwrap_or_default(),
                self.definition
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .get_synopsis(short)
            ));
            self.synopsis.borrow_mut().insert(key.clone(), value);
        }

        self.synopsis.borrow()[&key].clone()
    }

    fn add_usage(&self, usage: &BStr) {
        let mut usage = BString::from(usage);
        let name = self.name.borrow().clone().unwrap_or_default();
        if !usage.starts_with(&name) {
            usage = bformat!("{name} {usage}");
        }

        self.usages.borrow_mut().push(usage);
    }

    fn get_usages(&self) -> Vec<BString> {
        self.usages.borrow().clone()
    }

    fn get_helper(
        &self,
        name: &BStr,
    ) -> anyhow::Result<std::rc::Rc<std::cell::RefCell<dyn HelperInterface>>> {
        let helper_set = match &*self.helper_set.borrow() {
            None => {
                anyhow::bail!(LogicException::new(bformat!(
                    "Cannot retrieve helper \"{name}\" because there is no HelperSet defined. Did you forget to add your command to the application or to set the application on the command using the setApplication() method? You can also set the HelperSet directly using the setHelperSet() method."
                )));
            }
            Some(helper_set) => helper_set.clone(),
        };

        helper_set.borrow().get(name)
    }

    fn set_code(
        &self,
        code: Box<dyn Fn(&mut dyn InputInterface, &mut dyn OutputInterface) -> PhpMixed>,
    ) {
        // TODO(php-runtime): PHP rebinds an unbound Closure's $this to the command instance via
        // ReflectionFunction/Closure::bind. Rust closures have no `$this` rebinding;
        // the closure is stored as-is.
        *self.code.borrow_mut() = Some(code);
    }

    fn get_code(
        &self,
    ) -> std::cell::Ref<
        '_,
        Option<Box<dyn Fn(&mut dyn InputInterface, &mut dyn OutputInterface) -> PhpMixed>>,
    > {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helper::FormatterHelper;
    use shirabe_php_shim::Catch;

    #[test]
    fn get_helper_looks_the_helper_up_by_name() {
        let formatter: std::rc::Rc<std::cell::RefCell<dyn HelperInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(FormatterHelper::default()));
        let command = CommandData::new(Some("namespace:name".into()));
        command.set_helper_set(HelperSet::new(vec![(None, formatter.clone())]));

        assert!(std::rc::Rc::ptr_eq(
            &command.get_helper(BStr::new("formatter")).unwrap(),
            &formatter,
        ));
    }

    #[test]
    fn get_helper_throws_for_an_undefined_helper() {
        let command = CommandData::new(Some("namespace:name".into()));
        command.set_helper_set(HelperSet::new(vec![]));

        let error = command.get_helper(BStr::new("foo")).unwrap_err();
        let exception = error.catch::<InvalidArgumentException>().unwrap();
        assert_eq!(
            exception.get_message(),
            "The helper \"foo\" is not defined."
        );
    }

    #[test]
    fn get_helper_without_a_helper_set_throws() {
        let command = CommandData::new(Some("namespace:name".into()));

        let error = command.get_helper(BStr::new("formatter")).unwrap_err();
        let exception = error.catch::<LogicException>().unwrap();
        assert_eq!(
            exception.get_message(),
            "Cannot retrieve helper \"formatter\" because there is no HelperSet defined. Did you forget to add your command to the application or to set the application on the command using the setApplication() method? You can also set the HelperSet directly using the setHelperSet() method."
        );
    }
}
