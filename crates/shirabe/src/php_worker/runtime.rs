//! Thin wrappers that evaluate a PHP built-in, or load a piece of runtime, in the worker on behalf
//! of the ported code.
//!
//! The wrappers differ in what they set up before the call, and are kept apart by module:
//!
//! - the ones here run against whatever the worker has loaded so far;
//! - the ones in [`autoloading`] turn the script autoloader on first;
//! - the ones in [`script`] turn the script autoloader on and serve the callbacks with the
//!   [`ScriptRpcDispatcher`](crate::php_worker::rpc::dispatch::ScriptRpcDispatcher).

use crate::php_worker::call::PhpCall;
use crate::php_worker::installed_versions;
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_rpc::{FromWire, PhpObjHandle, PluginValue};
use shirabe_php_shim::bformat;

pub(crate) mod autoloading;
pub(crate) mod script;

/// Makes the worker's script-class autoloader active, so class queries and script execution
/// in the child can resolve classes through the Rust-side [`ClassLoader`]s registered on the
/// autoload stack.
///
/// [`ClassLoader`]: crate::autoload::ClassLoader
pub(crate) fn ensure_script_autoloader() -> anyhow::Result<()> {
    PhpCall::Function(BStr::new("__shirabe_enable_script_autoloader"))
        .run::<PluginValue>(Vec::new())?;
    Ok(())
}

/// Loads the Composer PHP runtime (symfony/console and friends) into the worker, needed
/// before a `scripts` Command class can be autoloaded and hosted, or a plugin class can implement
/// a Composer interface there.
pub(crate) fn ensure_composer_php_runtime(
    cache_dir: impl FnOnce() -> anyhow::Result<std::path::PathBuf>,
) -> anyhow::Result<()> {
    let autoload = composer_php_runtime_autoload(cache_dir)?;
    PhpCall::Function(BStr::new("__shirabe_require"))
        .run::<PluginValue>(vec![PluginValue::string(autoload)])?;
    installed_versions::load_pending()
}

/// For testing only: a test that never registers a plugin package still needs the Composer
/// PHP runtime in the worker before a class of its own can implement a Composer interface
/// there.
pub fn __ensure_composer_php_runtime(cache_dir: &std::path::Path) -> anyhow::Result<()> {
    ensure_composer_php_runtime(|| Ok(cache_dir.to_path_buf()))
}

/// The `vendor/autoload.php` of the Composer PHP runtime.
fn composer_php_runtime_autoload(
    cache_dir: impl FnOnce() -> anyhow::Result<std::path::PathBuf>,
) -> anyhow::Result<BString> {
    Ok(bformat!(
        "{}/vendor/autoload.php",
        shirabe_php_rpc::composer_runtime::base_path(cache_dir)?
    ))
}

/// `class_exists($class, $autoload)`.
pub(crate) fn class_exists(class: &BStr, autoload: bool) -> anyhow::Result<bool> {
    PhpCall::Function(BStr::new("class_exists")).run(vec![
        PluginValue::string(class),
        PluginValue::Bool(autoload),
    ])
}

/// `is_a($class, $parent, true)`: whether the class named `class` is `parent` or extends or
/// implements it.
pub(crate) fn is_a_class(class: &BStr, parent: &BStr) -> anyhow::Result<bool> {
    PhpCall::Function(BStr::new("is_a")).run(vec![
        PluginValue::string(class),
        PluginValue::string(parent),
        PluginValue::Bool(true),
    ])
}

/// `is_a($obj, $class)` evaluated in the worker: the child's own class table answers, so
/// parent classes are covered (a `PhpObjHandle`'s `implements` lists interfaces only).
pub(crate) fn is_a(handle: &PhpObjHandle, class: &BStr) -> anyhow::Result<bool> {
    PhpCall::Function(BStr::new("is_a")).run(vec![
        PluginValue::PhpHandle(handle.clone()),
        PluginValue::string(class),
    ])
}

/// `eval($code)`.
pub(crate) fn eval(code: BString) -> anyhow::Result<()> {
    PhpCall::Function(BStr::new("__shirabe_eval"))
        .run::<PluginValue>(vec![PluginValue::string(code)])?;
    Ok(())
}

/// `\Composer\Autoload\composerRequire($fileIdentifier, $file)`.
pub(crate) fn composer_require(file_identifier: &BStr, file: &BStr) -> anyhow::Result<()> {
    PhpCall::Function(BStr::new("__shirabe_composer_require")).run::<PluginValue>(vec![
        PluginValue::string(file_identifier),
        PluginValue::string(file),
    ])?;
    Ok(())
}

/// `new $class(...$args)`, returning the P-table handle of the new entity.
pub(crate) fn new_object(class: &BStr, args: Vec<PluginValue>) -> anyhow::Result<PhpObjHandle> {
    PhpCall::New(class).run(args)
}

/// The replacement `doRenderThrowable` writes for an anonymous class name found in a message:
/// `(get_parent_class($class) ?: key(class_implements($class)) ?: 'class').'@anonymous'` when
/// `class_exists($class, false)`, else the name unchanged. Anonymous classes are declared only by
/// PHP code the worker ran, so the worker's class table answers, and without a worker no such
/// class exists.
pub(crate) fn anonymous_class_display_name(class: &BStr) -> anyhow::Result<BString> {
    let string = || PluginValue::string(class);
    let exists = shirabe_php_rpc::worker_is_running()
        && PhpCall::Function(BStr::new("class_exists"))
            .run::<bool>(vec![string(), PluginValue::Bool(false)])?;
    if !exists {
        return Ok(class.to_owned());
    }

    let get_parent_class = PhpCall::Function(BStr::new("get_parent_class"));
    let parent = match get_parent_class.run::<PluginValue>(vec![string()])? {
        PluginValue::Bool(false) => None,
        value => Some(
            BString::from_wire(&value)
                .map_err(|throw| get_parent_class.unsupported_shape(throw))?,
        ),
    };
    let name = match parent {
        Some(parent) => parent,
        None => PhpCall::Function(BStr::new("class_implements"))
            .run::<IndexMap<BString, PluginValue>>(vec![string()])?
            .into_keys()
            .next()
            .unwrap_or_else(|| BString::from("class")),
    };
    Ok(bformat!("{name}@anonymous"))
}
