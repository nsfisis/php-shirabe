//! The handoff to the worker-side console application (the `Composer\Console\Application`
//! defined under the RPC crate's `php/runtime/`), which runs plugin-provided commands. This
//! module has no Composer counterpart.

use crate::composer::ComposerHandle;
use crate::io::IOInterface;
use crate::php_worker::call::PhpCall;
use crate::php_worker::rpc::table::{composer_handle_value, io_handle_value};
use bstr::{BStr, BString};
use shirabe_php_rpc::{PhpObjHandle, PluginValue, ToWire};

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
