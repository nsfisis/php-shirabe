//! `Composer\InstalledVersions` lives in the PHP worker, where the code that reads it (plugins and
//! scripts) runs: the state `FilesystemRepository` writes is pushed there.

use crate::php_worker::call::PhpCall;
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_rpc::{PluginValue, ToPluginValue};
use shirabe_php_shim::{PhpMixed, path_to_bytes};

/// The `installed.php` passed to [`safely_load`], waiting for the Composer PHP runtime to be
/// loaded into the PHP worker.
static PENDING_INSTALLED_VERSIONS: std::sync::Mutex<Option<std::path::PathBuf>> =
    std::sync::Mutex::new(None);

/// Defers validating and loading `installed.php` into the worker's `InstalledVersions` until the
/// Composer PHP runtime is loaded there (see [`load_pending`]). Only the first path is kept, as
/// Composer loads the file only while `InstalledVersions` is not defined yet.
pub(crate) fn safely_load(path: &std::path::Path) {
    let mut pending = PENDING_INSTALLED_VERSIONS
        .lock()
        .unwrap_or_else(|e| panic!("pending installed.php mutex poisoned: {e}"));
    if pending.is_none() {
        *pending = Some(path.to_path_buf());
    }
}

/// Runs the deferred [`safely_load`] in the PHP worker. Called right after the Composer PHP
/// runtime is loaded there, before any plugin or script code runs.
pub(crate) fn load_pending() -> anyhow::Result<()> {
    let path = PENDING_INSTALLED_VERSIONS
        .lock()
        .unwrap_or_else(|e| panic!("pending installed.php mutex poisoned: {e}"))
        .take();
    if let Some(path) = path {
        PhpCall::Function(BStr::new("__shirabe_safely_load_installed_versions"))
            .run::<PluginValue>(vec![PluginValue::string(path_to_bytes(&path))])?;
    }
    Ok(())
}

/// Reloads the worker's `InstalledVersions` with `versions`, the data just written to
/// `{repo_dir}/installed.php`. Skipped when no worker is running: with no child there is nothing
/// that could observe the state. The worker side skips it only when the class is not even
/// autoloadable there (no Composer PHP runtime = no observer code either).
pub(crate) fn reload(
    versions: IndexMap<BString, PhpMixed>,
    repo_dir: &std::path::Path,
) -> anyhow::Result<()> {
    if !shirabe_php_rpc::worker_is_running() {
        return Ok(());
    }
    PhpCall::Function(BStr::new("__shirabe_installed_versions_reload")).run::<PluginValue>(
        vec![
            PhpMixed::Array(versions).to_plugin_value(),
            PluginValue::string(path_to_bytes(repo_dir)),
        ],
    )?;
    Ok(())
}
