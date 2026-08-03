//! PHP-backed `PluginInterface` adapter and the R table serving its RPC callbacks.
//!
//! This module has no Composer counterpart: it is the Rust half of the plugin runtime split
//! (a real PHP child process executes the plugin code, see `docs/dev/php-rpc.md`). The plugin
//! entity lives in the worker's P table; Rust-side entities the plugin can call back into
//! (`$composer`, `$io`) live in the R table here.

use crate::autoload::ClassLoader;
use crate::composer::ComposerHandle;
use crate::event_dispatcher::event_dispatcher::dispatch_event_method;
use crate::event_dispatcher::{EventInterface, EventSubscriberInterface, SubscribedEventEntry};
use crate::io::IOInterface;
use crate::plugin::plugin_interface::PluginInterface;
use indexmap::IndexMap;
use shirabe_php_rpc::{
    PhpObjHandle, PhpThrow, PluginValue, RustMethodDispatcher, RustObjHandle, call_php_method,
    release_php_handle,
};

/// A Rust-side entity a PHP proxy stub points back to.
#[derive(Debug, Clone)]
enum RustEntity {
    Composer(ComposerHandle),
    Io(std::rc::Rc<std::cell::RefCell<dyn IOInterface>>),
}

thread_local! {
    /// The R table. Entries are strong references held while the child-side stub lives; a
    /// ReleaseRustHandle notification (sent by the stub's `__destruct`) removes the entry.
    /// Handles are monotonically increasing and never reused, so a released handle can never
    /// be confused with a later entity (no generation counter is needed).
    /// TODO(plugin): thread-local while the worker and its stub intern table are
    /// process-global; the session lock serializes calls, and every dispatch currently runs on
    /// the thread that registered the handle, but a handle minted on one thread is invisible
    /// to another.
    static R_TABLE: std::cell::RefCell<IndexMap<u64, RustEntity>> =
        std::cell::RefCell::new(IndexMap::new());

    static RELEASE_HOOK_INSTALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Installs the ReleaseRustHandle listener that drops R-table entries when the child-side stub
/// is destructed. Idempotent; called by every entity registration.
fn ensure_release_hook_installed() {
    RELEASE_HOOK_INSTALLED.with(|installed| {
        if !installed.get() {
            installed.set(true);
            shirabe_php_rpc::set_release_rust_handle_hook(|rhandle| {
                R_TABLE.with(|table| {
                    table.borrow_mut().shift_remove(&rhandle);
                });
            });
        }
    });
}

/// Registers the Composer instance in the R table, interned by shared-pointer identity so the
/// same instance always crosses the boundary as the same handle (`===` in the child).
pub(crate) fn register_composer_entity(composer: &ComposerHandle) -> u64 {
    ensure_release_hook_installed();
    R_TABLE.with(|table| {
        let mut table = table.borrow_mut();
        let ptr = std::rc::Rc::as_ptr(composer.as_rc()) as *const () as usize;
        for (rhandle, entity) in table.iter() {
            if let RustEntity::Composer(existing) = entity
                && std::rc::Rc::as_ptr(existing.as_rc()) as *const () as usize == ptr
            {
                return *rhandle;
            }
        }
        let rhandle = shirabe_php_rpc::alloc_rhandle();
        table.insert(rhandle, RustEntity::Composer(composer.clone()));
        rhandle
    })
}

/// Registers an IO instance in the R table, interned like [`register_composer_entity`].
pub(crate) fn register_io_entity(io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>) -> u64 {
    ensure_release_hook_installed();
    R_TABLE.with(|table| {
        let mut table = table.borrow_mut();
        let ptr = std::rc::Rc::as_ptr(io) as *const () as usize;
        for (rhandle, entity) in table.iter() {
            if let RustEntity::Io(existing) = entity
                && std::rc::Rc::as_ptr(existing) as *const () as usize == ptr
            {
                return *rhandle;
            }
        }
        let rhandle = shirabe_php_rpc::alloc_rhandle();
        table.insert(rhandle, RustEntity::Io(io.clone()));
        rhandle
    })
}

/// The PHP class name (= proxy stub class) of a Rust IO instance, for the `__class` field of
/// its handle descriptor.
pub(crate) fn io_stub_class(
    io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
) -> anyhow::Result<&'static str> {
    let borrowed = io.borrow();
    let any = borrowed.as_any();
    if any
        .downcast_ref::<crate::io::buffer_io::BufferIO>()
        .is_some()
    {
        Ok("Composer\\IO\\BufferIO")
    } else if any
        .downcast_ref::<crate::io::console_io::ConsoleIO>()
        .is_some()
    {
        Ok("Composer\\IO\\ConsoleIO")
    } else if any.downcast_ref::<crate::io::null_io::NullIO>().is_some() {
        Ok("Composer\\IO\\NullIO")
    } else {
        // TODO(plugin): only IO implementations with a generated proxy stub can cross the
        // boundary; the rest are an explicit error until stubs of their own are generated.
        Err(anyhow::anyhow!(
            "no proxy stub class is available for this IO implementation"
        ))
    }
}

/// Looks a class up in every registered Rust-side `ClassLoader`, in registration order — the
/// Rust mirror of what the PHP `spl_autoload_register` stack would do in-process.
///
/// TODO(plugin): `ClassLoader::register` keeps one loader per vendor-dir (matching upstream's
/// `$registeredLoaders`), but the real spl stack keeps every registered loader; because the
/// `spl_autoload_register` shim is a no-op, registering a second plugin loader under the same
/// vendor-dir evicts the first one here, and a class of the earlier plugin that was never
/// loaded can become unresolvable (PHP would still find it).
pub(crate) fn find_file_in_registered_loaders(class: &str) -> Option<String> {
    for (_vendor_dir, mut loader) in ClassLoader::get_registered_loaders() {
        if let Some(file) = loader.find_file(class) {
            return Some(file);
        }
    }
    None
}

/// Serves `CallRustMethod` requests from the child while a plugin-related call is in flight:
/// rhandle 0 is the runtime service endpoint (autoload lookups), every other rhandle resolves
/// through the R table. Unsupported methods are explicit errors, never silent fallbacks.
#[derive(Debug, Default)]
pub(crate) struct PluginRpcDispatcher<'a> {
    /// At most one live event handle exposed per dispatched listener call, alongside the
    /// persistent R table.
    ///
    /// TODO(plugin): a listener that stores the event stub beyond its own call observes an
    /// unknown handle error afterwards; keeping events in the R table needs full proxying of
    /// the object graph an event exposes, which does not exist yet.
    pub(crate) event: Option<(u64, &'a dyn EventInterface)>,
}

impl RustMethodDispatcher for PluginRpcDispatcher<'_> {
    fn dispatch(
        &mut self,
        rhandle: u64,
        method_name: &str,
        args: Vec<PluginValue>,
        _out_param_positions: &[u32],
    ) -> Result<PluginValue, PhpThrow> {
        if rhandle == 0 {
            if method_name == "__shirabe_find_file" {
                let class = match args.first() {
                    // TODO(phase-e): lossy UTF-8; class names are bytes in PHP.
                    Some(PluginValue::String(bytes)) => String::from_utf8_lossy(bytes).into_owned(),
                    other => {
                        return Err(runtime_throw(format!(
                            "__shirabe_find_file expects a class name argument, got {other:?}"
                        )));
                    }
                };
                return Ok(match find_file_in_registered_loaders(&class) {
                    Some(file) => PluginValue::string(file),
                    None => PluginValue::Null,
                });
            }
            return Err(runtime_throw(format!(
                "unknown runtime service method `{method_name}`"
            )));
        }

        if let Some((event_rhandle, event)) = self.event
            && event_rhandle == rhandle
        {
            return dispatch_event_method(event, method_name);
        }

        // The entity is cloned out so no table borrow is held while the handler runs (a
        // handler that re-enters register_*_entity would otherwise panic on the RefCell).
        let entity = R_TABLE.with(|table| table.borrow().get(&rhandle).cloned());
        match entity {
            Some(RustEntity::Io(io)) => dispatch_io_method(&io, method_name, &args),
            Some(RustEntity::Composer(_)) => {
                // TODO(plugin): the Composer object graph (getConfig, getRepositoryManager,
                // ...) becomes reachable over RPC later.
                Err(runtime_throw(format!(
                    "the Composer method `{method_name}` is not available over RPC yet"
                )))
            }
            None => Err(runtime_throw(format!("unknown Rust handle {rhandle}"))),
        }
    }
}

fn dispatch_io_method(
    io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    method_name: &str,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    match method_name {
        "write" | "writeError" => {
            let (messages, newline, verbosity) = decode_write_args(method_name, args)?;
            let borrowed = io.borrow();
            for message in &messages {
                if method_name == "write" {
                    borrowed.write3(message, newline, verbosity);
                } else {
                    borrowed.write_error3(message, newline, verbosity);
                }
            }
            Ok(PluginValue::Null)
        }
        "isInteractive" => Ok(PluginValue::Bool(io.borrow().is_interactive())),
        "isVerbose" => Ok(PluginValue::Bool(io.borrow().is_verbose())),
        "isVeryVerbose" => Ok(PluginValue::Bool(io.borrow().is_very_verbose())),
        "isDebug" => Ok(PluginValue::Bool(io.borrow().is_debug())),
        "isDecorated" => Ok(PluginValue::Bool(io.borrow().is_decorated())),
        // TODO(plugin): the remaining IOInterface surface (ask*, authentications, ...) is
        // widened on demand, driven by explicit errors from real plugins.
        other => Err(runtime_throw(format!(
            "the IO method `{other}` is not available over RPC yet"
        ))),
    }
}

/// Decodes `($messages, bool $newline, int $verbosity)`: `$messages` is a string or a list of
/// strings in PHP.
fn decode_write_args(
    method_name: &str,
    args: &[PluginValue],
) -> Result<(Vec<String>, bool, i64), PhpThrow> {
    // TODO(phase-e): lossy UTF-8; IO messages are bytes in PHP.
    let messages = match args.first() {
        Some(PluginValue::String(bytes)) => vec![String::from_utf8_lossy(bytes).into_owned()],
        Some(PluginValue::List(items)) => {
            let mut messages = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    PluginValue::String(bytes) => {
                        messages.push(String::from_utf8_lossy(bytes).into_owned());
                    }
                    other => {
                        return Err(runtime_throw(format!(
                            "{method_name} expects string messages, got {other:?}"
                        )));
                    }
                }
            }
            messages
        }
        other => {
            return Err(runtime_throw(format!(
                "{method_name} expects a string or list of strings, got {other:?}"
            )));
        }
    };
    let newline = match args.get(1) {
        Some(PluginValue::Bool(b)) => *b,
        None => true,
        other => {
            return Err(runtime_throw(format!(
                "{method_name} expects a bool newline flag, got {other:?}"
            )));
        }
    };
    let verbosity = match args.get(2) {
        Some(PluginValue::Int(v)) => *v,
        None => crate::io::NORMAL,
        other => {
            return Err(runtime_throw(format!(
                "{method_name} expects an int verbosity, got {other:?}"
            )));
        }
    };
    Ok((messages, newline, verbosity))
}

fn runtime_throw(message: String) -> PhpThrow {
    PhpThrow {
        exception_class: "RuntimeException".to_string(),
        message,
        code: 0,
    }
}

/// `PluginInterface` adapter for a plugin entity living in the PHP child process: every
/// lifecycle call is forwarded as a `CallPhpMethod` RPC.
#[derive(Debug)]
pub struct PhpPluginProxy {
    pub(crate) phandle: u64,
    pub(crate) class: String,
    /// Every interface the entity's class implements (`class_implements` in the child), for
    /// the `instanceof` checks PHP performs on plugin objects.
    pub(crate) implements: Vec<String>,
}

impl PhpPluginProxy {
    pub fn new(phandle: u64, class: String, implements: Vec<String>) -> Self {
        Self {
            phandle,
            class,
            implements,
        }
    }

    fn forward_lifecycle_call(
        &self,
        method: &str,
        composer: &ComposerHandle,
        io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> anyhow::Result<()> {
        let composer_rhandle = register_composer_entity(composer);
        let io_rhandle = register_io_entity(io);
        let io_class = io_stub_class(io)?;
        let args = vec![
            PluginValue::RustHandle(RustObjHandle {
                rhandle: composer_rhandle,
                class: "Composer\\Composer".to_string(),
                epoch: 0,
                snapshot: None,
            }),
            PluginValue::RustHandle(RustObjHandle {
                rhandle: io_rhandle,
                class: io_class.to_string(),
                epoch: 0,
                snapshot: None,
            }),
        ];
        let outcome = call_php_method(
            self.phandle,
            method,
            args,
            Some(&mut PluginRpcDispatcher::default()),
        )?;
        match outcome {
            Ok(_) => Ok(()),
            // TODO(plugin): the original exception class is collapsed to RuntimeException on
            // this side of the boundary.
            Err(throw) => Err(anyhow::anyhow!(shirabe_php_shim::RuntimeException {
                message: throw.message,
                code: throw.code,
            })),
        }
    }

    /// For testing only: reads a public property of the plugin entity in the child, mirroring
    /// PHPUnit assertions like `$plugins[0]->version`.
    pub fn __get_property(&self, name: &str) -> anyhow::Result<shirabe_php_shim::PhpMixed> {
        let outcome = shirabe_php_rpc::call_function(
            "__shirabe_get_property",
            vec![
                PluginValue::PhpHandle(shirabe_php_rpc::PhpObjHandle {
                    phandle: self.phandle,
                    class: self.class.clone(),
                    implements: Vec::new(),
                }),
                PluginValue::string(name),
            ],
        )?;
        match outcome {
            Ok(value) => Ok(value.to_php_mixed()?),
            Err(throw) => Err(anyhow::anyhow!(shirabe_php_shim::RuntimeException {
                message: throw.message,
                code: throw.code,
            })),
        }
    }
}

impl PluginInterface for PhpPluginProxy {
    fn activate(
        &mut self,
        composer: ComposerHandle,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> anyhow::Result<()> {
        self.forward_lifecycle_call("activate", &composer, &io)
    }

    fn deactivate(
        &mut self,
        composer: ComposerHandle,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> anyhow::Result<()> {
        self.forward_lifecycle_call("deactivate", &composer, &io)
    }

    fn uninstall(
        &mut self,
        composer: ComposerHandle,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> anyhow::Result<()> {
        self.forward_lifecycle_call("uninstall", &composer, &io)
    }

    fn get_class_name(&self) -> String {
        self.class.clone()
    }

    fn as_event_subscriber(&self) -> Option<&dyn EventSubscriberInterface> {
        if self
            .implements
            .iter()
            .any(|interface| interface == "Composer\\EventDispatcher\\EventSubscriberInterface")
        {
            Some(self)
        } else {
            None
        }
    }

    fn __as_php_plugin_proxy(&self) -> Option<&PhpPluginProxy> {
        Some(self)
    }
}

impl EventSubscriberInterface for PhpPluginProxy {
    fn get_subscribed_events(&self) -> anyhow::Result<IndexMap<String, SubscribedEventEntry>> {
        // PHP: `$subscriber->getSubscribedEvents()` (an instance call of the static method).
        let outcome = call_php_method(
            self.phandle,
            "getSubscribedEvents",
            Vec::new(),
            Some(&mut PluginRpcDispatcher::default()),
        )?;
        let value = match outcome {
            Ok(value) => value,
            // TODO(plugin): the original exception class is collapsed to RuntimeException on
            // this side of the boundary.
            Err(throw) => {
                return Err(anyhow::anyhow!(shirabe_php_shim::RuntimeException {
                    message: throw.message,
                    code: throw.code,
                }));
            }
        };
        decode_subscribed_events(&self.class, value)
    }

    fn subscriber_handle(&self) -> PhpObjHandle {
        PhpObjHandle {
            phandle: self.phandle,
            class: self.class.clone(),
            implements: self.implements.clone(),
        }
    }
}

/// Decodes a `getSubscribedEvents()` wire value into the three shapes `addSubscriber`
/// distinguishes: `'method'`, `['method', priority]`, and `[['method', priority], ...]`.
/// A shape outside these is an explicit error, never a silently dropped listener.
fn decode_subscribed_events(
    class: &str,
    value: PluginValue,
) -> anyhow::Result<IndexMap<String, SubscribedEventEntry>> {
    let entries = match value {
        PluginValue::Array(entries) => entries,
        PluginValue::List(items) => {
            // A PHP list-shaped array (integer keys) cannot map event names; an empty one is
            // the only legal case (an empty PHP array serializes as a list).
            if items.is_empty() {
                IndexMap::new()
            } else {
                return Err(subscribed_events_shape_error(
                    class,
                    &PluginValue::List(items),
                ));
            }
        }
        other => return Err(subscribed_events_shape_error(class, &other)),
    };
    let mut events: IndexMap<String, SubscribedEventEntry> = IndexMap::new();
    for (event_name, params) in entries {
        // TODO(phase-e): lossy UTF-8; event and method names are bytes in PHP.
        let event_name = String::from_utf8_lossy(&event_name).into_owned();
        let entry = match &params {
            PluginValue::String(method) => {
                SubscribedEventEntry::Method(String::from_utf8_lossy(method).into_owned())
            }
            PluginValue::List(items) => match items.first() {
                Some(PluginValue::String(method)) => SubscribedEventEntry::MethodWithPriority(
                    String::from_utf8_lossy(method).into_owned(),
                    decode_listener_priority(class, items.get(1))?,
                ),
                _ => {
                    let mut listeners = Vec::with_capacity(items.len());
                    for listener in items {
                        let PluginValue::List(pair) = listener else {
                            return Err(subscribed_events_shape_error(class, &params));
                        };
                        let Some(PluginValue::String(method)) = pair.first() else {
                            return Err(subscribed_events_shape_error(class, &params));
                        };
                        listeners.push((
                            String::from_utf8_lossy(method).into_owned(),
                            decode_listener_priority(class, pair.get(1))?,
                        ));
                    }
                    SubscribedEventEntry::Methods(listeners)
                }
            },
            _ => return Err(subscribed_events_shape_error(class, &params)),
        };
        events.insert(event_name, entry);
    }
    Ok(events)
}

fn decode_listener_priority(
    class: &str,
    value: Option<&PluginValue>,
) -> anyhow::Result<Option<i64>> {
    match value {
        None => Ok(None),
        Some(PluginValue::Int(priority)) => Ok(Some(*priority)),
        Some(other) => Err(subscribed_events_shape_error(class, other)),
    }
}

fn subscribed_events_shape_error(class: &str, value: &PluginValue) -> anyhow::Error {
    anyhow::anyhow!(shirabe_php_shim::RuntimeException {
        message: format!(
            "{class}::getSubscribedEvents() returned an unsupported shape over RPC: {value:?}"
        ),
        code: 0,
    })
}

impl Drop for PhpPluginProxy {
    fn drop(&mut self) {
        // A dead worker has nothing left to release.
        let _ = release_php_handle(self.phandle);
    }
}
