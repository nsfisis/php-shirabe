//! Rust-to-PHP RPC over a Unix domain socket. See `docs/dev/php-rpc.md`.

pub mod composer_runtime;
mod env;
pub mod frame;
pub mod rar;
pub mod session;
pub mod value;
pub mod wire;
pub mod xdebug;

pub use value::{PhpObjHandle, PhpObject, PluginValue, RustObjHandle};
pub use wire::{FromWire, ToWire};

use bstr::{B, BStr, BString, ByteSlice};
use frame::Frame;
use indexmap::IndexMap;
use shirabe_php_shim::{
    PhpMixed, ToPhpString, bformat, os_str_from_bytes, path_from_bytes, php_str,
};
use shirabe_symfony_process::PhpExecutableFinder;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use wire::protocol_value;

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
fn cached<T>(
    cell: &'static OnceLock<T>,
    init: impl FnOnce() -> anyhow::Result<T>,
) -> anyhow::Result<&'static T> {
    if let Some(value) = cell.get() {
        return Ok(value);
    }
    let value = init()?;
    Ok(cell.get_or_init(|| value))
}

wire_record! {
    /// `curl_version()`, together with the `CURL_*` constants the `diagnose` command consults.
    /// Every `Option` field is `None` when the corresponding array key or constant is absent.
    #[derive(Debug)]
    pub struct Curl {
        pub version: BString,
        pub libz_version: Option<BString>,
        pub brotli_version: Option<BString>,
        pub ssl_version: Option<BString>,
        pub features: Option<i64>,
        pub version_zstd: Option<i64>,
        pub version_http2: Option<i64>,
        pub has_http_version_2_0: bool,
        pub version_http3: Option<i64>,
    }
}

wire_record! {
    /// Everything the `diagnose` command needs to know about the PHP runtime, fetched in a single
    /// round trip because the command would otherwise query the same runtime dozens of times.
    #[derive(Debug)]
    pub struct Diagnostics {
        pub php_version: BString,
        pub php_version_id: i64,
        /// `None` when `PHP_BINARY` is undefined.
        pub php_binary: Option<std::path::PathBuf>,
        /// `None` when `OPENSSL_VERSION_TEXT` is undefined.
        pub openssl_version_text: Option<BString>,
        /// `0` when `OPENSSL_VERSION_NUMBER` is undefined.
        pub openssl_version_number: i64,
        pub has_hhvm_version: bool,
        pub has_php_windows_version_build: bool,
        /// `Composer\XdebugHandler\XdebugHandler::isXdebugActive()`.
        pub xdebug_active: bool,
        /// `0` when the ionCube loader is not loaded.
        pub ioncube_loader_iversion: i64,
        /// Empty when the ionCube loader is not loaded.
        pub ioncube_loader_version: BString,
        /// `phpinfo(INFO_GENERAL)` output.
        pub phpinfo_general: BString,
        /// `None` when the curl extension is not loaded.
        pub curl: Option<Curl>,
        extensions: IndexMap<BString, bool>,
        functions: IndexMap<BString, bool>,
        /// PHP's `false` (no such setting) is `None`.
        ini_settings: IndexMap<BString, Option<BString>>,
    }
}

wire_record! {
    /// The names the `__shirabe_diagnose` payload reports on.
    struct DiagnoseQuery {
        extensions: Vec<BString>,
        functions: Vec<BString>,
        ini_settings: Vec<BString>,
    }
}

/// The extensions `Diagnostics::extension_loaded` can be asked about.
const DIAGNOSE_EXTENSIONS: &[&BStr] = &[
    php_str(b"apcu"),
    php_str(b"curl"),
    php_str(b"filter"),
    php_str(b"hash"),
    php_str(b"iconv"),
    php_str(b"ionCube Loader"),
    php_str(b"mbstring"),
    php_str(b"openssl"),
    php_str(b"Phar"),
    php_str(b"uopz"),
    php_str(b"zip"),
    php_str(b"zlib"),
];

/// The functions `Diagnostics::function_exists` can be asked about.
const DIAGNOSE_FUNCTIONS: &[&BStr] = &[
    php_str(b"disk_free_space"),
    php_str(b"json_decode"),
    php_str(b"proc_open"),
];

/// The settings `Diagnostics::ini_get` can be asked about.
const DIAGNOSE_INI_SETTINGS: &[&BStr] = &[
    php_str(b"allow_url_fopen"),
    php_str(b"apc.enable_cli"),
    php_str(b"uopz.disable"),
    php_str(b"uopz.exit"),
    php_str(b"xdebug.profiler_enabled"),
];

impl Diagnostics {
    /// PHP `extension_loaded($name)`. Only the extensions the worker reports can be asked about;
    /// any other name is a bug in the caller, not a missing extension.
    pub fn extension_loaded(&self, name: impl AsRef<[u8]>) -> bool {
        let name = name.as_ref().as_bstr();
        *self.extensions.get(name).unwrap_or_else(|| {
            panic!("PHP RPC: extension {name:?} is not reported by the diagnose payload")
        })
    }

    /// PHP `function_exists($name)`. See [`Diagnostics::extension_loaded`] for the fixed set of names.
    pub fn function_exists(&self, name: impl AsRef<[u8]>) -> bool {
        let name = name.as_ref().as_bstr();
        *self.functions.get(name).unwrap_or_else(|| {
            panic!("PHP RPC: function {name:?} is not reported by the diagnose payload")
        })
    }

    /// PHP `ini_get($option)`, with PHP's `false` (no such setting) mapped to `None`. See
    /// [`Diagnostics::extension_loaded`] for the fixed set of names.
    pub fn ini_get(&self, option: impl AsRef<[u8]>) -> Option<&BStr> {
        let option = option.as_ref().as_bstr();
        self.ini_settings
            .get(option)
            .unwrap_or_else(|| {
                panic!("PHP RPC: ini setting {option:?} is not reported by the diagnose payload")
            })
            .as_ref()
            .map(|value| value.as_bstr())
    }
}

static DIAGNOSTICS: OnceLock<Diagnostics> = OnceLock::new();

/// PHP runtime information for the `diagnose` command. The worker is queried once per process;
/// subsequent calls reuse the cached payload.
pub fn get_diagnostics() -> anyhow::Result<&'static Diagnostics> {
    cached(&DIAGNOSTICS, || {
        query(
            "__shirabe_diagnose",
            DiagnoseQuery {
                extensions: DIAGNOSE_EXTENSIONS
                    .iter()
                    .map(|name| (*name).to_owned())
                    .collect(),
                functions: DIAGNOSE_FUNCTIONS
                    .iter()
                    .map(|name| (*name).to_owned())
                    .collect(),
                ini_settings: DIAGNOSE_INI_SETTINGS
                    .iter()
                    .map(|name| (*name).to_owned())
                    .collect(),
            },
        )
    })
}

/// The constants `PlatformInfo::has_constant` can be asked about.
const PLATFORM_CONSTANTS: &[&BStr] = &[
    php_str(b"PHP_VERSION"),
    php_str(b"PHP_DEBUG"),
    php_str(b"PHP_ZTS"),
    php_str(b"PHP_INT_SIZE"),
    php_str(b"AF_INET6"),
    php_str(b"GD_VERSION"),
    php_str(b"GMP_VERSION"),
    php_str(b"ICONV_VERSION"),
    php_str(b"INTL_ICU_VERSION"),
    php_str(b"LIBXML_DOTTED_VERSION"),
    php_str(b"MB_ONIGURUMA_VERSION"),
    php_str(b"OPENSSL_VERSION_TEXT"),
    php_str(b"PCRE_VERSION"),
    php_str(b"PGSQL_LIBPQ_VERSION"),
    php_str(b"RD_KAFKA_VERSION"),
    php_str(b"SODIUM_LIBRARY_VERSION"),
    php_str(b"LIBXSLT_DOTTED_VERSION"),
    php_str(b"ZipArchive::LIBZIP_VERSION"),
    php_str(b"ZLIB_VERSION"),
];

/// The classes `PlatformInfo::has_class` can be asked about.
const PLATFORM_CLASSES: &[&BStr] = &[php_str(b"ResourceBundle"), php_str(b"IntlChar")];

/// The extensions whose `ReflectionExtension::info()` output `PlatformInfo::get_extension_info`
/// can be asked about, when they are loaded.
const PLATFORM_EXTENSION_INFO: &[&BStr] = &[
    php_str(b"amqp"),
    php_str(b"bz2"),
    php_str(b"curl"),
    php_str(b"date"),
    php_str(b"fileinfo"),
    php_str(b"gd"),
    php_str(b"intl"),
    php_str(b"ldap"),
    php_str(b"mbstring"),
    php_str(b"memcached"),
    php_str(b"mongodb"),
    php_str(b"mysqlnd"),
    php_str(b"pcre"),
    php_str(b"pdo_mysql"),
    php_str(b"pdo_pgsql"),
    php_str(b"pdo_sqlite"),
    php_str(b"pgsql"),
    php_str(b"pq"),
    php_str(b"sqlite3"),
    php_str(b"ssh2"),
    php_str(b"xsl"),
    php_str(b"yaml"),
    php_str(b"zlib"),
];

wire_record! {
    /// The names the `__shirabe_platform` payload reports on.
    struct PlatformQuery {
        constants: Vec<BString>,
        classes: Vec<BString>,
        extension_info: Vec<BString>,
    }
}

wire_record! {
    /// The answer to the `__shirabe_platform` query, which [`PlatformInfo`] is built from.
    struct PlatformPayload {
        extensions: Vec<BString>,
        extension_versions: IndexMap<BString, BString>,
        extension_info: IndexMap<BString, BString>,
        /// Only the defined constants.
        constants: IndexMap<BString, PhpMixed>,
        classes: IndexMap<BString, bool>,
        curl_version: Option<BString>,
        inet_pton_ipv6: Option<BString>,
        resource_bundle_version: Option<BString>,
        intl_char_unicode_version: Option<Vec<i64>>,
        imagick_version_string: Option<BString>,
    }
}

/// Everything `PlatformRepository` needs to know about the PHP runtime, fetched in a single round
/// trip because it would otherwise query the same runtime once per extension and constant.
///
/// The fields standing in for PHP calls and objects carry the entries the consumer reads off their
/// results rather than the results themselves.
#[derive(Debug, Clone)]
pub struct PlatformInfo {
    extensions: Vec<BString>,
    extension_versions: IndexMap<BString, BString>,
    extension_info: IndexMap<BString, BString>,
    /// Keyed as `ltrim($class.'::'.$constant, ':')`; `None` for a reported but undefined constant.
    constants: IndexMap<BString, Option<PhpMixed>>,
    classes: IndexMap<BString, bool>,
    /// `curl_version()['version']`, or `None` when the curl extension is not loaded.
    pub curl_version: Option<BString>,
    /// `@inet_pton('::')`, with PHP's `false` mapped to `None`.
    pub inet_pton_ipv6: Option<Vec<u8>>,
    /// `ResourceBundle::create('root', 'ICUDATA', false)->get('Version')`, or `None` when the class
    /// is absent or the bundle cannot be opened.
    pub resource_bundle_version: Option<BString>,
    /// `IntlChar::getUnicodeVersion()`, or `None` when the class is absent.
    pub intl_char_unicode_version: Option<Vec<i64>>,
    /// `(new Imagick())->getVersion()['versionString']`, or `None` when the extension is not
    /// loaded.
    pub imagick_version_string: Option<BString>,
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
                .map(|name| ((*name).to_owned(), None))
                .collect(),
            classes: PLATFORM_CLASSES
                .iter()
                .map(|name| ((*name).to_owned(), false))
                .collect(),
            curl_version: None,
            inet_pton_ipv6: None,
            resource_bundle_version: None,
            intl_char_unicode_version: None,
            imagick_version_string: None,
        }
    }
}

impl PlatformInfo {
    /// PHP `get_loaded_extensions()`.
    pub fn get_extensions(&self) -> &[BString] {
        &self.extensions
    }

    /// PHP `phpversion($extension)`, with PHP's `false` mapped to `'0'`.
    pub fn get_extension_version(&self, extension: impl AsRef<[u8]>) -> &BStr {
        let extension = extension.as_ref().as_bstr();
        self.extension_versions
            .get(extension)
            .unwrap_or_else(|| {
                panic!("PHP RPC: extension {extension:?} is not loaded in the platform payload")
            })
            .as_bstr()
    }

    /// PHP `(new \ReflectionExtension($extension))->info()` output. Only the extensions the worker
    /// reports can be asked about; any other name is a bug in the caller.
    pub fn get_extension_info(&self, extension: impl AsRef<[u8]>) -> &BStr {
        let extension = extension.as_ref().as_bstr();
        self.extension_info
            .get(extension)
            .unwrap_or_else(|| {
                panic!("PHP RPC: extension {extension:?} info is not in the platform payload")
            })
            .as_bstr()
    }

    /// PHP `defined(ltrim($class.'::'.$constant, ':'))`. Only the constants the worker reports can
    /// be asked about; any other name is a bug in the caller, not an undefined constant.
    pub fn has_constant(&self, constant_name: impl AsRef<[u8]>, class: Option<&BStr>) -> bool {
        self.constant(constant_name, class).is_some()
    }

    /// PHP `constant(ltrim($class.'::'.$constant, ':'))`, answering null for an undefined
    /// constant. See [`PlatformInfo::has_constant`] for the fixed set of names.
    pub fn get_constant(&self, constant_name: impl AsRef<[u8]>, class: Option<&BStr>) -> PhpMixed {
        self.constant(constant_name, class)
            .cloned()
            .unwrap_or(PhpMixed::Null)
    }

    /// PHP `class_exists($class, false)`. See [`PlatformInfo::has_constant`] for the fixed set of
    /// names.
    pub fn has_class(&self, class: impl AsRef<[u8]>) -> bool {
        let class = class.as_ref().as_bstr();
        *self.classes.get(class).unwrap_or_else(|| {
            panic!("PHP RPC: class {class:?} is not reported by the platform payload")
        })
    }

    /// For testing only: reports `extensions` as loaded, each at `version`.
    pub fn __set_extensions(
        &mut self,
        extensions: impl IntoIterator<Item = impl Into<BString>>,
        version: impl AsRef<[u8]>,
    ) {
        let extensions: Vec<BString> = extensions.into_iter().map(Into::into).collect();
        self.extension_versions = extensions
            .iter()
            .map(|name| (name.clone(), BString::from(version.as_ref())))
            .collect();
        self.extensions = extensions;
    }

    /// For testing only: reports `info` as the `ReflectionExtension::info()` output of `extension`.
    pub fn __set_extension_info(&mut self, extension: impl AsRef<[u8]>, info: impl AsRef<[u8]>) {
        self.extension_info.insert(
            BString::from(extension.as_ref()),
            BString::from(info.as_ref()),
        );
    }

    /// For testing only: reports the constant as defined with `value`. Panics on a constant the
    /// worker does not report, so a test cannot describe a runtime the worker cannot report.
    pub fn __set_constant(
        &mut self,
        constant_name: impl AsRef<[u8]>,
        class: Option<&BStr>,
        value: PhpMixed,
    ) {
        let key = Self::constant_key(constant_name, class);
        let entry = self.constants.get_mut(&key).unwrap_or_else(|| {
            panic!("PHP RPC: constant {key:?} is not reported by the platform payload")
        });
        *entry = Some(value);
    }

    /// For testing only: reports the class as defined. See [`PlatformInfo::__set_constant`].
    pub fn __set_class(&mut self, class: impl AsRef<[u8]>) {
        let class = class.as_ref().as_bstr();
        let entry = self.classes.get_mut(class).unwrap_or_else(|| {
            panic!("PHP RPC: class {class:?} is not reported by the platform payload")
        });
        *entry = true;
    }

    fn constant(&self, constant_name: impl AsRef<[u8]>, class: Option<&BStr>) -> Option<&PhpMixed> {
        let key = Self::constant_key(constant_name, class);
        self.constants
            .get(&key)
            .unwrap_or_else(|| {
                panic!("PHP RPC: constant {key:?} is not reported by the platform payload")
            })
            .as_ref()
    }

    fn constant_key(constant_name: impl AsRef<[u8]>, class: Option<&BStr>) -> BString {
        let constant_name = constant_name.as_ref();
        match class {
            Some(class) => bformat!("{class}::{constant_name}"),
            None => BString::from(constant_name),
        }
    }
}

static PLATFORM_INFO: OnceLock<PlatformInfo> = OnceLock::new();

/// PHP runtime information for `PlatformRepository`. The worker is queried once per process;
/// subsequent calls reuse the cached payload.
pub fn get_platform_info() -> anyhow::Result<&'static PlatformInfo> {
    cached(&PLATFORM_INFO, || {
        let payload: PlatformPayload = query(
            "__shirabe_platform",
            PlatformQuery {
                constants: PLATFORM_CONSTANTS
                    .iter()
                    .map(|name| (*name).to_owned())
                    .collect(),
                classes: PLATFORM_CLASSES
                    .iter()
                    .map(|name| (*name).to_owned())
                    .collect(),
                extension_info: PLATFORM_EXTENSION_INFO
                    .iter()
                    .map(|name| (*name).to_owned())
                    .collect(),
            },
        )?;
        Ok(PlatformInfo {
            extensions: payload.extensions,
            extension_versions: payload.extension_versions,
            extension_info: payload.extension_info,
            constants: PLATFORM_CONSTANTS
                .iter()
                .map(|name| ((*name).to_owned(), payload.constants.get(B(name)).cloned()))
                .collect(),
            classes: PLATFORM_CLASSES
                .iter()
                .map(|name| {
                    let exists = *payload.classes.get(B(name)).unwrap_or_else(|| {
                        panic!("PHP RPC: the platform payload does not report the class {name:?}")
                    });
                    ((*name).to_owned(), exists)
                })
                .collect(),
            curl_version: payload.curl_version,
            inet_pton_ipv6: payload.inet_pton_ipv6.map(Vec::from),
            resource_bundle_version: payload.resource_bundle_version,
            intl_char_unicode_version: payload.intl_char_unicode_version,
            imagick_version_string: payload.imagick_version_string,
        })
    })
}

/// Runs a query against the worker: a PHP function, or an entry point answering one consumer's
/// questions about the runtime. A failure to reach the worker and an exception the call throws are
/// errors; an answer that does not decode as `T` is a protocol violation, because only the
/// worker's own code or a PHP builtin with a fixed return type writes it.
fn query<T: FromWire>(name: impl AsRef<[u8]>, request: impl ToWire) -> anyhow::Result<T> {
    let name = name.as_ref().as_bstr();
    let value = call_internal_function(name, vec![request.to_wire()])??;
    Ok(protocol_value(&value, bformat!("the answer of {name}")))
}

/// An answer typed `T|false`, with `false` mapped to `None`. Any other answer that does not decode
/// as `T` is a protocol violation, as for [`query`].
fn or_false<T: FromWire>(value: &PluginValue, what: impl AsRef<[u8]>) -> Option<T> {
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

/// What `Composer\XdebugHandler\XdebugHandler::getAllIniFiles()` measures: `[(string)
/// php_ini_loaded_file()]` merged with the trimmed, comma-split `php_ini_scanned_files()` list
/// when scanning is active. The worker runs on the ini files of the machine, so these are the
/// user's own.
pub fn get_all_ini_files() -> anyhow::Result<Vec<std::path::PathBuf>> {
    query("__shirabe_get_all_ini_files", PluginValue::Null)
}

/// `Composer\XdebugHandler\XdebugHandler::isXdebugActive()` as measured in the worker, which is
/// the process Xdebug is loaded into.
fn xdebug_active() -> anyhow::Result<bool> {
    query("__shirabe_xdebug_active", PluginValue::Null)
}

/// A PHP exception that crossed the RPC boundary (the recoverable failure lane, as opposed to
/// the fatal `anyhow::Error` lane used for a dead worker or a broken channel).
#[derive(Debug, Clone)]
pub struct PhpThrow {
    pub exception_class: BString,
    pub message: BString,
    pub code: i64,
    /// The state the exception carries beyond `message` and `code`, keyed by the property names
    /// its class declares. The receiving side revives them onto the instance it rebuilds.
    ///
    /// Boxed so a `Result<_, PhpThrow>` stays small; every dispatcher returns one.
    pub properties: Box<IndexMap<BString, PluginValue>>,
    /// Where the exception was created, as `getFile()` and `getLine()` report it. Empty and 0
    /// for a failure that was no exception.
    pub file: BString,
    pub line: i64,
    /// The superclasses of `exception_class`, nearest first, so the receiving side can rebuild
    /// the nearest one it has a type for. Empty from Rust to PHP, where every class is known.
    pub parents: Vec<BString>,
}

impl PhpThrow {
    /// An exception of `exception_class` carrying nothing beyond its message and code.
    pub fn new(
        exception_class: impl Into<BString>,
        message: impl Into<BString>,
        code: i64,
    ) -> PhpThrow {
        PhpThrow {
            exception_class: exception_class.into(),
            message: message.into(),
            code,
            properties: Box::new(IndexMap::new()),
            file: BString::default(),
            line: 0,
            parents: Vec::new(),
        }
    }

    pub fn runtime(message: impl Into<BString>) -> PhpThrow {
        PhpThrow::new("RuntimeException", message, 0)
    }

    /// The exception as it crosses the boundary wherever it does — as the payload of a `Throw`
    /// frame, a job rejection, or a value an entry point answers with: `[class, message, code,
    /// properties, file, line, parents]`. `\Shirabe\MaterializedThrowable::describe()` writes the
    /// same list on the PHP side.
    pub fn to_wire(&self) -> PluginValue {
        PluginValue::List(self.wire_fields())
    }

    /// Rebuilds the exception [`PhpThrow::to_wire`] describes. Both sides write the description
    /// in the same commit of Shirabe, so one that does not decode is a protocol violation.
    pub fn from_wire(value: PluginValue) -> PhpThrow {
        PhpThrow::from_wire_fields(protocol_value(&value, "an exception description"))
    }

    pub(crate) fn wire_fields(&self) -> Vec<PluginValue> {
        vec![
            PluginValue::string(self.exception_class.clone()),
            PluginValue::string(self.message.clone()),
            PluginValue::Int(self.code),
            PluginValue::Array((*self.properties).clone()),
            PluginValue::string(self.file.clone()),
            PluginValue::Int(self.line),
            PluginValue::List(
                self.parents
                    .iter()
                    .cloned()
                    .map(PluginValue::string)
                    .collect(),
            ),
        ]
    }

    pub(crate) fn from_wire_fields(fields: Vec<PluginValue>) -> PhpThrow {
        let Ok([class, message, code, properties, file, line, parents]) =
            <[PluginValue; 7]>::try_from(fields)
        else {
            panic!("PHP RPC: protocol violation — an exception description has not 7 fields");
        };
        PhpThrow {
            exception_class: protocol_value(&class, "the exception class"),
            message: protocol_value(&message, "the exception message"),
            code: protocol_value(&code, "the exception code"),
            properties: Box::new(protocol_value(&properties, "the exception properties")),
            file: protocol_value(&file, "the exception file"),
            line: protocol_value(&line, "the exception line"),
            parents: protocol_value(&parents, "the exception parents"),
        }
    }
}

impl std::fmt::Display for PhpThrow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: {}",
            self.exception_class.display_lossy(),
            self.message.display_lossy()
        )
    }
}

impl std::error::Error for PhpThrow {}

/// Handles `CallRustMethod` requests arriving while a Rust-initiated call is waiting for its
/// `Return` (the cooperative reentrancy loop). A handler may itself issue nested RPC calls.
///
/// A handler whose method takes by-ref parameters writes the value each one holds afterwards into
/// `out_params` under its position, and the stub copies it back into the caller's variable;
/// leaving a position out means the method never assigned to it, which is what PHP does with an
/// untouched by-ref parameter.
pub trait RustMethodDispatcher {
    fn dispatch(
        &mut self,
        rhandle: u64,
        method_name: &BStr,
        args: Vec<PluginValue>,
        out_params: &mut IndexMap<u32, PluginValue>,
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

/// Answers every `CallRustMethod` with an explicit error: the dispatcher of the calls this crate
/// makes for itself, none of which runs code that calls back.
#[derive(Debug)]
struct RefuseCallbacks;

impl RustMethodDispatcher for RefuseCallbacks {
    fn dispatch(
        &mut self,
        rhandle: u64,
        method_name: &BStr,
        _args: Vec<PluginValue>,
        _out_params: &mut IndexMap<u32, PluginValue>,
    ) -> Result<PluginValue, PhpThrow> {
        Err(PhpThrow::runtime(bformat!(
            "no Rust method dispatcher is active for this call (rhandle {rhandle}, method `{method_name}`)"
        )))
    }
}

/// Calls a PHP function in the worker, or the worker's entry point of that name. The outer
/// `Result` is the fatal lane (dead worker, broken framing); the inner one carries a PHP
/// exception if the call threw. `dispatcher` serves the `CallRustMethod` requests the worker
/// makes while the call is in flight.
pub fn call_function(
    name: impl AsRef<[u8]>,
    args: Vec<PluginValue>,
    dispatcher: &mut dyn RustMethodDispatcher,
) -> anyhow::Result<Result<PluginValue, PhpThrow>> {
    rpc_call(
        |corr_id| Frame::CallFunction {
            corr_id,
            function_name: name.as_ref().into(),
            args,
        },
        dispatcher,
    )
}

/// Calls `$class::$method(...$args)` in the worker (autoloading the class if needed).
pub fn call_static_method(
    class: impl AsRef<[u8]>,
    method: impl AsRef<[u8]>,
    args: Vec<PluginValue>,
    dispatcher: &mut dyn RustMethodDispatcher,
) -> anyhow::Result<Result<PluginValue, PhpThrow>> {
    rpc_call(
        |corr_id| Frame::CallStaticMethod {
            corr_id,
            pclass: class.as_ref().into(),
            method_name: method.as_ref().into(),
            args,
        },
        dispatcher,
    )
}

/// Instantiates `new $class(...$ctor_args)` in the worker (autoloading the class if needed).
/// On success the returned value is a `PluginValue::PhpHandle` registered in the worker's P
/// table; the entity stays alive there while the handle does.
pub fn new_object(
    class: impl AsRef<[u8]>,
    ctor_args: Vec<PluginValue>,
    dispatcher: &mut dyn RustMethodDispatcher,
) -> anyhow::Result<Result<PluginValue, PhpThrow>> {
    rpc_call(
        |corr_id| Frame::NewObject {
            corr_id,
            pclass: class.as_ref().into(),
            ctor_args,
        },
        dispatcher,
    )
}

/// Calls `$obj->$method(...$args)` on a P-table entity in the worker.
pub fn call_php_method(
    handle: &PhpObjHandle,
    method: impl AsRef<[u8]>,
    args: Vec<PluginValue>,
    dispatcher: &mut dyn RustMethodDispatcher,
) -> anyhow::Result<Result<PluginValue, PhpThrow>> {
    rpc_call(
        |corr_id| Frame::CallPhpMethod {
            corr_id,
            phandle: handle.phandle(),
            method_name: method.as_ref().into(),
            args,
        },
        dispatcher,
    )
}

/// [`call_function`] for the calls this crate makes for itself.
pub(crate) fn call_internal_function(
    name: impl AsRef<[u8]>,
    args: Vec<PluginValue>,
) -> anyhow::Result<Result<PluginValue, PhpThrow>> {
    call_function(name, args, &mut RefuseCallbacks)
}

/// Whether the PHP worker process has been spawned by this process. Callers that only need to
/// mirror state into an already-running worker (e.g. `InstalledVersions::reload` pushes) use
/// this to avoid spawning a worker that would have nothing to observe.
pub fn worker_is_running() -> bool {
    WORKER
        .lock()
        .unwrap_or_else(|e| panic!("PHP RPC: worker mutex poisoned: {e}"))
        .is_some()
}

fn rpc_call(
    request: impl FnOnce(u64) -> Frame,
    dispatcher: &mut dyn RustMethodDispatcher,
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
            Frame::Return {
                corr_id,
                value,
                out_params,
            } if corr_id == my_id => {
                assert!(
                    out_params.is_empty(),
                    "PHP RPC: protocol violation — the worker answered with out params: \
                     {out_params:?}"
                );
                return Ok(Ok(value));
            }
            Frame::Throw { corr_id, throw } if corr_id == my_id => {
                return Ok(Err(throw));
            }
            Frame::CallRustMethod {
                corr_id,
                rhandle,
                method_name,
                args,
            } => {
                let mut out_params = IndexMap::new();
                let outcome =
                    dispatcher.dispatch(rhandle, method_name.as_bstr(), args, &mut out_params);
                let reply = match outcome {
                    Ok(value) => Frame::Return {
                        corr_id,
                        value,
                        out_params,
                    },
                    Err(throw) => Frame::Throw { corr_id, throw },
                };
                send_frame(&reply)?;
            }
            Frame::ReleaseRustHandle { rhandle } => {
                // A one-way notification sent by a child-side stub's __destruct; the shirabe
                // crate registers a hook that drops the matching R-table entry. The hook is
                // installed before any entry exists, so an unhooked release is a no-op.
                RELEASE_RUST_HANDLE_HOOK.with(|hook| {
                    if let Some(hook) = hook.borrow().as_ref() {
                        hook(rhandle);
                    }
                });
                continue;
            }
            other => panic!(
                "PHP RPC: protocol violation — unexpected frame while waiting for corr_id \
                 {my_id}: {other:?}"
            ),
        }
    }
}

const GLUE_SCRIPT: &[u8] = include_bytes!("../php/worker.php");

/// Proxy stub classes made autoloadable inside the worker, generated by
/// `scripts/plugin-stub-generator/generate-stubs` into `php/stubs/`. `build.rs` builds the list
/// from the directory itself.
const STUB_FILES: &[(&BStr, &[u8])] = include!(concat!(env!("OUT_DIR"), "/stub-files.rs"));

/// Hand-written worker-side classes under `php/runtime/` (two-world implementations with behavior
/// of their own, not mechanical proxies), written into the same autoload directory as the
/// generated stubs so the prepended stub autoloader resolves their FQCNs ahead of any real class
/// file. `build.rs` builds the list from the directory itself.
const RUNTIME_FILES: &[(&BStr, &[u8])] = include!(concat!(env!("OUT_DIR"), "/runtime-files.rs"));

/// Guard classes made autoloadable inside the worker, behind the stubs: they shadow the real
/// Composer class of every Rust-owned FQCN no stub proxies, so the worker cannot fall through to
/// the real implementation and run an instance the Rust side never sees. Generated by
/// `scripts/plugin-stub-generator/generate-stubs` into `php/guards/`. `build.rs` builds the list
/// from the directory itself.
const GUARD_FILES: &[(&BStr, &[u8])] = include!(concat!(env!("OUT_DIR"), "/guard-files.rs"));

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
    #[expect(
        clippy::disallowed_types,
        clippy::disallowed_macros,
        reason = "an anyhow context the port raises about its child, holding no PHP string"
    )]
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

    /// Ends this process the way the worker ended, if it has: the worker stands in for the PHP
    /// process Composer runs in, so a plugin's `exit()`, a fatal error or a signal that ends the
    /// worker ends Shirabe with the same exit code or signal, as it ends Composer. Returns when the
    /// worker is still running.
    ///
    /// TODO(php-runtime): Composer's own shutdown functions, such as the memory-limit hint its
    /// Application registers, run in the process that ended and so are not run here.
    fn exit_if_worker_is_dead(&mut self) {
        // The socket closes as the worker ends, a moment before it can be reaped.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        let status = loop {
            match self.child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                _ => return,
            }
        };
        use std::io::Write as _;
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        if let Some(code) = status.code() {
            std::process::exit(code);
        }
        if let Some(signal) = std::os::unix::process::ExitStatusExt::signal(&status)
            && let Ok(signal) = nix::sys::signal::Signal::try_from(signal)
        {
            let default = nix::sys::signal::SigAction::new(
                nix::sys::signal::SigHandler::SigDfl,
                nix::sys::signal::SaFlags::empty(),
                nix::sys::signal::SigSet::empty(),
            );
            // SAFETY: SIG_DFL installs no handler code, and nothing runs after the raise below
            // that could rely on the disposition this replaces.
            let _ = unsafe { nix::sys::signal::sigaction(signal, &default) };
            let _ = nix::sys::signal::raise(signal);
            std::process::exit(128 + signal as i32);
        }
    }
}

/// The PHP worker, spawned on first use.
static WORKER: Mutex<Option<Worker>> = Mutex::new(None);

/// Runs `f` on the worker while holding its mutex, spawning the worker first if it is not running.
fn with_worker<T>(f: impl FnOnce(&mut Worker) -> T) -> anyhow::Result<T> {
    let mut guard = WORKER
        .lock()
        .unwrap_or_else(|e| panic!("PHP RPC: worker mutex poisoned: {e}"));
    let worker = match &mut *guard {
        Some(worker) => worker,
        None => guard.insert(
            spawn_worker().map_err(|e| anyhow::anyhow!("failed to spawn the PHP worker: {e:#}"))?,
        ),
    };
    Ok(f(worker))
}

/// Writes one frame while holding the worker mutex only for the duration of the write, so the
/// session owner (see `session`) can interleave sends and blocking reads without keeping the
/// worker locked across a whole call. A `ReleasePhpHandle` for every P handle dropped since the
/// previous write goes out first.
fn send_frame(frame: &Frame) -> anyhow::Result<()> {
    with_worker(|worker| {
        let mut write = |frame: &Frame| {
            frame::write_frame(&mut worker.stream, frame).map_err(|e| {
                worker.exit_if_worker_is_dead();
                anyhow::Error::new(e).context(worker.worker_state())
            })
        };
        for phandle in value::take_released_php_handles() {
            write(&Frame::ReleasePhpHandle { phandle })?;
        }
        write(frame)
    })?
}

fn recv_frame() -> anyhow::Result<Frame> {
    with_worker(|worker| {
        frame::read_frame(&mut worker.stream).map_err(|e| {
            worker.exit_if_worker_is_dead();
            anyhow::Error::new(e).context(worker.worker_state())
        })
    })?
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
        let path = dir.join(path_from_bytes(relative_path));
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

    let mut command = std::process::Command::new(path_from_bytes(&php));
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
        .arg(os_str_from_bytes(&WORKER_SOCKET_FD.to_php_string()))
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
    use shirabe_php_shim::utf8_text;

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
                function_name: "defined".into(),
                args: vec![PluginValue::string("PHP_VERSION")],
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
        let mut worker = spawn_worker().expect("failed to spawn PHP worker");

        // The `-d xdebug.mode=off` half is invisible from PHP while the extension is not loaded
        // (an unregistered ini entry is not readable), so only the environment half — the one
        // that overrides every ini setting — can be asserted here.
        frame::write_frame(
            &mut worker.stream,
            &Frame::CallFunction {
                corr_id: 1,
                function_name: "getenv".into(),
                args: vec![PluginValue::string("XDEBUG_MODE")],
            },
        )
        .expect("failed to ask the worker for its Xdebug mode");
        let reply = frame::read_frame(&mut worker.stream).expect("failed to read the worker reply");
        match reply {
            Frame::Return { value, .. } => assert_eq!(value, PluginValue::string("off")),
            other => panic!("unexpected reply: {other:?}"),
        }

        worker.child.kill().expect("failed to kill PHP worker");
        worker.child.wait().expect("failed to reap PHP worker");
    }

    #[test]
    fn queries_string_lists() {
        // XdebugHandler::getAllIniFiles() always yields at least one entry, which is the empty
        // string when no php.ini is loaded.
        assert!(!get_all_ini_files().unwrap().is_empty());
    }

    #[test]
    fn queries_memory_usage() {
        for real_usage in [false, true] {
            let usage = memory_get_usage(real_usage).unwrap();
            assert!(usage > 0);
            assert!(memory_get_peak_usage(real_usage).unwrap() >= usage);
        }
    }

    #[test]
    fn queries_diagnostics() {
        let diagnostics = get_diagnostics().unwrap();
        assert_eq!(diagnostics.php_version, get_php_version().unwrap().version);
        assert!(diagnostics.php_version_id >= 70205);
        let php_binary = get_php_binary().unwrap();
        assert_eq!(
            diagnostics.php_binary.as_deref(),
            Some(php_binary.as_path())
        );
        assert!(diagnostics.function_exists("json_decode"));
        assert!(!diagnostics.extension_loaded("ionCube Loader"));
        assert!(
            diagnostics.phpinfo_general.contains_str("PHP Version"),
            "expected phpinfo(INFO_GENERAL) output, got: {}",
            diagnostics.phpinfo_general.display_lossy(),
        );
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

    #[test]
    fn queries_platform_info() {
        let platform_info = get_platform_info().unwrap();

        let extensions = platform_info.get_extensions();
        assert!(
            extensions.iter().any(|extension| extension == "Core"),
            "expected the Core extension among {extensions:?}",
        );
        assert_eq!(
            platform_info.get_extension_version("Core"),
            get_php_version().unwrap().version
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
