//! `Command` adapter for a command entity living in the PHP child process.

use crate::command::{BaseCommand, BaseCommandData};
use crate::delegate_base_command_trait_impls_to_inner;
use crate::php_worker::call::PhpCall;
use crate::php_worker::console::{CONSOLE_APP_CONTEXT, PENDING_COMMAND_HANDLES};
use crate::php_worker::runtime::php_is_a;
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_rpc::{FromWire, PhpObjHandle, PluginValue};
use shirabe_php_shim::{PhpClass, PhpMixed, RuntimeException, ToPhpString, bformat};
use shirabe_symfony_console::command::Command;
use shirabe_symfony_console::input::{InputInterface, InputValue};
use shirabe_symfony_console::output::OutputInterface;

shirabe_php_rpc::wire_record! {
    /// A command's input definition, help text and extra usages, as
    /// `__shirabe_read_command_definition` reads them.
    #[derive(Debug)]
    struct CommandDefinition {
        arguments: Vec<CommandArgument>,
        options: Vec<CommandOption>,
        help: BString,
        usages: Vec<BString>,
    }
}

shirabe_php_rpc::wire_record! {
    /// One `InputArgument` of a [`CommandDefinition`].
    #[derive(Debug)]
    struct CommandArgument {
        name: BString,
        required: bool,
        is_array: bool,
        description: BString,
        default: PhpMixed,
    }
}

shirabe_php_rpc::wire_record! {
    /// One `InputOption` of a [`CommandDefinition`].
    #[derive(Debug)]
    struct CommandOption {
        name: BString,
        shortcut: Option<BString>,
        accept_value: bool,
        is_value_required: bool,
        is_array: bool,
        is_negatable: bool,
        description: BString,
        default: PhpMixed,
    }
}

/// `BaseCommand` adapter for a command entity living in the PHP child process. The Rust-side
/// command state mirrors the child's metadata (name, description, aliases, hidden/proxy flags,
/// help, usages and the input definition — read back over RPC at construction, after the PHP
/// constructor ran `configure()`), so `list` and `help` render from local state; running the
/// command forwards the whole input line to the worker-side console application.
#[derive(Debug)]
pub struct PhpCommandProxy {
    base_command_data: BaseCommandData,
    handle: PhpObjHandle,
    proxy_command: bool,
}

impl PhpCommandProxy {
    pub(crate) fn new(handle: PhpObjHandle) -> anyhow::Result<Self> {
        let proxy_command = Self::call_metadata_getter(&handle, BStr::new("isProxyCommand"))?;
        Self::build(handle, proxy_command)
    }

    /// A command class named by a `composer.json` script only has to extend Symfony's `Command`,
    /// so `isProxyCommand()` is asked for only when it also extends Composer's `BaseCommand`.
    pub(crate) fn new_script_command(handle: PhpObjHandle) -> anyhow::Result<Self> {
        let proxy_command = if php_is_a(&handle, BStr::new("Composer\\Command\\BaseCommand"))? {
            Self::call_metadata_getter(&handle, BStr::new("isProxyCommand"))?
        } else {
            false
        };
        Self::build(handle, proxy_command)
    }

    fn build(handle: PhpObjHandle, proxy_command: bool) -> anyhow::Result<Self> {
        let data = BaseCommandData::new(None);
        let name: Option<BString> = Self::call_metadata_getter(&handle, BStr::new("getName"))?;
        if let Some(name) = name {
            Command::set_name(&data, name.as_bstr())?;
        }
        let description: BString =
            Self::call_metadata_getter(&handle, BStr::new("getDescription"))?;
        Command::set_description(&data, description.as_bstr());
        let aliases: Vec<BString> = Self::call_metadata_getter(&handle, BStr::new("getAliases"))?;
        Command::set_aliases(&data, aliases)?;
        let hidden = Self::call_metadata_getter(&handle, BStr::new("isHidden"))?;
        Command::set_hidden(&data, hidden);
        Self::read_back_definition(&handle, &data)?;
        PENDING_COMMAND_HANDLES.with(|handles| handles.borrow_mut().push(handle.clone()));
        Ok(Self {
            base_command_data: data,
            handle,
            proxy_command,
        })
    }

    /// Mirrors the command's input definition (plus help text and extra usages) into the
    /// Rust-side command state, so `help`/`list` render it without touching the worker.
    fn read_back_definition(handle: &PhpObjHandle, data: &BaseCommandData) -> anyhow::Result<()> {
        use shirabe_symfony_console::input::{
            DefinitionItem, InputArgument, InputDefinition, InputOption,
        };

        let definition: CommandDefinition =
            PhpCall::Function(BStr::new("__shirabe_read_command_definition"))
                .run(vec![PluginValue::PhpHandle(handle.clone())])?;

        let mut items: Vec<DefinitionItem> = Vec::new();
        for argument in definition.arguments {
            let mut mode = if argument.required {
                InputArgument::REQUIRED
            } else {
                InputArgument::OPTIONAL
            };
            if argument.is_array {
                mode |= InputArgument::IS_ARRAY;
            }
            items.push(DefinitionItem::InputArgument(InputArgument::new(
                argument.name,
                Some(mode),
                argument.description,
                InputValue::from_php_mixed(&argument.default),
            )?));
        }
        for option in definition.options {
            let mut mode = if option.accept_value {
                if option.is_value_required {
                    InputOption::VALUE_REQUIRED
                } else {
                    InputOption::VALUE_OPTIONAL
                }
            } else {
                InputOption::VALUE_NONE
            };
            if option.is_array {
                mode |= InputOption::VALUE_IS_ARRAY;
            }
            if option.is_negatable {
                mode |= InputOption::VALUE_NEGATABLE;
            }
            // `getDefault()` exposes the stored representation (`false` for VALUE_NONE), while
            // the constructor only accepts null there; mirror the constructor's normalization.
            let default = if option.accept_value {
                InputValue::from_php_mixed(&option.default)
            } else {
                InputValue::Null
            };
            items.push(DefinitionItem::InputOption(InputOption::new(
                &option.name,
                option.shortcut.as_ref().map(|shortcut| shortcut.as_bstr()),
                Some(mode),
                option.description,
                default,
            )?));
        }
        data.command_data().set_definition(
            shirabe_symfony_console::command::SetDefinitionArg::Definition(InputDefinition::new(
                items,
            )?),
        );

        Command::set_help(data, definition.help.as_bstr());
        for usage in definition.usages {
            Command::add_usage(data, usage.as_bstr());
        }
        Ok(())
    }

    fn call_metadata_getter<R: FromWire>(
        handle: &PhpObjHandle,
        method: &BStr,
    ) -> anyhow::Result<R> {
        PhpCall::Method(handle, method).run(Vec::new())
    }
}

#[async_trait::async_trait(?Send)]
impl Command for PhpCommandProxy {
    /// Forwards the whole run to the worker-side console application (the proxy-command idiom
    /// the trait allows): the real Symfony machinery there performs input binding, validation,
    /// interaction and execution against the live PHP command object, writing to the stdio the
    /// worker inherited. The Rust-side `base_run` half must not run against the mirrored
    /// definition, or binding and interaction would happen twice.
    async fn run(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        _output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let context = CONSOLE_APP_CONTEXT
            .with(|slot| slot.borrow().clone())
            .ok_or_else(|| -> anyhow::Error {
                RuntimeException::new(bformat!("cannot run plugin-provided command {}: no worker-side console application context was published", self.handle.class()))
                .into()
            })?;
        let app = context.booted_app()?;
        let input_line = input.borrow().to_php_string();
        PhpCall::Function(BStr::new("__shirabe_run_console_application")).run(vec![
            PluginValue::PhpHandle(app),
            PluginValue::string(input_line),
        ])
    }

    async fn execute(
        &self,
        _input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        _output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        // `run` above never reaches this template hook; a direct call would bypass the
        // worker-side binding, so it stays an explicit error.
        Err(RuntimeException::new(bformat!("plugin-provided command {} executes in the PHP worker through run(); execute() must not be called directly", self.handle.class()))
        .into())
    }

    fn is_proxy_command(&self) -> bool {
        self.proxy_command
    }

    shirabe_symfony_console::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for PhpCommandProxy {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    delegate_base_command_trait_impls_to_inner!(base_command_data);
}

impl PhpClass for PhpCommandProxy {
    fn php_class_name(&self) -> &BStr {
        self.handle.class()
    }
}
