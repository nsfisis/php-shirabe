//! Rust-to-PHP RPC over a Unix domain socket. See `docs/dev/php-rpc.md`.

use anyhow::Context as _;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::process::PhpExecutableFinder;
use shirabe_php_shim::PhpMixed;
use std::io::{Read as _, Write as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// PHP `\PHP_VERSION`.
pub fn get_php_version() -> String {
    match get_constant("PHP_VERSION") {
        PhpMixed::String(s) => s,
        other => panic!("PHP RPC: PHP_VERSION constant did not resolve to a string: {other:?}"),
    }
}

/// PHP `\PHP_BINARY`.
pub fn get_php_binary() -> String {
    match get_constant("PHP_BINARY") {
        PhpMixed::String(s) => s,
        other => panic!("PHP RPC: PHP_BINARY constant did not resolve to a string: {other:?}"),
    }
}

/// PHP `defined($name)`.
pub fn has_constant(name: &str) -> bool {
    match call("defined", name) {
        PhpMixed::Bool(b) => b,
        other => panic!("PHP RPC: `defined` did not return a bool: {other:?}"),
    }
}

/// PHP `constant($name)`.
pub fn get_constant(name: &str) -> PhpMixed {
    call("constant", name)
}

/// PHP `inet_pton($address)`.
pub fn inet_pton(address: &str) -> PhpMixed {
    call("inet_pton", address)
}

/// PHP `curl_version()['version']`.
pub fn curl_version() -> Option<String> {
    match call("curl_version", "") {
        PhpMixed::String(s) => Some(s),
        PhpMixed::Null => None,
        other => panic!("PHP RPC: `curl_version` returned an unexpected value: {other:?}"),
    }
}

/// PHP `(new \ReflectionExtension($name))->info()` output.
pub fn get_extension_info(name: &str) -> String {
    match call("extension_info", name) {
        PhpMixed::String(s) => s,
        other => panic!("PHP RPC: `extension_info` did not return a string: {other:?}"),
    }
}

/// `curl_version()`, together with the `CURL_*` constants the `diagnose` command consults. Every
/// `Option` field is `None` when the corresponding array key or constant is absent.
#[derive(Debug)]
pub struct Curl {
    pub version: String,
    pub libz_version: Option<String>,
    pub brotli_version: Option<String>,
    pub ssl_version: Option<String>,
    pub features: Option<i64>,
    pub version_zstd: Option<i64>,
    pub version_http2: Option<i64>,
    pub has_http_version_2_0: bool,
    pub version_http3: Option<i64>,
}

/// Everything the `diagnose` command needs to know about the PHP runtime, fetched in a single
/// round trip because the command would otherwise probe the same runtime dozens of times.
#[derive(Debug)]
pub struct Diagnostics {
    pub php_version: String,
    pub php_version_id: i64,
    /// `None` when `PHP_BINARY` is undefined.
    pub php_binary: Option<String>,
    /// `None` when `OPENSSL_VERSION_TEXT` is undefined.
    pub openssl_version_text: Option<String>,
    /// `0` when `OPENSSL_VERSION_NUMBER` is undefined.
    pub openssl_version_number: i64,
    pub has_hhvm_version: bool,
    pub has_php_windows_version_build: bool,
    /// `Composer\XdebugHandler\XdebugHandler::isXdebugActive()`.
    pub xdebug_active: bool,
    /// `0` when the ionCube loader is not loaded.
    pub ioncube_loader_iversion: i64,
    /// Empty when the ionCube loader is not loaded.
    pub ioncube_loader_version: String,
    /// `phpinfo(INFO_GENERAL)` output, captured via `ob_start()`/`ob_get_clean()`.
    pub phpinfo_general: String,
    /// `None` when the curl extension is not loaded.
    pub curl: Option<Curl>,
    extensions: IndexMap<String, bool>,
    functions: IndexMap<String, bool>,
    ini_settings: IndexMap<String, Option<String>>,
}

impl Diagnostics {
    /// PHP `extension_loaded($name)`. Only the extensions the worker probes can be asked about;
    /// any other name is a bug in the caller, not a missing extension.
    pub fn extension_loaded(&self, name: &str) -> bool {
        *self.extensions.get(name).unwrap_or_else(|| {
            panic!("PHP RPC: extension `{name}` is not probed by the diagnose payload")
        })
    }

    /// PHP `function_exists($name)`. See [`Diagnostics::extension_loaded`] for the fixed probe set.
    pub fn function_exists(&self, name: &str) -> bool {
        *self.functions.get(name).unwrap_or_else(|| {
            panic!("PHP RPC: function `{name}` is not probed by the diagnose payload")
        })
    }

    /// PHP `ini_get($option)`, with PHP's `false` (no such setting) mapped to `None`. See
    /// [`Diagnostics::extension_loaded`] for the fixed probe set.
    pub fn ini_get(&self, option: &str) -> Option<&str> {
        self.ini_settings
            .get(option)
            .unwrap_or_else(|| {
                panic!("PHP RPC: ini setting `{option}` is not probed by the diagnose payload")
            })
            .as_deref()
    }
}

static DIAGNOSTICS: OnceLock<Diagnostics> = OnceLock::new();

/// PHP runtime information for the `diagnose` command. The worker is queried once per process;
/// subsequent calls reuse the cached payload.
pub fn get_diagnostics() -> &'static Diagnostics {
    DIAGNOSTICS.get_or_init(|| {
        let payload = call("diagnose", "");
        let payload = payload
            .as_array()
            .unwrap_or_else(|| panic!("PHP RPC: `diagnose` did not return an array: {payload:?}"));

        Diagnostics {
            php_version: string_field(payload, "php_version"),
            php_version_id: int_field(payload, "php_version_id"),
            php_binary: nullable_string_field(payload, "php_binary"),
            openssl_version_text: nullable_string_field(payload, "openssl_version_text"),
            openssl_version_number: int_field(payload, "openssl_version_number"),
            has_hhvm_version: bool_field(payload, "has_hhvm_version"),
            has_php_windows_version_build: bool_field(payload, "has_php_windows_version_build"),
            xdebug_active: bool_field(payload, "xdebug_active"),
            ioncube_loader_iversion: int_field(payload, "ioncube_loader_iversion"),
            ioncube_loader_version: string_field(payload, "ioncube_loader_version"),
            phpinfo_general: string_field(payload, "phpinfo_general"),
            curl: curl_field(payload, "curl"),
            extensions: map_field(payload, "extensions")
                .iter()
                .map(|(name, value)| (name.clone(), as_bool(value, name)))
                .collect(),
            functions: map_field(payload, "functions")
                .iter()
                .map(|(name, value)| (name.clone(), as_bool(value, name)))
                .collect(),
            ini_settings: map_field(payload, "ini")
                .iter()
                .map(|(name, value)| (name.clone(), as_nullable_string(value, name)))
                .collect(),
        }
    })
}

fn field<'a>(payload: &'a IndexMap<String, PhpMixed>, key: &str) -> &'a PhpMixed {
    payload
        .get(key)
        .unwrap_or_else(|| panic!("PHP RPC: `diagnose` payload has no `{key}` entry"))
}

fn string_field(payload: &IndexMap<String, PhpMixed>, key: &str) -> String {
    match field(payload, key) {
        PhpMixed::String(s) => s.clone(),
        other => panic!("PHP RPC: `diagnose` payload entry `{key}` is not a string: {other:?}"),
    }
}

fn nullable_string_field(payload: &IndexMap<String, PhpMixed>, key: &str) -> Option<String> {
    as_nullable_string(field(payload, key), key)
}

fn int_field(payload: &IndexMap<String, PhpMixed>, key: &str) -> i64 {
    match field(payload, key) {
        PhpMixed::Int(n) => *n,
        other => panic!("PHP RPC: `diagnose` payload entry `{key}` is not an int: {other:?}"),
    }
}

fn bool_field(payload: &IndexMap<String, PhpMixed>, key: &str) -> bool {
    as_bool(field(payload, key), key)
}

fn map_field<'a>(
    payload: &'a IndexMap<String, PhpMixed>,
    key: &str,
) -> &'a IndexMap<String, PhpMixed> {
    match field(payload, key) {
        PhpMixed::Array(map) => map,
        other => panic!("PHP RPC: `diagnose` payload entry `{key}` is not an array: {other:?}"),
    }
}

fn as_bool(value: &PhpMixed, key: &str) -> bool {
    match value {
        PhpMixed::Bool(b) => *b,
        other => panic!("PHP RPC: `diagnose` payload entry `{key}` is not a bool: {other:?}"),
    }
}

fn nullable_int_field(payload: &IndexMap<String, PhpMixed>, key: &str) -> Option<i64> {
    match field(payload, key) {
        PhpMixed::Int(n) => Some(*n),
        PhpMixed::Null => None,
        other => {
            panic!("PHP RPC: `diagnose` payload entry `{key}` is not an int or null: {other:?}")
        }
    }
}

fn curl_field(payload: &IndexMap<String, PhpMixed>, key: &str) -> Option<Curl> {
    let curl = match field(payload, key) {
        PhpMixed::Null => return None,
        PhpMixed::Array(map) => map,
        other => {
            panic!("PHP RPC: `diagnose` payload entry `{key}` is not an array or null: {other:?}")
        }
    };

    Some(Curl {
        version: string_field(curl, "version"),
        libz_version: nullable_string_field(curl, "libz_version"),
        brotli_version: nullable_string_field(curl, "brotli_version"),
        ssl_version: nullable_string_field(curl, "ssl_version"),
        features: nullable_int_field(curl, "features"),
        version_zstd: nullable_int_field(curl, "version_zstd"),
        version_http2: nullable_int_field(curl, "version_http2"),
        has_http_version_2_0: bool_field(curl, "has_http_version_2_0"),
        version_http3: nullable_int_field(curl, "version_http3"),
    })
}

fn as_nullable_string(value: &PhpMixed, key: &str) -> Option<String> {
    match value {
        PhpMixed::String(s) => Some(s.clone()),
        PhpMixed::Null => None,
        other => {
            panic!("PHP RPC: `diagnose` payload entry `{key}` is not a string or null: {other:?}")
        }
    }
}

/// PHP `phpversion($extension)`.
pub fn phpversion(extension: &str) -> Option<String> {
    match call("phpversion", extension) {
        PhpMixed::String(s) => Some(s),
        PhpMixed::Bool(false) => None,
        other => panic!("PHP RPC: `phpversion` returned an unexpected value: {other:?}"),
    }
}

/// PHP `get_loaded_extensions()`.
pub fn get_loaded_extensions() -> Vec<String> {
    string_list(call("get_loaded_extensions", ""), "get_loaded_extensions")
}

/// `Composer\XdebugHandler\XdebugHandler::getAllIniFiles()` (minus the `self::$name` branch,
/// which is unreachable since this port never constructs an XdebugHandler): `[(string)
/// php_ini_loaded_file()]` merged with the trimmed, comma-split `php_ini_scanned_files()` list
/// when scanning is active.
pub fn get_all_ini_files() -> Vec<String> {
    string_list(call("get_all_ini_files", ""), "get_all_ini_files")
}

fn string_list(value: PhpMixed, name: &str) -> Vec<String> {
    match value {
        PhpMixed::List(items) => items
            .into_iter()
            .map(|item| match item {
                PhpMixed::String(s) => s,
                other => panic!("PHP RPC: `{name}` returned a non-string element: {other:?}"),
            })
            .collect(),
        other => panic!("PHP RPC: `{name}` did not return a list: {other:?}"),
    }
}

const GLUE_SCRIPT: &str = include_str!("../php/worker.php");

struct Worker {
    stream: UnixStream,
    // Also queried for its exit status when a socket read/write fails, to tell a dead worker
    // apart from a framing bug. Kept alive for the process lifetime along with the temp dir
    // holding the socket and glue script; neither is dropped because the worker lives in a
    // never-dropped static.
    child: std::process::Child,
    _tempdir: tempfile::TempDir,
}

impl Worker {
    fn request(&mut self, name: &str, arg: &str) -> anyhow::Result<Vec<u8>> {
        let mut payload = name.as_bytes().to_vec();
        payload.push(0);
        payload.extend_from_slice(arg.as_bytes());
        write_frame(&mut self.stream, &payload).with_context(|| self.worker_state())?;
        read_frame(&mut self.stream).with_context(|| self.worker_state())
    }

    /// Describes the PHP worker's current process state, to be attached as `anyhow::Context` to
    /// an I/O error so a dead worker (crash, OOM kill, ...) can be told apart from a live one
    /// hitting a framing bug.
    fn worker_state(&mut self) -> String {
        match self.child.try_wait() {
            Ok(Some(status)) => format!("PHP worker process already exited: {status}"),
            Ok(None) => format!(
                "PHP worker process (pid {}) is still running",
                self.child.id()
            ),
            Err(wait_err) => format!("failed to check PHP worker process status: {wait_err}"),
        }
    }
}

// TODO(phase-c): every failure here panics rather than propagating a `Result`; this is an interim
// step until PHP RPC gets proper error handling (see docs/dev/php-rpc.md).
static WORKER: LazyLock<Mutex<Worker>> = LazyLock::new(|| {
    Mutex::new(
        spawn_worker().unwrap_or_else(|e| panic!("PHP RPC: failed to spawn PHP worker: {e:#}")),
    )
});

fn call(name: &str, arg: &str) -> PhpMixed {
    let mut guard = WORKER
        .lock()
        .unwrap_or_else(|e| panic!("PHP RPC: worker mutex poisoned: {e}"));
    let payload = guard
        .request(name, arg)
        .unwrap_or_else(|e| panic!("PHP RPC: request `{name}` failed: {e:#}"));
    parse_serialized_value(&payload).unwrap_or_else(|| {
        panic!("PHP RPC: request `{name}` returned an unparseable payload: {payload:?}")
    })
}

fn spawn_worker() -> anyhow::Result<Worker> {
    let php = PhpExecutableFinder::new()
        .find(false)
        .ok_or_else(|| anyhow::anyhow!("no PHP executable found"))?;

    let tempdir = tempfile::tempdir()?;
    let socket_path = tempdir.path().join("rpc.sock");
    let script_path = tempdir.path().join("worker.php");
    std::fs::write(&script_path, GLUE_SCRIPT)?;

    // Bind before spawning so the socket exists when the child connects.
    let listener = UnixListener::bind(&socket_path)?;
    listener.set_nonblocking(true)?;

    let child = std::process::Command::new(&php)
        .arg(&script_path)
        .arg(&socket_path)
        .spawn()?;

    // Poll for the child's connection with a bounded deadline so a child that never connects does
    // not hang the caller.
    let deadline = Instant::now() + Duration::from_secs(10);
    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    anyhow::bail!("timed out waiting for the PHP worker to connect");
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => return Err(e.into()),
        }
    };
    stream.set_nonblocking(false)?;

    Ok(Worker {
        stream,
        child,
        _tempdir: tempdir,
    })
}

fn write_frame(stream: &mut UnixStream, payload: &[u8]) -> std::io::Result<()> {
    stream.write_all(&(payload.len() as u64).to_le_bytes())?;
    stream.write_all(payload)?;
    stream.flush()
}

fn read_frame(stream: &mut UnixStream) -> std::io::Result<Vec<u8>> {
    let mut header = [0u8; 8];
    stream.read_exact(&mut header)?;
    let len = u64::from_le_bytes(header) as usize;
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload)?;
    Ok(payload)
}

/// Parse a whole `serialize()` payload, rejecting trailing garbage.
fn parse_serialized_value(payload: &[u8]) -> Option<PhpMixed> {
    let mut pos = 0;
    let value = parse_value(payload, &mut pos)?;
    (pos == payload.len()).then_some(value)
}

/// Parse one `serialize()` value starting at `pos`, advancing it past the value: `N;`, `b:0/1;`,
/// `i:<n>;`, `d:<f>;`, `s:<len>:"<bytes>";`, `a:<count>:{<key><value>...}`.
fn parse_value(payload: &[u8], pos: &mut usize) -> Option<PhpMixed> {
    let tag = payload.get(*pos..*pos + 2)?;
    *pos += 2;
    match tag {
        b"N;" => Some(PhpMixed::Null),
        b"b:" => match take_until(payload, pos, b';')? {
            b"0" => Some(PhpMixed::Bool(false)),
            b"1" => Some(PhpMixed::Bool(true)),
            _ => None,
        },
        b"i:" => parse_int(take_until(payload, pos, b';')?).map(PhpMixed::Int),
        b"d:" => std::str::from_utf8(take_until(payload, pos, b';')?)
            .ok()?
            .parse()
            .ok()
            .map(PhpMixed::Float),
        b"s:" => parse_string_body(payload, pos).map(PhpMixed::String),
        b"a:" => parse_array_body(payload, pos),
        _ => None,
    }
}

/// Parse the `<len>:"<bytes>";` tail of a serialized string.
fn parse_string_body(payload: &[u8], pos: &mut usize) -> Option<String> {
    let len: usize = std::str::from_utf8(take_until(payload, pos, b':')?)
        .ok()?
        .parse()
        .ok()?;
    if payload.get(*pos) != Some(&b'"') {
        return None;
    }
    *pos += 1;
    let bytes = payload.get(*pos..*pos + len)?;
    *pos += len;
    if payload.get(*pos..*pos + 2) != Some(b"\";") {
        return None;
    }
    *pos += 2;
    Some(String::from_utf8_lossy(bytes).into_owned())
}

/// Parse the `<count>:{<key><value>...}` tail of a serialized array. An array whose keys are
/// exactly `0..count` maps to `PhpMixed::List`, matching how PHP renders such an array as a JSON
/// list; anything else maps to `PhpMixed::Array` with the keys stringified.
fn parse_array_body(payload: &[u8], pos: &mut usize) -> Option<PhpMixed> {
    let count: usize = std::str::from_utf8(take_until(payload, pos, b':')?)
        .ok()?
        .parse()
        .ok()?;
    if payload.get(*pos) != Some(&b'{') {
        return None;
    }
    *pos += 1;

    let mut entries: IndexMap<String, PhpMixed> = IndexMap::new();
    let mut is_list = true;
    for index in 0..count {
        let key = match parse_value(payload, pos)? {
            PhpMixed::Int(n) => {
                is_list &= n == index as i64;
                n.to_string()
            }
            PhpMixed::String(s) => {
                is_list = false;
                s
            }
            _ => return None,
        };
        entries.insert(key, parse_value(payload, pos)?);
    }

    if payload.get(*pos) != Some(&b'}') {
        return None;
    }
    *pos += 1;

    Some(if is_list {
        PhpMixed::List(entries.into_values().collect())
    } else {
        PhpMixed::Array(entries)
    })
}

/// Return the bytes from `pos` up to the next `terminator`, advancing `pos` past it.
fn take_until<'a>(payload: &'a [u8], pos: &mut usize, terminator: u8) -> Option<&'a [u8]> {
    let end = *pos + payload.get(*pos..)?.iter().position(|&b| b == terminator)?;
    let bytes = &payload[*pos..end];
    *pos = end + 1;
    Some(bytes)
}

fn parse_int(bytes: &[u8]) -> Option<i64> {
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_string_scalar() {
        assert_eq!(
            parse_serialized_value(b"s:5:\"8.5.7\";"),
            Some(PhpMixed::String("8.5.7".to_string())),
        );
    }

    #[test]
    fn parses_empty_string() {
        assert_eq!(
            parse_serialized_value(b"s:0:\"\";"),
            Some(PhpMixed::String(String::new())),
        );
    }

    #[test]
    fn parses_string_with_embedded_quote() {
        assert_eq!(
            parse_serialized_value(b"s:3:\"a\"b\";"),
            Some(PhpMixed::String("a\"b".to_string())),
        );
    }

    #[test]
    fn rejects_truncated_string() {
        assert_eq!(parse_serialized_value(b"s:5:\"ab\";"), None);
    }

    #[test]
    fn rejects_trailing_garbage() {
        assert_eq!(parse_serialized_value(b"i:42;i:43;"), None);
    }

    #[test]
    fn request_error_reports_dead_worker_exit_status() {
        let mut worker = spawn_worker().expect("failed to spawn PHP worker");
        worker.child.kill().expect("failed to kill PHP worker");
        worker.child.wait().expect("failed to reap PHP worker");

        let err = worker
            .request("defined", "PHP_VERSION")
            .expect_err("request against a dead worker should fail");
        let message = format!("{err:#}");
        assert!(
            message.contains("PHP worker process already exited"),
            "unexpected error message: {message}"
        );
    }

    #[test]
    fn rejects_non_numeric_length() {
        assert_eq!(parse_serialized_value(b"s:x:\"ab\";"), None);
    }

    #[test]
    fn parses_scalar_null() {
        assert_eq!(parse_serialized_value(b"N;"), Some(PhpMixed::Null));
    }

    #[test]
    fn parses_scalar_bool() {
        assert_eq!(parse_serialized_value(b"b:0;"), Some(PhpMixed::Bool(false)));
        assert_eq!(parse_serialized_value(b"b:1;"), Some(PhpMixed::Bool(true)));
    }

    #[test]
    fn parses_scalar_int() {
        assert_eq!(parse_serialized_value(b"i:8;"), Some(PhpMixed::Int(8)));
        assert_eq!(parse_serialized_value(b"i:-1;"), Some(PhpMixed::Int(-1)));
    }

    #[test]
    fn parses_scalar_float() {
        assert_eq!(
            parse_serialized_value(b"d:1.5;"),
            Some(PhpMixed::Float(1.5))
        );
    }

    #[test]
    fn rejects_malformed_scalar() {
        assert_eq!(parse_serialized_value(b"b:2;"), None);
        assert_eq!(parse_serialized_value(b"i:x;"), None);
        assert_eq!(parse_serialized_value(b"d:x;"), None);
        assert_eq!(parse_serialized_value(b"garbage"), None);
    }

    #[test]
    fn parses_list_array() {
        assert_eq!(
            parse_serialized_value(b"a:2:{i:0;s:1:\"a\";i:1;i:7;}"),
            Some(PhpMixed::List(vec![
                PhpMixed::String("a".to_string()),
                PhpMixed::Int(7),
            ])),
        );
    }

    #[test]
    fn parses_empty_array_as_list() {
        assert_eq!(
            parse_serialized_value(b"a:0:{}"),
            Some(PhpMixed::List(vec![]))
        );
    }

    #[test]
    fn parses_keyed_array() {
        let expected: IndexMap<String, PhpMixed> = [
            ("zip".to_string(), PhpMixed::Bool(true)),
            ("apcu".to_string(), PhpMixed::Null),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            parse_serialized_value(b"a:2:{s:3:\"zip\";b:1;s:4:\"apcu\";N;}"),
            Some(PhpMixed::Array(expected)),
        );
    }

    #[test]
    fn parses_nested_array() {
        let inner: IndexMap<String, PhpMixed> = [("curl".to_string(), PhpMixed::Bool(false))]
            .into_iter()
            .collect();
        let expected: IndexMap<String, PhpMixed> = [
            ("extensions".to_string(), PhpMixed::Array(inner)),
            ("php_version_id".to_string(), PhpMixed::Int(80500)),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            parse_serialized_value(
                b"a:2:{s:10:\"extensions\";a:1:{s:4:\"curl\";b:0;}s:14:\"php_version_id\";i:80500;}"
            ),
            Some(PhpMixed::Array(expected)),
        );
    }

    #[test]
    fn rejects_malformed_array() {
        // Count larger than the number of entries.
        assert_eq!(parse_serialized_value(b"a:2:{i:0;i:1;}"), None);
        // Missing closing brace.
        assert_eq!(parse_serialized_value(b"a:1:{i:0;i:1;"), None);
        // Non-scalar key.
        assert_eq!(parse_serialized_value(b"a:1:{N;i:1;}"), None);
    }

    #[test]
    fn queries_string_lists_when_php_available() {
        if PhpExecutableFinder::new().find(false).is_none() {
            // No PHP in this environment; the worker cannot start.
            return;
        }

        let extensions = get_loaded_extensions();
        assert!(
            extensions.iter().any(|extension| extension == "Core"),
            "expected the Core extension among {extensions:?}",
        );

        // XdebugHandler::getAllIniFiles() always yields at least one entry, which is the empty
        // string when no php.ini is loaded.
        assert!(!get_all_ini_files().is_empty());
    }

    #[test]
    fn queries_diagnostics_when_php_available() {
        if PhpExecutableFinder::new().find(false).is_none() {
            // No PHP in this environment; the worker cannot start.
            return;
        }

        let diagnostics = get_diagnostics();
        assert_eq!(diagnostics.php_version, get_php_version());
        assert!(diagnostics.php_version_id >= 70205);
        let php_binary = get_php_binary();
        assert_eq!(diagnostics.php_binary.as_deref(), Some(php_binary.as_str()));
        assert!(diagnostics.function_exists("json_decode"));
        assert!(!diagnostics.extension_loaded("ionCube Loader"));
        assert!(
            diagnostics.phpinfo_general.contains("PHP Version"),
            "expected phpinfo(INFO_GENERAL) output, got: {}",
            diagnostics.phpinfo_general,
        );
    }

    #[test]
    fn frame_roundtrip() {
        let (mut a, mut b) = UnixStream::pair().unwrap();
        write_frame(&mut a, b"get_php_version").unwrap();
        assert_eq!(read_frame(&mut b).unwrap(), b"get_php_version");
    }

    #[test]
    fn frame_roundtrip_empty_payload() {
        let (mut a, mut b) = UnixStream::pair().unwrap();
        write_frame(&mut a, b"").unwrap();
        assert_eq!(read_frame(&mut b).unwrap(), b"");
    }

    #[test]
    fn queries_real_php_when_available() {
        if PhpExecutableFinder::new().find(false).is_none() {
            // No PHP in this environment; the worker cannot start.
            return;
        }

        let version = get_php_version();
        assert!(!version.is_empty(), "expected a PHP version");
        assert!(
            version
                .split('.')
                .next()
                .and_then(|n| n.parse::<u32>().ok())
                .is_some(),
            "version should start with a number: {version}",
        );

        let binary = get_php_binary();
        assert!(!binary.is_empty(), "expected a PHP binary path");
        assert!(
            std::path::Path::new(&binary).exists(),
            "binary should exist: {binary}",
        );
    }

    #[test]
    fn queries_constants_when_php_available() {
        if PhpExecutableFinder::new().find(false).is_none() {
            // No PHP in this environment; the worker cannot start.
            return;
        }

        assert!(has_constant("PHP_VERSION"));
        assert!(!has_constant("SHIRABE_DOES_NOT_EXIST_XYZ"));

        assert_eq!(get_constant("PHP_INT_SIZE"), PhpMixed::Int(8));
        assert_eq!(get_constant("SHIRABE_DOES_NOT_EXIST_XYZ"), PhpMixed::Null);
        match get_constant("PHP_VERSION") {
            PhpMixed::String(s) => assert!(!s.is_empty(), "expected a non-empty PHP_VERSION"),
            other => panic!("expected a string, got {other:?}"),
        }
    }
}
