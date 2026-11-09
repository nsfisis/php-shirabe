//! PHP builtins the worker answers without calling back into Rust.

pub mod rar;

use crate::transport::call_internal_function;
use crate::value::PluginValue;
use crate::wire::{FromWire, ToWire, protocol_value};
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_shim::{bformat, path_from_bytes};
use std::sync::OnceLock;

/// The version constants of the PHP the worker runs.
#[derive(Debug)]
pub struct PhpVersion {
    /// `\PHP_VERSION`.
    pub version: BString,
    /// `\PHP_VERSION_ID`.
    pub version_id: i64,
    /// `\PHP_MAJOR_VERSION`.
    pub major: i64,
    /// `\PHP_MINOR_VERSION`.
    pub minor: i64,
    /// `\PHP_RELEASE_VERSION`.
    pub release: i64,
}

static PHP_VERSION: OnceLock<PhpVersion> = OnceLock::new();

/// The PHP version the worker runs. The worker is queried once per process; subsequent calls
/// reuse the cached constants.
pub fn get_php_version() -> anyhow::Result<&'static PhpVersion> {
    cached(&PHP_VERSION, || {
        Ok(PhpVersion {
            version: constant("PHP_VERSION")?,
            version_id: constant("PHP_VERSION_ID")?,
            major: constant("PHP_MAJOR_VERSION")?,
            minor: constant("PHP_MINOR_VERSION")?,
            release: constant("PHP_RELEASE_VERSION")?,
        })
    })
}

/// PHP `\PHP_BINARY`.
pub fn get_php_binary() -> anyhow::Result<std::path::PathBuf> {
    constant("PHP_BINARY")
}

/// PHP `constant($name)`.
fn constant<T: FromWire>(name: impl AsRef<[u8]>) -> anyhow::Result<T> {
    query("__shirabe_constant", name.as_ref().as_bstr())
}

/// The value `cell` caches, computing it with `init` on first use. A failed `init` caches
/// nothing, so the next call asks again.
pub fn cached<T>(
    cell: &'static OnceLock<T>,
    init: impl FnOnce() -> anyhow::Result<T>,
) -> anyhow::Result<&'static T> {
    if let Some(value) = cell.get() {
        return Ok(value);
    }
    let value = init()?;
    Ok(cell.get_or_init(|| value))
}

/// Runs a query against the worker: a PHP function, or an entry point answering one consumer's
/// questions about the runtime. A failure to reach the worker and an exception the call throws are
/// errors; an answer that does not decode as `T` is a protocol violation, because only the
/// worker's own code or a PHP builtin with a fixed return type writes it.
pub fn query<T: FromWire>(name: impl AsRef<[u8]>, request: impl ToWire) -> anyhow::Result<T> {
    let name = name.as_ref().as_bstr();
    let value = call_internal_function(name, vec![request.to_wire()])??;
    Ok(protocol_value(&value, bformat!("the answer of {name}")))
}

/// An answer typed `T|false`, with `false` mapped to `None`. Any other answer that does not decode
/// as `T` is a protocol violation, as for [`query`].
pub(crate) fn or_false<T: FromWire>(value: &PluginValue, what: impl AsRef<[u8]>) -> Option<T> {
    match value {
        PluginValue::Bool(false) => None,
        value => Some(protocol_value(value, what)),
    }
}

/// A PHP function answering `string|false`, with `false` mapped to `None`.
fn string_or_false(name: impl AsRef<[u8]>, arg: &BStr) -> anyhow::Result<Option<BString>> {
    let name = name.as_ref().as_bstr();
    let value: PluginValue = query(name, arg)?;
    Ok(or_false(&value, bformat!("the answer of {name}")))
}

/// PHP `phpversion($extension)`.
pub fn phpversion(extension: impl AsRef<[u8]>) -> anyhow::Result<Option<BString>> {
    string_or_false("phpversion", extension.as_ref().as_bstr())
}

/// PHP `memory_get_usage($real_usage)`, measured in the worker. The memory of Shirabe's own
/// process is not included.
pub fn memory_get_usage(real_usage: bool) -> anyhow::Result<i64> {
    query("memory_get_usage", real_usage)
}

/// PHP `memory_get_peak_usage($real_usage)`, measured in the worker. The memory of Shirabe's own
/// process is not included.
pub fn memory_get_peak_usage(real_usage: bool) -> anyhow::Result<i64> {
    query("memory_get_peak_usage", real_usage)
}

/// PHP `class_exists($class)`, autoloading through the worker's autoloaders.
pub fn class_exists(class: impl AsRef<[u8]>) -> anyhow::Result<bool> {
    query("class_exists", class.as_ref().as_bstr())
}

/// PHP `stream_resolve_include_path($filename)`. It is answered by the worker because the
/// `include_path` it searches is the worker's own, including whatever a required `autoload_real.php`
/// put there with `set_include_path()`.
pub fn stream_resolve_include_path(
    filename: impl AsRef<[u8]>,
) -> anyhow::Result<Option<std::path::PathBuf>> {
    let resolved = string_or_false("stream_resolve_include_path", filename.as_ref().as_bstr())?;
    Ok(resolved.map(|s| path_from_bytes(&s).to_path_buf()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shirabe_php_shim::utf8_text;

    #[test]
    fn queries_memory_usage() {
        for real_usage in [false, true] {
            let usage = memory_get_usage(real_usage).unwrap();
            assert!(usage > 0);
            assert!(memory_get_peak_usage(real_usage).unwrap() >= usage);
        }
    }

    #[test]
    fn queries_real_php() {
        let php = get_php_version().unwrap();
        assert!(!php.version.is_empty(), "expected a PHP version");
        assert_eq!(
            php.version
                .split_str(".")
                .next()
                .and_then(|n| utf8_text(n)?.parse::<i64>().ok()),
            Some(php.major),
            "version should start with the major version: {}",
            php.version.display_lossy(),
        );
        assert_eq!(
            php.version_id,
            php.major * 10000 + php.minor * 100 + php.release
        );

        let binary = get_php_binary().unwrap();
        assert!(!binary.as_os_str().is_empty(), "expected a PHP binary path");
        assert!(
            std::path::Path::new(&binary).exists(),
            "binary should exist: {}",
            binary.display(),
        );
    }
}
