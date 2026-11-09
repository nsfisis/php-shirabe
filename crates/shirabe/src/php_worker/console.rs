//! The handoff to the worker-side console application (the `Composer\Console\Application`
//! defined under the RPC crate's `php/runtime/`), which runs plugin-provided commands, and the
//! built-in commands those run back on this side. This module has no Composer counterpart.

use crate::composer::ComposerHandle;
use crate::console::Application;
use crate::io::IOInterface;
use crate::php_worker::call::PhpCall;
use crate::php_worker::rpc::table::{composer_handle_value, io_handle_value};
use crate::util::sync_executor::block_on;
use bstr::{BStr, BString};
use shirabe_php_rpc::{PhpObjHandle, PluginValue, ToWire};
use shirabe_php_shim::{RuntimeException, bformat, trim};
use shirabe_symfony_console::command::Command as SymfonyCommand;
use shirabe_symfony_console::input::InputInterface;
use shirabe_symfony_console::output::{ConsoleOutput, OutputInterface};

shirabe_php_rpc::wire_record! {
    /// Metadata row for one Rust-implemented command, mirrored into the worker as a
    /// `\Shirabe\RustCommandStub` so a worker-hosted command can `find()` and invoke built-in
    /// commands (their execution crosses back into this process).
    #[derive(Debug, Clone)]
    pub(crate) struct RustCommandMetadata {
        pub(crate) name: BString,
        pub(crate) description: BString,
        pub(crate) aliases: Vec<BString>,
        pub(crate) hidden: bool,
    }
}

shirabe_php_rpc::wire_record! {
    /// What `__shirabe_console_application_boot` builds the worker-side application from.
    #[derive(Debug)]
    struct ConsoleBootConfig {
        composer: Option<PluginValue>,
        io: PluginValue,
        initial_working_directory: Option<std::path::PathBuf>,
        disable_plugins_by_default: bool,
        disable_scripts_by_default: bool,
        rust_commands: Vec<RustCommandMetadata>,
        plugin_commands: Vec<PhpObjHandle>,
    }
}

/// Handoff state for the worker-side console application (the `Composer\Console\Application`
/// defined under the RPC crate's `php/runtime/`): assembled by
/// `Application::register_worker_console_commands` as worker-hosted commands are registered,
/// booted in the worker the first time one of them actually runs.
#[derive(Debug)]
pub(crate) struct PhpConsoleApplicationContext {
    composer: Option<ComposerHandle>,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    initial_working_directory: Option<std::path::PathBuf>,
    disable_plugins_by_default: bool,
    disable_scripts_by_default: bool,
    rust_commands: std::cell::RefCell<Vec<RustCommandMetadata>>,
    plugin_commands: std::cell::RefCell<Vec<PhpObjHandle>>,
    app: std::cell::RefCell<Option<PhpObjHandle>>,
}

thread_local! {
    /// Handles of the `PhpCommandProxy` instances built since the last drain; drained into the
    /// console application context by `Application::register_worker_console_commands`.
    pub(crate) static PENDING_COMMAND_HANDLES: std::cell::RefCell<Vec<PhpObjHandle>> =
        const { std::cell::RefCell::new(Vec::new()) };

    /// The published context, read by `PhpCommandProxy::run` at execution time.
    pub(crate) static CONSOLE_APP_CONTEXT: std::cell::RefCell<Option<std::rc::Rc<PhpConsoleApplicationContext>>> =
        const { std::cell::RefCell::new(None) };
}

/// Clears handles a failed earlier collection may have left behind.
pub(crate) fn reset_pending_plugin_command_handles() {
    PENDING_COMMAND_HANDLES.with(|handles| handles.borrow_mut().clear());
}

pub(crate) fn take_pending_plugin_command_handles() -> Vec<PhpObjHandle> {
    PENDING_COMMAND_HANDLES.with(|handles| std::mem::take(&mut *handles.borrow_mut()))
}

pub(crate) fn publish_console_application_context(
    composer: Option<&ComposerHandle>,
    io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    initial_working_directory: Option<std::path::PathBuf>,
    disable_plugins_by_default: bool,
    disable_scripts_by_default: bool,
    rust_commands: Vec<RustCommandMetadata>,
    plugin_commands: Vec<PhpObjHandle>,
) {
    let context = std::rc::Rc::new(PhpConsoleApplicationContext {
        composer: composer.cloned(),
        io: io.clone(),
        initial_working_directory,
        disable_plugins_by_default,
        disable_scripts_by_default,
        rust_commands: std::cell::RefCell::new(rust_commands),
        plugin_commands: std::cell::RefCell::new(plugin_commands),
        app: std::cell::RefCell::new(None),
    });
    CONSOLE_APP_CONTEXT.with(|slot| *slot.borrow_mut() = Some(context));
}

/// Adds command entities to the published handoff, for commands registered after it was
/// published. A worker-side application that already booted gets them right away, so the two
/// sides keep the same command set.
pub(crate) fn extend_console_application_commands(
    handles: Vec<PhpObjHandle>,
) -> anyhow::Result<()> {
    let context = CONSOLE_APP_CONTEXT
        .with(|slot| slot.borrow().clone())
        .expect("only an application that published the handoff extends it");
    for handle in handles {
        if let Some(app) = context.app.borrow().as_ref() {
            PhpCall::Method(app, BStr::new("add"))
                .run::<PluginValue>(vec![PluginValue::PhpHandle(handle.clone())])?;
        }
        context.plugin_commands.borrow_mut().push(handle);
    }
    Ok(())
}

/// Adds a Rust-implemented command to the published handoff, for commands registered after it
/// was published. A worker-side application that already booted gets its
/// `\Shirabe\RustCommandStub` right away, so the two sides keep the same command set.
pub(crate) fn extend_console_application_rust_command(
    metadata: RustCommandMetadata,
) -> anyhow::Result<()> {
    let context = CONSOLE_APP_CONTEXT
        .with(|slot| slot.borrow().clone())
        .expect("only an application that published the handoff extends it");
    let app = context.app.borrow().clone();
    if let Some(app) = app {
        let stub: PhpObjHandle = PhpCall::New(BStr::new("Shirabe\\RustCommandStub"))
            .run(vec![metadata.clone().to_wire()])?;
        PhpCall::Method(&app, BStr::new("add"))
            .run::<PluginValue>(vec![PluginValue::PhpHandle(stub)])?;
    }
    context.rust_commands.borrow_mut().push(metadata);
    Ok(())
}

impl PhpConsoleApplicationContext {
    /// Boots the worker-side application on first use and returns its handle.
    pub(crate) fn booted_app(&self) -> anyhow::Result<PhpObjHandle> {
        if let Some(app) = self.app.borrow().as_ref() {
            return Ok(app.clone());
        }
        let config = ConsoleBootConfig {
            composer: self.composer.as_ref().map(composer_handle_value),
            io: io_handle_value(&self.io)?,
            initial_working_directory: self.initial_working_directory.clone(),
            disable_plugins_by_default: self.disable_plugins_by_default,
            disable_scripts_by_default: self.disable_scripts_by_default,
            rust_commands: self.rust_commands.borrow().clone(),
            plugin_commands: self.plugin_commands.borrow().clone(),
        };
        let app: PhpObjHandle = PhpCall::Function(BStr::new("__shirabe_console_application_boot"))
            .run(vec![config.to_wire()])?;
        *self.app.borrow_mut() = Some(app.clone());
        Ok(app)
    }
}

thread_local! {
    /// The application answering `__shirabe_run_rust_command` callbacks from the plugin
    /// worker's reverse command stubs; registered when plugin commands are collected.
    static WORKER_REVERSE_APPLICATION: std::cell::RefCell<
        Option<std::rc::Weak<std::cell::RefCell<Application>>>,
    > = const { std::cell::RefCell::new(None) };
}

/// The metadata a worker-side `\Shirabe\RustCommandStub` mirrors of a registered command.
pub(crate) fn rust_command_metadata(command: &dyn SymfonyCommand) -> RustCommandMetadata {
    RustCommandMetadata {
        name: command
            .get_name()
            .expect("a registered command always has a name"),
        description: command.get_description(),
        aliases: command.get_aliases(),
        hidden: command.is_hidden(),
    }
}

/// Registers the application the worker's reverse command stubs call back into.
pub(crate) fn register_worker_reverse_application(
    application: std::rc::Weak<std::cell::RefCell<Application>>,
) {
    WORKER_REVERSE_APPLICATION.with(|slot| *slot.borrow_mut() = Some(application));
}

/// Runs a built-in command on behalf of a plugin-provided command executing in the worker
/// (the reverse half of the two-world command split): the stringified input the plugin passed
/// is re-parsed here and the command runs against this side's application state — helper set
/// included — writing to the same stdio the worker inherited.
pub(crate) fn run_worker_reverse_command(name: &BStr, input_line: &BStr) -> anyhow::Result<i64> {
    let application = WORKER_REVERSE_APPLICATION
        .with(|slot| slot.borrow().as_ref().and_then(std::rc::Weak::upgrade))
        .ok_or_else(|| -> anyhow::Error {
            RuntimeException::new(bformat!(
                "cannot run command {name}: no application is registered for worker callbacks"
            ))
            .into()
        })?;
    let command = application.borrow_mut().find(name)?;
    // PHP's `(string) $input` omits the command name when the input was built without an
    // explicit `command` entry, but the merged definition binds the first positional token to
    // the `command` argument, so the name is prepended when the first token is not this
    // command.
    let trimmed = trim(input_line);
    let first_token = trimmed
        .split(u8::is_ascii_whitespace)
        .find(|token| !token.is_empty())
        .unwrap_or_default();
    let is_named = {
        let command = command.borrow();
        command
            .get_name()
            .is_some_and(|name| name.as_slice() == first_token)
            || command
                .get_aliases()
                .iter()
                .any(|alias| alias.as_slice() == first_token)
    };
    let line = if is_named {
        trimmed
    } else if trimmed.is_empty() {
        BString::from(name)
    } else {
        bformat!("{name} {trimmed}")
    };
    let input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>> = std::rc::Rc::new(
        std::cell::RefCell::new(shirabe_symfony_console::input::StringInput::new(&line)?),
    );
    let output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> = std::rc::Rc::new(
        std::cell::RefCell::new(ConsoleOutput::new(None, None, None)?),
    );
    // The worker's callback is a synchronous RPC request, answered on the thread serving it.
    block_on(command.borrow().run(input, output))
}
