//! Rust-to-PHP RPC over a Unix domain socket. See `docs/dev/php-rpc.md`.

pub mod frame;
pub mod session;
pub mod value;

pub use value::{PhpClassHandle, PhpObjHandle, PluginValue, RustObjHandle};

use frame::Frame;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::process::PhpExecutableFinder;
use shirabe_php_shim::PhpMixed;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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

/// A PHP exception that crossed the RPC boundary (the recoverable failure lane, as opposed to
/// the fatal `anyhow::Error` lane used for a dead worker or a broken channel).
#[derive(Debug, Clone)]
pub struct PhpThrow {
    pub exception_class: String,
    pub message: String,
    pub code: i64,
}

impl PhpThrow {
    fn runtime(message: String) -> PhpThrow {
        PhpThrow {
            exception_class: "RuntimeException".to_string(),
            message,
            code: 0,
        }
    }
}

impl std::fmt::Display for PhpThrow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.exception_class, self.message)
    }
}

impl std::error::Error for PhpThrow {}

/// Handles `CallRustMethod` requests arriving while a Rust-initiated call is waiting for its
/// `Return` (the cooperative reentrancy loop). A handler may itself issue nested RPC calls.
pub trait RustMethodDispatcher {
    fn dispatch(
        &mut self,
        rhandle: u64,
        method_name: &str,
        args: Vec<PluginValue>,
        out_param_positions: &[u32],
    ) -> Result<PluginValue, PhpThrow>;
}

/// The Rust side allocates odd correlation ids; the PHP side allocates even ones. Calls nest
/// strictly, so this split is not needed for disambiguation — it just makes any captured frame
/// attributable to its initiator.
static NEXT_CORR_ID: AtomicU64 = AtomicU64::new(1);

/// Allocates a Rust-side object handle. Handle 0 is reserved for the runtime service endpoint
/// (e.g. `__shirabe_find_file` autoload queries), so ids start at 1.
static NEXT_RHANDLE: AtomicU64 = AtomicU64::new(1);

thread_local! {
    /// Listener for ReleaseRustHandle notifications, thread-local like the R table it prunes.
    static RELEASE_RUST_HANDLE_HOOK: std::cell::RefCell<Option<Box<dyn Fn(u64)>>> =
        const { std::cell::RefCell::new(None) };
}

/// Registers the listener invoked with the released handle whenever a ReleaseRustHandle
/// notification arrives on this thread. Replaces any previously registered listener.
pub fn set_release_rust_handle_hook(hook: impl Fn(u64) + 'static) {
    RELEASE_RUST_HANDLE_HOOK.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(hook));
    });
}

pub fn alloc_rhandle() -> u64 {
    NEXT_RHANDLE.fetch_add(1, Ordering::Relaxed)
}

/// Calls a PHP function in the worker. The outer `Result` is the fatal lane (dead worker, broken
/// framing); the inner one carries a PHP exception if the call threw.
pub fn call_function(
    name: &str,
    args: Vec<PluginValue>,
) -> anyhow::Result<Result<PluginValue, PhpThrow>> {
    call_function_with_dispatcher(name, args, None)
}

pub fn call_function_with_dispatcher(
    name: &str,
    args: Vec<PluginValue>,
    dispatcher: Option<&mut dyn RustMethodDispatcher>,
) -> anyhow::Result<Result<PluginValue, PhpThrow>> {
    rpc_call(
        |corr_id| Frame::CallFunction {
            corr_id,
            function_name: name.to_string(),
            args,
            out_param_positions: Vec::new(),
        },
        dispatcher,
    )
}

/// Calls `$class::$method(...$args)` in the worker (autoloading the class if needed).
pub fn call_static_method(
    class: &str,
    method: &str,
    args: Vec<PluginValue>,
    dispatcher: Option<&mut dyn RustMethodDispatcher>,
) -> anyhow::Result<Result<PluginValue, PhpThrow>> {
    rpc_call(
        |corr_id| Frame::CallStaticMethod {
            corr_id,
            pclass: class.to_string(),
            method_name: method.to_string(),
            args,
            out_param_positions: Vec::new(),
        },
        dispatcher,
    )
}

/// Instantiates `new $class(...$ctor_args)` in the worker (autoloading the class if needed).
/// On success the returned value is a `PluginValue::PhpHandle` registered in the worker's P
/// table; the entity stays alive there until `release_php_handle`.
pub fn new_object(
    class: &str,
    ctor_args: Vec<PluginValue>,
    dispatcher: Option<&mut dyn RustMethodDispatcher>,
) -> anyhow::Result<Result<PluginValue, PhpThrow>> {
    rpc_call(
        |corr_id| Frame::NewObject {
            corr_id,
            pclass: class.to_string(),
            ctor_args,
        },
        dispatcher,
    )
}

/// Calls `$obj->$method(...$args)` on a P-table entity in the worker.
pub fn call_php_method(
    phandle: u64,
    method: &str,
    args: Vec<PluginValue>,
    dispatcher: Option<&mut dyn RustMethodDispatcher>,
) -> anyhow::Result<Result<PluginValue, PhpThrow>> {
    rpc_call(
        |corr_id| Frame::CallPhpMethod {
            corr_id,
            phandle,
            method_name: method.to_string(),
            args,
            out_param_positions: Vec::new(),
        },
        dispatcher,
    )
}

/// One-way notification dropping a P-table entity in the worker. Callers releasing from a
/// destructor should ignore the error: a dead worker has nothing left to release.
pub fn release_php_handle(phandle: u64) -> anyhow::Result<()> {
    let _session = session::SessionGuard::enter();
    send_frame(&Frame::ReleasePhpHandle { phandle })
}

/// Whether the PHP worker process has been spawned by this process. Callers that only need to
/// mirror state into an already-running worker (e.g. `InstalledVersions::reload` pushes) use
/// this to avoid spawning a worker that would have nothing to observe.
pub fn worker_is_running() -> bool {
    WORKER_SPAWNED.load(Ordering::SeqCst)
}

fn rpc_call(
    request: impl FnOnce(u64) -> Frame,
    mut dispatcher: Option<&mut dyn RustMethodDispatcher>,
) -> anyhow::Result<Result<PluginValue, PhpThrow>> {
    // Held for the whole logical call session; nested calls from the same thread (issued by a
    // dispatcher handler) re-enter immediately, other threads are serialized.
    let _session = session::SessionGuard::enter();
    let my_id = NEXT_CORR_ID.fetch_add(2, Ordering::Relaxed);
    send_frame(&request(my_id))?;
    loop {
        let incoming = recv_frame()?;
        match incoming {
            Frame::Return { corr_id, value, .. } if corr_id == my_id => {
                return Ok(Ok(value));
            }
            Frame::Throw {
                corr_id,
                exception_class,
                message,
                code,
            } if corr_id == my_id => {
                return Ok(Err(PhpThrow {
                    exception_class,
                    message,
                    code,
                }));
            }
            Frame::CallRustMethod {
                corr_id,
                rhandle,
                method_name,
                args,
                out_param_positions,
            } => {
                let outcome = match dispatcher.as_deref_mut() {
                    Some(dispatcher) => {
                        dispatcher.dispatch(rhandle, &method_name, args, &out_param_positions)
                    }
                    // Never fall back to a silent null: an unroutable callback is reported as an
                    // explicit error on the PHP side.
                    None => Err(PhpThrow::runtime(format!(
                        "no Rust method dispatcher is active for this call \
                         (rhandle {rhandle}, method `{method_name}`)"
                    ))),
                };
                let reply = match outcome {
                    Ok(value) => Frame::Return {
                        corr_id,
                        value,
                        out_params: IndexMap::new(),
                    },
                    Err(throw) => Frame::Throw {
                        corr_id,
                        exception_class: throw.exception_class,
                        message: throw.message,
                        code: throw.code,
                    },
                };
                send_frame(&reply)?;
            }
            Frame::ReleaseRustHandle { rhandle } => {
                // A one-way notification sent by a child-side stub's __destruct; the shirabe
                // crate registers a hook that drops the matching R-table entry. Per-call
                // script-event handles carry no table state, so an unhooked release is a no-op.
                RELEASE_RUST_HANDLE_HOOK.with(|hook| {
                    if let Some(hook) = hook.borrow().as_ref() {
                        hook(rhandle);
                    }
                });
                continue;
            }
            Frame::EpochBump { .. } => {
                // Rust is the sender of epoch bumps; tolerate the symmetric direction.
                continue;
            }
            other => panic!(
                "PHP RPC: protocol violation — unexpected frame while waiting for corr_id \
                 {my_id}: {other:?}"
            ),
        }
    }
}

fn call(name: &str, arg: &str) -> PhpMixed {
    let outcome = call_function(name, vec![PluginValue::string(arg)])
        .unwrap_or_else(|e| panic!("PHP RPC: request `{name}` failed: {e:#}"));
    let value = match outcome {
        Ok(value) => value,
        Err(throw) => panic!("PHP RPC: request `{name}` threw {throw}"),
    };
    value
        .to_php_mixed()
        .unwrap_or_else(|e| panic!("PHP RPC: request `{name}` returned an unusable value: {e:#}"))
}

const GLUE_SCRIPT: &str = include_str!("../php/worker.php");

/// Proxy stub classes made autoloadable inside the worker. Generated by
/// `scripts/plugin-stub-generator/generate-stubs`; this list must cover its targets.list (the
/// generator's `--check` mode verifies both the file contents and this list).
const STUB_FILES: &[(&str, &str)] = &[
    (
        "Composer/EventDispatcher/Event.php",
        include_str!("../php/stubs/Composer/EventDispatcher/Event.php"),
    ),
    (
        "Composer/Script/Event.php",
        include_str!("../php/stubs/Composer/Script/Event.php"),
    ),
    (
        "Composer/PartialComposer.php",
        include_str!("../php/stubs/Composer/PartialComposer.php"),
    ),
    (
        "Composer/Composer.php",
        include_str!("../php/stubs/Composer/Composer.php"),
    ),
    (
        "Composer/IO/BaseIO.php",
        include_str!("../php/stubs/Composer/IO/BaseIO.php"),
    ),
    (
        "Composer/IO/ConsoleIO.php",
        include_str!("../php/stubs/Composer/IO/ConsoleIO.php"),
    ),
    (
        "Composer/IO/BufferIO.php",
        include_str!("../php/stubs/Composer/IO/BufferIO.php"),
    ),
    (
        "Composer/IO/NullIO.php",
        include_str!("../php/stubs/Composer/IO/NullIO.php"),
    ),
];

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

// TODO(phase-c): a failed spawn panics rather than propagating a `Result`; this is an interim
// step until PHP RPC gets proper error handling (see docs/dev/php-rpc.md).
static WORKER: LazyLock<Mutex<Worker>> = LazyLock::new(|| {
    let worker =
        spawn_worker().unwrap_or_else(|e| panic!("PHP RPC: failed to spawn PHP worker: {e:#}"));
    WORKER_SPAWNED.store(true, Ordering::SeqCst);
    Mutex::new(worker)
});

static WORKER_SPAWNED: AtomicBool = AtomicBool::new(false);

/// Writes one frame while holding the worker mutex only for the duration of the write, so the
/// session owner (see `session`) can interleave sends and blocking reads without keeping the
/// worker locked across a whole call.
fn send_frame(frame: &Frame) -> anyhow::Result<()> {
    let mut guard = WORKER
        .lock()
        .unwrap_or_else(|e| panic!("PHP RPC: worker mutex poisoned: {e}"));
    let result = frame::write_frame(&mut guard.stream, frame);
    result.map_err(|e| anyhow::Error::new(e).context(guard.worker_state()))
}

fn recv_frame() -> anyhow::Result<Frame> {
    let mut guard = WORKER
        .lock()
        .unwrap_or_else(|e| panic!("PHP RPC: worker mutex poisoned: {e}"));
    let result = frame::read_frame(&mut guard.stream);
    result.map_err(|e| anyhow::Error::new(e).context(guard.worker_state()))
}

fn spawn_worker() -> anyhow::Result<Worker> {
    let php = PhpExecutableFinder::new()
        .find(false)
        .ok_or_else(|| anyhow::anyhow!("no PHP executable found"))?;

    let tempdir = tempfile::tempdir()?;
    let socket_path = tempdir.path().join("rpc.sock");
    let script_path = tempdir.path().join("worker.php");
    std::fs::write(&script_path, GLUE_SCRIPT)?;

    let stubs_dir = tempdir.path().join("stubs");
    for (relative_path, contents) in STUB_FILES {
        let path = stubs_dir.join(relative_path);
        std::fs::create_dir_all(path.parent().expect("stub paths have a parent"))?;
        std::fs::write(&path, contents)?;
    }

    // Bind before spawning so the socket exists when the child connects.
    let listener = UnixListener::bind(&socket_path)?;
    listener.set_nonblocking(true)?;

    // The socket lives in a 0700 temp dir already; restricting the socket file itself makes the
    // protection independent of the directory permission.
    std::fs::set_permissions(
        &socket_path,
        std::os::unix::fs::PermissionsExt::from_mode(0o600),
    )?;

    let child = std::process::Command::new(&php)
        // The Rust-side codec produces the byte representation of the default (and only
        // supported) serialize_precision; pin the child to it in case a distro php.ini overrides
        // the default.
        .arg("-d")
        .arg("serialize_precision=-1")
        .arg(&script_path)
        .arg(&socket_path)
        .arg(&stubs_dir)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_error_reports_dead_worker_exit_status() {
        let mut worker = spawn_worker().expect("failed to spawn PHP worker");
        worker.child.kill().expect("failed to kill PHP worker");
        worker.child.wait().expect("failed to reap PHP worker");

        // Writing may still succeed into the socket buffer; the read is what must fail.
        let _ = frame::write_frame(
            &mut worker.stream,
            &Frame::CallFunction {
                corr_id: 1,
                function_name: "defined".to_string(),
                args: vec![PluginValue::string("PHP_VERSION")],
                out_param_positions: Vec::new(),
            },
        );
        frame::read_frame(&mut worker.stream).expect_err("reading from a dead worker should fail");
        let state = worker.worker_state();
        assert!(
            state.contains("PHP worker process already exited"),
            "unexpected worker state: {state}"
        );
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
