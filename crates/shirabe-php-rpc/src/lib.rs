//! Rust-to-PHP RPC over a Unix domain socket. See `docs/dev/php-rpc.md`.

pub mod composer_runtime;
mod env;
pub mod frame;
pub mod session;
pub mod value;
pub mod xdebug;

pub use value::{PhpClassHandle, PhpObjHandle, PhpObject, PluginValue, RustObjHandle};

use frame::Frame;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;
use shirabe_symfony_process::PhpExecutableFinder;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex, OnceLock};

/// The version constants of the PHP the worker runs.
#[derive(Debug)]
pub struct PhpVersion {
    /// `\PHP_VERSION`.
    pub version: String,
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
pub fn get_php_version() -> &'static PhpVersion {
    PHP_VERSION.get_or_init(|| PhpVersion {
        version: string_constant("PHP_VERSION"),
        version_id: int_constant("PHP_VERSION_ID"),
        major: int_constant("PHP_MAJOR_VERSION"),
        minor: int_constant("PHP_MINOR_VERSION"),
        release: int_constant("PHP_RELEASE_VERSION"),
    })
}

/// PHP `\PHP_BINARY`.
pub fn get_php_binary() -> String {
    string_constant("PHP_BINARY")
}

/// PHP `constant($name)`.
fn get_constant(name: &str) -> PhpMixed {
    call("constant", name)
}

fn string_constant(name: &str) -> String {
    match get_constant(name) {
        PhpMixed::String(s) => s,
        other => panic!("PHP RPC: {name} constant did not resolve to a string: {other:?}"),
    }
}

fn int_constant(name: &str) -> i64 {
    match get_constant(name) {
        PhpMixed::Int(n) => n,
        other => panic!("PHP RPC: {name} constant did not resolve to an int: {other:?}"),
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
/// round trip because the command would otherwise query the same runtime dozens of times.
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
    /// `phpinfo(INFO_GENERAL)` output.
    pub phpinfo_general: String,
    /// `None` when the curl extension is not loaded.
    pub curl: Option<Curl>,
    extensions: IndexMap<String, bool>,
    functions: IndexMap<String, bool>,
    ini_settings: IndexMap<String, Option<String>>,
}

impl Diagnostics {
    /// PHP `extension_loaded($name)`. Only the extensions the worker reports can be asked about;
    /// any other name is a bug in the caller, not a missing extension.
    pub fn extension_loaded(&self, name: &str) -> bool {
        *self.extensions.get(name).unwrap_or_else(|| {
            panic!("PHP RPC: extension `{name}` is not reported by the diagnose payload")
        })
    }

    /// PHP `function_exists($name)`. See [`Diagnostics::extension_loaded`] for the fixed set of names.
    pub fn function_exists(&self, name: &str) -> bool {
        *self.functions.get(name).unwrap_or_else(|| {
            panic!("PHP RPC: function `{name}` is not reported by the diagnose payload")
        })
    }

    /// PHP `ini_get($option)`, with PHP's `false` (no such setting) mapped to `None`. See
    /// [`Diagnostics::extension_loaded`] for the fixed set of names.
    pub fn ini_get(&self, option: &str) -> Option<&str> {
        self.ini_settings
            .get(option)
            .unwrap_or_else(|| {
                panic!("PHP RPC: ini setting `{option}` is not reported by the diagnose payload")
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
        let payload = &Payload {
            what: "diagnose",
            map: payload,
        };

        Diagnostics {
            php_version: payload.string("php_version"),
            php_version_id: payload.int("php_version_id"),
            php_binary: payload.nullable_string("php_binary"),
            openssl_version_text: payload.nullable_string("openssl_version_text"),
            openssl_version_number: payload.int("openssl_version_number"),
            has_hhvm_version: payload.bool("has_hhvm_version"),
            has_php_windows_version_build: payload.bool("has_php_windows_version_build"),
            xdebug_active: payload.bool("xdebug_active"),
            ioncube_loader_iversion: payload.int("ioncube_loader_iversion"),
            ioncube_loader_version: payload.string("ioncube_loader_version"),
            phpinfo_general: payload.string("phpinfo_general"),
            curl: curl_field(payload, "curl"),
            extensions: payload.bool_map("extensions"),
            functions: payload.bool_map("functions"),
            ini_settings: payload
                .entries("ini")
                .map(|(name, value)| (name.clone(), as_nullable_string(value, "ini", name)))
                .collect(),
        }
    })
}

/// The constants the `platform` payload carries, in the worker's order.
const PLATFORM_CONSTANTS: &[&str] = &[
    "PHP_VERSION",
    "PHP_DEBUG",
    "PHP_ZTS",
    "PHP_INT_SIZE",
    "AF_INET6",
    "GD_VERSION",
    "GMP_VERSION",
    "ICONV_VERSION",
    "INTL_ICU_VERSION",
    "LIBXML_DOTTED_VERSION",
    "MB_ONIGURUMA_VERSION",
    "OPENSSL_VERSION_TEXT",
    "PCRE_VERSION",
    "PGSQL_LIBPQ_VERSION",
    "RD_KAFKA_VERSION",
    "SODIUM_LIBRARY_VERSION",
    "LIBXSLT_DOTTED_VERSION",
    "ZipArchive::LIBZIP_VERSION",
    "ZLIB_VERSION",
];

/// The classes the `platform` payload carries the existence of.
const PLATFORM_CLASSES: &[&str] = &["ResourceBundle", "IntlChar"];

/// Everything `PlatformRepository` needs to know about the PHP runtime, fetched in a single round
/// trip because it would otherwise query the same runtime once per extension and constant.
///
/// The fields standing in for PHP objects (`resource_bundle`, `imagick`) carry the entries the
/// consumer reads off them rather than the object itself.
#[derive(Debug, Clone)]
pub struct PlatformInfo {
    extensions: Vec<String>,
    extension_versions: IndexMap<String, String>,
    extension_info: IndexMap<String, String>,
    /// Keyed as `ltrim($class.'::'.$constant, ':')`; `None` for a reported but undefined constant.
    constants: IndexMap<String, Option<PhpMixed>>,
    classes: IndexMap<String, bool>,
    /// `curl_version()`, or null when the curl extension is not loaded.
    pub curl_version: PhpMixed,
    /// `@inet_pton('::')`.
    pub inet_pton_ipv6: PhpMixed,
    /// `['Version' => ResourceBundle::create('root', 'ICUDATA', false)->get('Version')]`, or null
    /// when the class is absent or the bundle cannot be opened.
    pub resource_bundle: PhpMixed,
    /// `IntlChar::getUnicodeVersion()`, or null when the class is absent.
    pub intl_char_unicode_version: PhpMixed,
    /// `(new Imagick())->getVersion()`, or null when the extension is not loaded.
    pub imagick: PhpMixed,
}

impl Default for PlatformInfo {
    /// A runtime with no extensions loaded, no classes defined and every reported constant
    /// undefined.
    fn default() -> Self {
        PlatformInfo {
            extensions: Vec::new(),
            extension_versions: IndexMap::new(),
            extension_info: IndexMap::new(),
            constants: PLATFORM_CONSTANTS
                .iter()
                .map(|name| ((*name).to_string(), None))
                .collect(),
            classes: PLATFORM_CLASSES
                .iter()
                .map(|name| ((*name).to_string(), false))
                .collect(),
            curl_version: PhpMixed::Null,
            inet_pton_ipv6: PhpMixed::Null,
            resource_bundle: PhpMixed::Null,
            intl_char_unicode_version: PhpMixed::Null,
            imagick: PhpMixed::Null,
        }
    }
}

impl PlatformInfo {
    /// PHP `get_loaded_extensions()`.
    pub fn get_extensions(&self) -> &[String] {
        &self.extensions
    }

    /// PHP `phpversion($extension)`, with PHP's `false` mapped to `'0'`.
    pub fn get_extension_version(&self, extension: &str) -> &str {
        self.extension_versions
            .get(extension)
            .unwrap_or_else(|| {
                panic!("PHP RPC: extension `{extension}` is not loaded in the platform payload")
            })
            .as_str()
    }

    /// PHP `(new \ReflectionExtension($extension))->info()` output. Only the extensions the worker
    /// reports can be asked about; any other name is a bug in the caller.
    pub fn get_extension_info(&self, extension: &str) -> &str {
        self.extension_info
            .get(extension)
            .unwrap_or_else(|| {
                panic!("PHP RPC: extension `{extension}` info is not in the platform payload")
            })
            .as_str()
    }

    /// PHP `defined(ltrim($class.'::'.$constant, ':'))`. Only the constants the worker reports can
    /// be asked about; any other name is a bug in the caller, not an undefined constant.
    pub fn has_constant(&self, constant_name: &str, class: Option<&str>) -> bool {
        self.constant(constant_name, class).is_some()
    }

    /// PHP `constant(ltrim($class.'::'.$constant, ':'))`, answering null for an undefined
    /// constant. See [`PlatformInfo::has_constant`] for the fixed set of names.
    pub fn get_constant(&self, constant_name: &str, class: Option<&str>) -> PhpMixed {
        self.constant(constant_name, class)
            .cloned()
            .unwrap_or(PhpMixed::Null)
    }

    /// PHP `class_exists($class, false)`. See [`PlatformInfo::has_constant`] for the fixed set of
    /// names.
    pub fn has_class(&self, class: &str) -> bool {
        *self.classes.get(class).unwrap_or_else(|| {
            panic!("PHP RPC: class `{class}` is not reported by the platform payload")
        })
    }

    /// For testing only: reports `extensions` as loaded, each at `version`.
    pub fn __set_extensions(&mut self, extensions: Vec<String>, version: &str) {
        self.extension_versions = extensions
            .iter()
            .map(|name| (name.clone(), version.to_string()))
            .collect();
        self.extensions = extensions;
    }

    /// For testing only: reports `info` as the `ReflectionExtension::info()` output of `extension`.
    pub fn __set_extension_info(&mut self, extension: &str, info: &str) {
        self.extension_info
            .insert(extension.to_string(), info.to_string());
    }

    /// For testing only: reports the constant as defined with `value`. Panics on a constant the
    /// worker does not report, so a test cannot describe a runtime the worker cannot report.
    pub fn __set_constant(&mut self, constant_name: &str, class: Option<&str>, value: PhpMixed) {
        let key = Self::constant_key(constant_name, class);
        let entry = self.constants.get_mut(&key).unwrap_or_else(|| {
            panic!("PHP RPC: constant `{key}` is not reported by the platform payload")
        });
        *entry = Some(value);
    }

    /// For testing only: reports the class as defined. See [`PlatformInfo::__set_constant`].
    pub fn __set_class(&mut self, class: &str) {
        let entry = self.classes.get_mut(class).unwrap_or_else(|| {
            panic!("PHP RPC: class `{class}` is not reported by the platform payload")
        });
        *entry = true;
    }

    fn constant(&self, constant_name: &str, class: Option<&str>) -> Option<&PhpMixed> {
        let key = Self::constant_key(constant_name, class);
        self.constants
            .get(&key)
            .unwrap_or_else(|| {
                panic!("PHP RPC: constant `{key}` is not reported by the platform payload")
            })
            .as_ref()
    }

    fn constant_key(constant_name: &str, class: Option<&str>) -> String {
        match class {
            Some(class) => format!("{class}::{constant_name}"),
            None => constant_name.to_string(),
        }
    }
}

static PLATFORM_INFO: OnceLock<PlatformInfo> = OnceLock::new();

/// PHP runtime information for `PlatformRepository`. The worker is queried once per process;
/// subsequent calls reuse the cached payload.
pub fn get_platform_info() -> &'static PlatformInfo {
    PLATFORM_INFO.get_or_init(|| {
        let payload = call("platform", "");
        let payload = payload
            .as_array()
            .unwrap_or_else(|| panic!("PHP RPC: `platform` did not return an array: {payload:?}"));
        let payload = &Payload {
            what: "platform",
            map: payload,
        };

        let constant_names = payload.string_list("constant_names");
        assert_eq!(
            constant_names
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            PLATFORM_CONSTANTS,
            "PHP RPC: the worker reports different constants than the platform payload declares"
        );
        let constants = payload.map("constants");
        let classes = payload.bool_map("classes");
        assert_eq!(
            classes.keys().map(String::as_str).collect::<Vec<_>>(),
            PLATFORM_CLASSES,
            "PHP RPC: the worker reports different classes than the platform payload declares"
        );

        PlatformInfo {
            extensions: payload.string_list("extensions"),
            extension_versions: payload.string_map("extension_versions"),
            extension_info: payload.string_map("extension_info"),
            constants: constant_names
                .into_iter()
                .map(|name| {
                    let value = constants.and_then(|map| map.get(&name)).cloned();
                    (name, value)
                })
                .collect(),
            classes,
            curl_version: payload.mixed("curl_version"),
            inet_pton_ipv6: payload.mixed("inet_pton_ipv6"),
            resource_bundle: payload.mixed("resource_bundle"),
            intl_char_unicode_version: payload.mixed("intl_char_unicode_version"),
            imagick: payload.mixed("imagick"),
        }
    })
}

/// One worker answer, named after the dispatch entry that produced it so a decoding failure says
/// which payload was malformed.
struct Payload<'a> {
    what: &'static str,
    map: &'a IndexMap<String, PhpMixed>,
}

impl Payload<'_> {
    fn field(&self, key: &str) -> &PhpMixed {
        self.map
            .get(key)
            .unwrap_or_else(|| panic!("PHP RPC: `{}` payload has no `{key}` entry", self.what))
    }

    fn mixed(&self, key: &str) -> PhpMixed {
        self.field(key).clone()
    }

    fn string(&self, key: &str) -> String {
        match self.field(key) {
            PhpMixed::String(s) => s.clone(),
            other => panic!(
                "PHP RPC: `{}` payload entry `{key}` is not a string: {other:?}",
                self.what
            ),
        }
    }

    fn nullable_string(&self, key: &str) -> Option<String> {
        as_nullable_string(self.field(key), self.what, key)
    }

    fn int(&self, key: &str) -> i64 {
        match self.field(key) {
            PhpMixed::Int(n) => *n,
            other => panic!(
                "PHP RPC: `{}` payload entry `{key}` is not an int: {other:?}",
                self.what
            ),
        }
    }

    fn bool(&self, key: &str) -> bool {
        as_bool(self.field(key), self.what, key)
    }

    /// `None` for PHP's empty array, which carries no key type and so decodes as an empty list.
    fn map(&self, key: &str) -> Option<&IndexMap<String, PhpMixed>> {
        match self.field(key) {
            PhpMixed::Array(map) => Some(map),
            PhpMixed::List(items) if items.is_empty() => None,
            other => panic!(
                "PHP RPC: `{}` payload entry `{key}` is not an array: {other:?}",
                self.what
            ),
        }
    }

    fn entries(&self, key: &str) -> impl Iterator<Item = (&String, &PhpMixed)> {
        self.map(key).into_iter().flatten()
    }

    fn bool_map(&self, key: &str) -> IndexMap<String, bool> {
        self.entries(key)
            .map(|(name, value)| (name.clone(), as_bool(value, self.what, name)))
            .collect()
    }

    fn string_map(&self, key: &str) -> IndexMap<String, String> {
        self.entries(key)
            .map(|(name, value)| match value {
                PhpMixed::String(s) => (name.clone(), s.clone()),
                other => panic!(
                    "PHP RPC: `{}` payload entry `{key}[{name}]` is not a string: {other:?}",
                    self.what
                ),
            })
            .collect()
    }

    fn string_list(&self, key: &str) -> Vec<String> {
        match self.field(key) {
            PhpMixed::List(items) => items
                .iter()
                .map(|item| match item {
                    PhpMixed::String(s) => s.clone(),
                    other => panic!(
                        "PHP RPC: `{}` payload entry `{key}` has a non-string element: {other:?}",
                        self.what
                    ),
                })
                .collect(),
            other => panic!(
                "PHP RPC: `{}` payload entry `{key}` is not a list: {other:?}",
                self.what
            ),
        }
    }
}

fn as_bool(value: &PhpMixed, what: &str, key: &str) -> bool {
    match value {
        PhpMixed::Bool(b) => *b,
        other => panic!("PHP RPC: `{what}` payload entry `{key}` is not a bool: {other:?}"),
    }
}

fn as_nullable_string(value: &PhpMixed, what: &str, key: &str) -> Option<String> {
    match value {
        PhpMixed::String(s) => Some(s.clone()),
        PhpMixed::Null => None,
        other => {
            panic!("PHP RPC: `{what}` payload entry `{key}` is not a string or null: {other:?}")
        }
    }
}

fn nullable_int_field(payload: &Payload, key: &str) -> Option<i64> {
    let what = payload.what;
    match payload.field(key) {
        PhpMixed::Int(n) => Some(*n),
        PhpMixed::Null => None,
        other => {
            panic!("PHP RPC: `{what}` payload entry `{key}` is not an int or null: {other:?}")
        }
    }
}

fn curl_field(payload: &Payload, key: &str) -> Option<Curl> {
    let what = payload.what;
    let curl = match payload.field(key) {
        PhpMixed::Null => return None,
        PhpMixed::Array(map) => map,
        other => {
            panic!("PHP RPC: `{what}` payload entry `{key}` is not an array or null: {other:?}")
        }
    };
    let curl = &Payload { what, map: curl };

    Some(Curl {
        version: curl.string("version"),
        libz_version: curl.nullable_string("libz_version"),
        brotli_version: curl.nullable_string("brotli_version"),
        ssl_version: curl.nullable_string("ssl_version"),
        features: nullable_int_field(curl, "features"),
        version_zstd: nullable_int_field(curl, "version_zstd"),
        version_http2: nullable_int_field(curl, "version_http2"),
        has_http_version_2_0: curl.bool("has_http_version_2_0"),
        version_http3: nullable_int_field(curl, "version_http3"),
    })
}

/// PHP `phpversion($extension)`.
pub fn phpversion(extension: &str) -> Option<String> {
    match call("phpversion", extension) {
        PhpMixed::String(s) => Some(s),
        PhpMixed::Bool(false) => None,
        other => panic!("PHP RPC: `phpversion` returned an unexpected value: {other:?}"),
    }
}

/// PHP `stream_resolve_include_path($filename)`. It is answered by the worker because the
/// `include_path` it searches is the worker's own, including whatever a required `autoload_real.php`
/// put there with `set_include_path()`.
pub fn stream_resolve_include_path(filename: &str) -> Option<String> {
    match call("stream_resolve_include_path", filename) {
        PhpMixed::String(s) => Some(s),
        PhpMixed::Bool(false) => None,
        other => {
            panic!("PHP RPC: `stream_resolve_include_path` returned an unexpected value: {other:?}")
        }
    }
}

/// What `Composer\XdebugHandler\XdebugHandler::getAllIniFiles()` measures: `[(string)
/// php_ini_loaded_file()]` merged with the trimmed, comma-split `php_ini_scanned_files()` list
/// when scanning is active. The worker runs on the ini files of the machine, so these are the
/// user's own.
pub fn get_all_ini_files() -> Vec<String> {
    string_list(call("get_all_ini_files", ""), "get_all_ini_files")
}

/// `Composer\XdebugHandler\XdebugHandler::isXdebugActive()` as measured in the worker, which is
/// the process Xdebug is loaded into.
fn xdebug_active() -> bool {
    match call("xdebug_active", "") {
        PhpMixed::Bool(b) => b,
        other => panic!("PHP RPC: `xdebug_active` did not return a bool: {other:?}"),
    }
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
    let session = session::SessionGuard::enter();
    if session.is_outermost() {
        env::flush()?;
    }
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
        "Composer/EventDispatcher/EventDispatcher.php",
        include_str!("../php/stubs/Composer/EventDispatcher/EventDispatcher.php"),
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
        "Composer/Config.php",
        include_str!("../php/stubs/Composer/Config.php"),
    ),
    (
        "Composer/Downloader/DownloadManager.php",
        include_str!("../php/stubs/Composer/Downloader/DownloadManager.php"),
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
    (
        "Composer/Package/BasePackage.php",
        include_str!("../php/stubs/Composer/Package/BasePackage.php"),
    ),
    (
        "Composer/Package/Package.php",
        include_str!("../php/stubs/Composer/Package/Package.php"),
    ),
    (
        "Composer/Package/CompletePackage.php",
        include_str!("../php/stubs/Composer/Package/CompletePackage.php"),
    ),
    (
        "Composer/Package/RootPackage.php",
        include_str!("../php/stubs/Composer/Package/RootPackage.php"),
    ),
    (
        "Composer/Package/AliasPackage.php",
        include_str!("../php/stubs/Composer/Package/AliasPackage.php"),
    ),
    (
        "Composer/Package/CompleteAliasPackage.php",
        include_str!("../php/stubs/Composer/Package/CompleteAliasPackage.php"),
    ),
    (
        "Composer/Package/RootAliasPackage.php",
        include_str!("../php/stubs/Composer/Package/RootAliasPackage.php"),
    ),
    (
        "Composer/Repository/ArrayRepository.php",
        include_str!("../php/stubs/Composer/Repository/ArrayRepository.php"),
    ),
    (
        "Composer/Repository/WritableArrayRepository.php",
        include_str!("../php/stubs/Composer/Repository/WritableArrayRepository.php"),
    ),
    (
        "Composer/Repository/InstalledArrayRepository.php",
        include_str!("../php/stubs/Composer/Repository/InstalledArrayRepository.php"),
    ),
    (
        "Composer/Repository/FilesystemRepository.php",
        include_str!("../php/stubs/Composer/Repository/FilesystemRepository.php"),
    ),
    (
        "Composer/Repository/InstalledFilesystemRepository.php",
        include_str!("../php/stubs/Composer/Repository/InstalledFilesystemRepository.php"),
    ),
    (
        "Composer/Repository/RepositoryManager.php",
        include_str!("../php/stubs/Composer/Repository/RepositoryManager.php"),
    ),
    (
        "Composer/Installer/InstallationManager.php",
        include_str!("../php/stubs/Composer/Installer/InstallationManager.php"),
    ),
    (
        "Composer/Installer/PackageEvent.php",
        include_str!("../php/stubs/Composer/Installer/PackageEvent.php"),
    ),
    (
        "Composer/DependencyResolver/Operation/SolverOperation.php",
        include_str!("../php/stubs/Composer/DependencyResolver/Operation/SolverOperation.php"),
    ),
    (
        "Composer/DependencyResolver/Operation/InstallOperation.php",
        include_str!("../php/stubs/Composer/DependencyResolver/Operation/InstallOperation.php"),
    ),
    (
        "Composer/DependencyResolver/Operation/UpdateOperation.php",
        include_str!("../php/stubs/Composer/DependencyResolver/Operation/UpdateOperation.php"),
    ),
    (
        "Composer/DependencyResolver/Operation/UninstallOperation.php",
        include_str!("../php/stubs/Composer/DependencyResolver/Operation/UninstallOperation.php"),
    ),
    (
        "Composer/DependencyResolver/Operation/MarkAliasInstalledOperation.php",
        include_str!(
            "../php/stubs/Composer/DependencyResolver/Operation/MarkAliasInstalledOperation.php"
        ),
    ),
    (
        "Composer/DependencyResolver/Operation/MarkAliasUninstalledOperation.php",
        include_str!(
            "../php/stubs/Composer/DependencyResolver/Operation/MarkAliasUninstalledOperation.php"
        ),
    ),
];

/// Hand-written worker-side classes (two-world implementations with behavior of their own, not
/// mechanical proxies), written into the same autoload directory as the generated stubs so the
/// prepended stub autoloader resolves their FQCNs ahead of any real class file.
const RUNTIME_FILES: &[(&str, &str)] = &[
    (
        "Composer/Console/Application.php",
        include_str!("../php/runtime/Composer/Console/Application.php"),
    ),
    (
        "Composer/EventDispatcher/Event.php",
        include_str!("../php/runtime/Composer/EventDispatcher/Event.php"),
    ),
    (
        "Shirabe/MaterializedValue.php",
        include_str!("../php/runtime/Shirabe/MaterializedValue.php"),
    ),
    (
        "Shirabe/RustCommandStub.php",
        include_str!("../php/runtime/Shirabe/RustCommandStub.php"),
    ),
    (
        "Shirabe/RustPluginStub.php",
        include_str!("../php/runtime/Shirabe/RustPluginStub.php"),
    ),
    (
        "Shirabe/RustCapablePluginStub.php",
        include_str!("../php/runtime/Shirabe/RustCapablePluginStub.php"),
    ),
];

/// Guard classes made autoloadable inside the worker, behind the stubs: they shadow the real
/// Composer class of every Rust-owned FQCN no stub proxies, so the worker cannot fall through to
/// the real implementation and run an instance the Rust side never sees. Generated by
/// `scripts/plugin-stub-generator/generate-stubs`; too many to spell out here, so `build.rs`
/// builds the list from the directory itself.
const GUARD_FILES: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/guard-files.rs"));

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

// TODO(error-model): a failed spawn panics rather than propagating a `Result`; this is an interim
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

/// The descriptor the worker's end of the RPC socket is installed on in the child. `pre_exec`
/// dup2s onto it, which closes whatever the fork inherited there, so the number is ours to pick
/// as long as it is above the three standard streams.
const WORKER_SOCKET_FD: std::os::fd::RawFd = 3;

fn spawn_worker() -> anyhow::Result<Worker> {
    let php = PhpExecutableFinder::new()
        .find(false)
        .ok_or_else(|| anyhow::anyhow!("no PHP executable found"))?;

    let tempdir = tempfile::tempdir()?;
    let script_path = tempdir.path().join("worker.php");
    std::fs::write(&script_path, GLUE_SCRIPT)?;

    let stubs_dir = tempdir.path().join("stubs");
    let guards_dir = tempdir.path().join("guards");
    let files = STUB_FILES
        .iter()
        .chain(RUNTIME_FILES)
        .map(|entry| (&stubs_dir, entry))
        .chain(GUARD_FILES.iter().map(|entry| (&guards_dir, entry)));
    for (dir, (relative_path, contents)) in files {
        let path = dir.join(relative_path);
        std::fs::create_dir_all(path.parent().expect("generated class paths have a parent"))?;
        std::fs::write(&path, contents)?;
    }

    // A connected pair, not a bound path: an AF_UNIX path has to fit in `sun_path` (108 bytes),
    // which a long TMPDIR overruns, and the worker is a child we spawn ourselves, so it can
    // inherit its end instead of connecting to one. The pair is connected from the start, so
    // there is no accept to wait for either — a child that dies before reading shows up as EOF
    // on the first call, with its exit status attached by `worker_state`.
    let (stream, child_end) = UnixStream::pair()?;
    let child_end = std::os::fd::OwnedFd::from(child_end);

    let mut command = std::process::Command::new(&php);
    command
        // The Rust-side codec produces the byte representation of the default (and only
        // supported) serialize_precision; pin the child to it in case a distro php.ini overrides
        // the default.
        .arg("-d")
        .arg("serialize_precision=-1")
        // The Composer runtime bundle has no phar signature.
        .arg("-d")
        .arg("phar.require_hash=0");
    if xdebug::switches_xdebug_off() {
        // The environment variable takes precedence over every ini setting, so switching the
        // mode off takes both. See `docs/dev/xdebug.md`.
        command
            .arg("-d")
            .arg("xdebug.mode=off")
            .env("XDEBUG_MODE", "off");
    }
    command
        .arg(&script_path)
        .arg(WORKER_SOCKET_FD.to_string())
        .arg(&stubs_dir)
        .arg(&guards_dir);
    // SAFETY: the closure only calls async-signal-safe syscalls, as required between fork and
    // exec. It owns the child end, so the descriptor stays alive until the exec happens.
    unsafe {
        std::os::unix::process::CommandExt::pre_exec(&mut command, move || {
            install_worker_socket_fd(&child_end)
        });
    }
    let child = command.spawn()?;
    // Dropping the command drops the pre_exec closure with it, closing the parent's copy of the
    // child end. Without that the parent would never observe EOF on a dead worker.
    drop(command);

    Ok(Worker {
        stream,
        child,
        _tempdir: tempdir,
    })
}

/// Moves the worker's end of the socket onto [`WORKER_SOCKET_FD`] in the freshly forked child.
fn install_worker_socket_fd(child_end: &std::os::fd::OwnedFd) -> std::io::Result<()> {
    use std::os::fd::{AsRawFd as _, FromRawFd as _, IntoRawFd as _};

    if child_end.as_raw_fd() == WORKER_SOCKET_FD {
        // dup2(fd, fd) is a no-op that leaves FD_CLOEXEC set, which would close the descriptor
        // on exec; clear the flag by hand instead.
        nix::fcntl::fcntl(
            child_end,
            nix::fcntl::FcntlArg::F_SETFD(nix::fcntl::FdFlag::empty()),
        )?;
        return Ok(());
    }
    // SAFETY: dup2_raw closes the target if it is open and makes it a duplicate of the child
    // end; releasing the returned owner keeps it open across the exec.
    let installed = unsafe {
        nix::unistd::dup2_raw(
            child_end,
            std::os::fd::OwnedFd::from_raw_fd(WORKER_SOCKET_FD),
        )?
    };
    let _ = installed.into_raw_fd();
    Ok(())
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
    fn worker_starts_with_xdebug_switched_off() {
        if PhpExecutableFinder::new().find(false).is_none() {
            // No PHP in this environment; the worker cannot start.
            return;
        }

        let mut worker = spawn_worker().expect("failed to spawn PHP worker");

        // The `-d xdebug.mode=off` half is invisible from PHP while the extension is not loaded
        // (an unregistered ini entry is not readable), so only the environment half — the one
        // that overrides every ini setting — can be asserted here.
        frame::write_frame(
            &mut worker.stream,
            &Frame::CallFunction {
                corr_id: 1,
                function_name: "getenv".to_string(),
                args: vec![PluginValue::string("XDEBUG_MODE")],
                out_param_positions: Vec::new(),
            },
        )
        .expect("failed to ask the worker for its Xdebug mode");
        let reply = frame::read_frame(&mut worker.stream).expect("failed to read the worker reply");
        match reply {
            Frame::Return { value, .. } => assert_eq!(
                value.to_php_mixed().expect("unusable reply"),
                PhpMixed::String("off".to_string())
            ),
            other => panic!("unexpected reply: {other:?}"),
        }

        worker.child.kill().expect("failed to kill PHP worker");
        worker.child.wait().expect("failed to reap PHP worker");
    }

    #[test]
    fn queries_string_lists_when_php_available() {
        if PhpExecutableFinder::new().find(false).is_none() {
            // No PHP in this environment; the worker cannot start.
            return;
        }

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
        assert_eq!(diagnostics.php_version, get_php_version().version);
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

        let php = get_php_version();
        assert!(!php.version.is_empty(), "expected a PHP version");
        assert_eq!(
            php.version
                .split('.')
                .next()
                .and_then(|n| n.parse::<i64>().ok()),
            Some(php.major),
            "version should start with the major version: {}",
            php.version,
        );
        assert_eq!(
            php.version_id,
            php.major * 10000 + php.minor * 100 + php.release
        );

        let binary = get_php_binary();
        assert!(!binary.is_empty(), "expected a PHP binary path");
        assert!(
            std::path::Path::new(&binary).exists(),
            "binary should exist: {binary}",
        );
    }

    #[test]
    fn queries_platform_info_when_php_available() {
        if PhpExecutableFinder::new().find(false).is_none() {
            // No PHP in this environment; the worker cannot start.
            return;
        }

        let platform_info = get_platform_info();

        let extensions = platform_info.get_extensions();
        assert!(
            extensions.iter().any(|extension| extension == "Core"),
            "expected the Core extension among {extensions:?}",
        );
        assert_eq!(
            platform_info.get_extension_version("Core"),
            get_php_version().version
        );

        assert!(platform_info.has_constant("PHP_VERSION", None));
        assert_eq!(
            platform_info.get_constant("PHP_INT_SIZE", None),
            PhpMixed::Int(8)
        );
        match platform_info.get_constant("PHP_VERSION", None) {
            PhpMixed::String(s) => assert!(!s.is_empty(), "expected a non-empty PHP_VERSION"),
            other => panic!("expected a string, got {other:?}"),
        }
    }

    #[test]
    #[should_panic(expected = "is not reported by the platform payload")]
    fn platform_info_rejects_an_unreported_constant() {
        PlatformInfo::default().has_constant("SHIRABE_DOES_NOT_EXIST_XYZ", None);
    }
}
