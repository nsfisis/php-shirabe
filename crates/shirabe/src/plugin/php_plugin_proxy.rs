//! PHP-backed `PluginInterface` adapter and the R table serving its RPC callbacks.
//!
//! This module has no Composer counterpart: it is the Rust half of the plugin runtime split
//! (a real PHP child process executes the plugin code, see `docs/dev/php-rpc.md`). The plugin
//! entity lives in the worker's P table; Rust-side entities the plugin can call back into
//! (`$composer`, `$io`) live in the R table here.

use crate::autoload::ClassLoader;
use crate::composer::ComposerHandle;
use crate::io::IOInterface;
use crate::plugin::plugin_interface::PluginInterface;
use indexmap::IndexMap;
use shirabe_php_rpc::{
    PhpThrow, PluginValue, RustMethodDispatcher, RustObjHandle, call_php_method, release_php_handle,
};

/// A Rust-side entity a PHP proxy stub points back to.
#[derive(Debug)]
enum RustEntity {
    Composer(ComposerHandle),
    Io(std::rc::Rc<std::cell::RefCell<dyn IOInterface>>),
}

thread_local! {
    /// The R table. Entries are strong references kept for the worker's lifetime.
    /// TODO(plugin): GC (dropping entries on ReleaseRustHandle) is not implemented yet;
    /// until then entities registered here are intentionally never released.
    static R_TABLE: std::cell::RefCell<IndexMap<u64, RustEntity>> =
        std::cell::RefCell::new(IndexMap::new());
}

/// Registers the Composer instance in the R table, interned by shared-pointer identity so the
/// same instance always crosses the boundary as the same handle (`===` in the child).
pub(crate) fn register_composer_entity(composer: &ComposerHandle) -> u64 {
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
        // TODO(plugin): only the IO classes with hand-written proxy stubs can cross the
        // boundary until a stub generator exists.
        Err(anyhow::anyhow!(
            "no proxy stub class is available for this IO implementation"
        ))
    }
}

/// Looks a class up in every registered Rust-side `ClassLoader`, in registration order — the
/// Rust mirror of what the PHP `spl_autoload_register` stack would do in-process.
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
#[derive(Debug)]
pub(crate) struct PluginRpcDispatcher;

impl RustMethodDispatcher for PluginRpcDispatcher {
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

        R_TABLE.with(|table| {
            let table = table.borrow();
            match table.get(&rhandle) {
                Some(RustEntity::Io(io)) => dispatch_io_method(io, method_name, &args),
                Some(RustEntity::Composer(_)) => {
                    // TODO(plugin): the Composer object graph (getConfig, getRepositoryManager,
                    // ...) becomes reachable over RPC later.
                    Err(runtime_throw(format!(
                        "the Composer method `{method_name}` is not available over RPC yet"
                    )))
                }
                None => Err(runtime_throw(format!("unknown Rust handle {rhandle}"))),
            }
        })
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
}

impl PhpPluginProxy {
    pub fn new(phandle: u64, class: String) -> Self {
        Self { phandle, class }
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
        let outcome = call_php_method(self.phandle, method, args, Some(&mut PluginRpcDispatcher))?;
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

    fn as_php_plugin_proxy(&self) -> Option<&PhpPluginProxy> {
        Some(self)
    }
}

impl Drop for PhpPluginProxy {
    fn drop(&mut self) {
        // A dead worker has nothing left to release.
        let _ = release_php_handle(self.phandle);
    }
}
