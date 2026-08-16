//! Propagation of environment mutations into the PHP worker.
//!
//! The worker is a long-lived child: it holds the environment it was handed at spawn, so a
//! `putenv()` or a `$_ENV`/`$_SERVER` write the Rust side makes afterwards is invisible to the
//! PHP code running in it — `@putenv` in a `scripts` entry, the bin dir the event dispatcher
//! prepends to `PATH`, `COMPOSER_DEV_MODE`. The shim records every such write; this module
//! replays the ones the worker has not seen yet, before the next call crosses the boundary.
//!
//! TODO(php-runtime): the reverse direction is missing — a write PHP code makes to its own
//! environment is not reflected back into the Rust-side storages.

use crate::PluginValue;
use shirabe_php_shim::EnvStorageKind;
use std::os::unix::ffi::OsStrExt as _;

/// How many recorded mutations the worker has already been told about.
static SYNCED: std::sync::Mutex<usize> = std::sync::Mutex::new(0);

/// The name `__shirabe_sync_env` reads each mutation's target storage under.
fn storage_name(storage: EnvStorageKind) -> &'static str {
    match storage {
        EnvStorageKind::Process => "process",
        EnvStorageKind::Env => "_ENV",
        EnvStorageKind::Server => "_SERVER",
    }
}

/// Replays the environment mutations recorded since the last flush into the worker.
///
/// Called from the outermost `rpc_call` only, so the `__shirabe_sync_env` call it issues re-enters
/// the session instead of recursing into another flush.
pub(crate) fn flush() -> anyhow::Result<()> {
    let mut synced = SYNCED.lock().unwrap();
    let (cursor, mutations) = shirabe_php_shim::env_mutations_since(*synced);
    if mutations.is_empty() {
        return Ok(());
    }

    let mutations = mutations
        .into_iter()
        .map(|mutation| {
            PluginValue::List(vec![
                PluginValue::string(storage_name(mutation.storage)),
                PluginValue::String(mutation.key.as_bytes().to_vec()),
                match mutation.value {
                    Some(value) => PluginValue::String(value.as_bytes().to_vec()),
                    None => PluginValue::Null,
                },
            ])
        })
        .collect();
    let outcome = crate::call_function("__shirabe_sync_env", vec![PluginValue::List(mutations)])?;
    // The worker only writes the three storages; nothing it does there can throw.
    outcome.unwrap_or_else(|throw| panic!("PHP RPC: __shirabe_sync_env threw: {}", throw.message));
    *synced = cursor;
    Ok(())
}
