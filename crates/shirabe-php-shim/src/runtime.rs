use crate::PhpMixed;
use indexmap::IndexMap;
use shirabe_php_src::standard::versioning::php_version_compare;

pub const PHP_VERSION_ID: i64 = 80100;
pub const PHP_VERSION: &str = "8.1.0";

pub const PHP_MAJOR_VERSION: i64 = 8;
pub const PHP_MINOR_VERSION: i64 = 1;
pub const PHP_RELEASE_VERSION: i64 = 0;

pub const PHP_WINDOWS_VERSION_MAJOR: i64 = 0;
pub const PHP_WINDOWS_VERSION_MINOR: i64 = 0;
pub const PHP_WINDOWS_VERSION_BUILD: i64 = 0;

pub const PHP_INT_MAX: i64 = i64::MAX;
pub const PHP_INT_MIN: i64 = i64::MIN;
pub const PHP_INT_SIZE: i64 = 8;

pub const HHVM_VERSION: Option<&str> = None;

pub const E_ALL: i64 = 32767;
pub const E_WARNING: i64 = 2;
pub const E_NOTICE: i64 = 8;
pub const E_USER_WARNING: i64 = 512;
pub const E_USER_NOTICE: i64 = 1024;
pub const E_DEPRECATED: i64 = 8192;
pub const E_USER_DEPRECATED: i64 = 16384;

pub const PHP_BINARY: &str = "";

// NOTE: &str matching in const expression does not compile for now.
pub const PHP_OS: &str = match std::env::consts::OS.as_bytes() {
    b"linux" => "Linux",
    b"macos" => "Darwin",
    b"windows" => "WINNT",
    b"freebsd" => "FreeBSD",
    b"openbsd" => "OpenBSD",
    b"netbsd" => "NetBSD",
    b"dragonfly" => "DragonFly",
    b"solaris" | b"illumos" => "SunOS",
    _ => std::env::consts::OS,
};

// Models the constants defined in a standard modern PHP CLI environment on a
// non-Windows platform with the common extensions loaded (curl, openssl, json).
// Windows-only, HHVM and Composer-bootstrap constants are reported undefined.
pub fn defined(name: &str) -> bool {
    matches!(
        name,
        "CURLMOPT_MAX_HOST_CONNECTIONS"
            | "CURL_HTTP_VERSION_2_0"
            | "CURL_HTTP_VERSION_3"
            | "CURL_VERSION_HTTP2"
            | "CURL_VERSION_HTTP3"
            | "CURL_VERSION_HTTPS_PROXY"
            | "CURL_VERSION_LIBZ"
            | "CURL_VERSION_ZSTD"
            | "GLOB_BRACE"
            | "OPENSSL_VERSION_TEXT"
            | "PHP_BINARY"
            | "SIGINT"
            | "STDIN"
            | "STDOUT"
    )
}

// Models the classes available in a standard PHP CLI environment running Composer:
// the common bundled extensions (zip, Phar) plus Composer's own runtime classes.
pub fn class_exists(name: &str) -> bool {
    matches!(name, "Composer\\InstalledVersions" | "Phar" | "ZipArchive")
}

// Models the functions available in a standard modern PHP CLI environment on a
// non-Windows platform with the common extensions loaded (curl, mbstring, iconv,
// zlib, posix, pcntl). Opt-in or Windows-only functions are reported absent.
pub fn function_exists(name: &str) -> bool {
    matches!(
        name,
        "bzcompress"
            | "cli_set_process_title"
            | "curl_multi_exec"
            | "curl_multi_init"
            | "curl_multi_setopt"
            | "curl_share_init"
            | "curl_strerror"
            | "date_default_timezone_get"
            | "date_default_timezone_set"
            | "disk_free_space"
            | "exec"
            | "filter_var"
            | "getmypid"
            | "gzcompress"
            | "iconv"
            | "ini_set"
            | "json_decode"
            | "mb_check_encoding"
            | "mb_convert_encoding"
            | "mb_strlen"
            | "pcntl_async_signals"
            | "pcntl_signal"
            | "php_strip_whitespace"
            | "php_uname"
            | "posix_geteuid"
            | "posix_getpwuid"
            | "posix_getuid"
            | "posix_isatty"
            | "proc_open"
            | "putenv"
            | "shell_exec"
            | "stream_isatty"
            | "symlink"
    )
}

/// PHP `PHP_OS_FAMILY` constant: the family of the host OS.
/// One of "Windows", "BSD", "Darwin", "Solaris", "Linux", "Unknown".
pub fn php_os_family() -> &'static str {
    match std::env::consts::OS {
        "linux" | "android" => "Linux",
        "macos" | "ios" => "Darwin",
        "windows" => "Windows",
        "freebsd" | "dragonfly" | "netbsd" | "openbsd" => "BSD",
        "solaris" | "illumos" => "Solaris",
        _ => "Unknown",
    }
}

// Models the extensions loaded in a standard PHP CLI environment running Composer.
// Opt-in extensions (apcu, xdebug, ionCube, uopz) are reported absent.
pub fn extension_loaded(name: &str) -> bool {
    matches!(
        name,
        "Phar"
            | "curl"
            | "filter"
            | "hash"
            | "iconv"
            | "intl"
            | "mbstring"
            | "openssl"
            | "pcre"
            | "zip"
            | "zlib"
    )
}

// Models the configuration of a standard PHP CLI environment. Settings belonging
// to extensions that are not loaded (apcu, uopz, xdebug) are not registered, so
// PHP's ini_get returns false (None) for them.
pub fn ini_get(option: &str) -> Option<String> {
    match option {
        "allow_url_fopen" => Some("1".to_string()),
        "default_socket_timeout" => Some("60".to_string()),
        "disable_functions" => Some(String::new()),
        "mbstring.func_overload" => Some("0".to_string()),
        "memory_limit" => Some("-1".to_string()),
        "open_basedir" => Some(String::new()),
        _ => None,
    }
}

pub fn get_loaded_extensions() -> Vec<String> {
    // Mirrors the set recognized by extension_loaded().
    // TODO(php-runtime): this models only the Composer-relevant subset, not PHP's full extension list
    // (Core, standard, date, pcre, ...).
    [
        "Phar", "curl", "filter", "hash", "iconv", "intl", "mbstring", "openssl", "zip", "zlib",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

// TODO(php-runtime): the callback should be registered in PHP runtime.
pub fn set_error_handler(_callback: fn(i64, &str, &str, i64) -> bool) {}

pub fn debug_backtrace() -> Vec<IndexMap<String, PhpMixed>> {
    // TODO(php-runtime): capturing a PHP-style call stack requires runtime introspection of the
    // interpreter frames, which has no equivalent in the compiled shim.
    todo!()
}

/// Equivalent to PHP `include $file;`
pub fn include_file(file: &str) -> PhpMixed {
    let _ = file;
    // TODO(php-runtime): `include` evaluates a PHP source file at runtime; there is no PHP interpreter.
    todo!()
}

pub fn spl_autoload_register(
    callback: Box<dyn Fn(&str) -> PhpMixed + Send + Sync>,
    throw: bool,
    prepend: bool,
) -> bool {
    let _ = (callback, throw, prepend);
    // TODO(php-runtime): class autoloading has no analogue in compiled Rust (classes are not loaded by
    // name at runtime), so the callback is dropped. Returns success so callers that register an
    // autoloader during startup can proceed; this is not a faithful implementation.
    true
}

pub fn spl_autoload_unregister(callback: Box<dyn Fn(&str) -> PhpMixed + Send + Sync>) -> bool {
    let _ = callback;
    // TODO(php-runtime): see spl_autoload_register; nothing is registered, so this is a no-op stub.
    true
}

static ERROR_REPORTING_LEVEL: std::sync::atomic::AtomicI64 =
    std::sync::atomic::AtomicI64::new(E_ALL);

pub fn error_reporting(level: Option<i64>) -> i64 {
    let old = ERROR_REPORTING_LEVEL.load(std::sync::atomic::Ordering::Relaxed);
    if let Some(level) = level {
        ERROR_REPORTING_LEVEL.store(level, std::sync::atomic::Ordering::Relaxed);
    }
    old
}

pub fn spl_autoload_functions() -> Vec<PhpMixed> {
    // In compiled Rust there is no runtime class-autoload registry, so no autoload functions are
    // ever registered. Callers (e.g. EventDispatcher::do_dispatch, which diffs the registry before
    // and after running listeners to re-order plugin-prepended autoloaders) therefore always
    // observe an empty list. See spl_autoload_register/unregister, which remain unimplemented.
    Vec::new()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Lt,
    Le,
    Eq,
    Ne,
    Ge,
    Gt,
}

impl std::fmt::Display for CmpOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            CmpOp::Lt => "<",
            CmpOp::Le => "<=",
            CmpOp::Eq => "==",
            CmpOp::Ne => "!=",
            CmpOp::Ge => ">=",
            CmpOp::Gt => ">",
        })
    }
}

pub fn version_compare(v1: &str, v2: &str, op: CmpOp) -> bool {
    let ord = version_compare_ordering(v1, v2);
    match op {
        CmpOp::Lt => ord.is_lt(),
        CmpOp::Le => ord.is_le(),
        CmpOp::Eq => ord.is_eq(),
        CmpOp::Ne => ord.is_ne(),
        CmpOp::Ge => ord.is_ge(),
        CmpOp::Gt => ord.is_gt(),
    }
}

pub fn version_compare_ordering(v1: &str, v2: &str) -> std::cmp::Ordering {
    let ord = php_version_compare(v1, v2);
    if ord < 0 {
        std::cmp::Ordering::Less
    } else if ord > 0 {
        std::cmp::Ordering::Greater
    } else {
        std::cmp::Ordering::Equal
    }
}

// TODO(php-runtime): the previous handler should be restored in the PHP runtime.
// Paired with set_error_handler, which is a no-op in this shim.
pub fn restore_error_handler() {}

pub trait HasAddress {
    fn address(&self) -> usize;
}

impl<T: ?Sized> HasAddress for &T {
    fn address(&self) -> usize {
        *self as *const T as *const u8 as usize
    }
}

impl<T: ?Sized> HasAddress for std::rc::Rc<T> {
    fn address(&self) -> usize {
        std::rc::Rc::as_ptr(self) as *const u8 as usize
    }
}

pub fn spl_object_hash<T: HasAddress>(object: T) -> String {
    format!("{:032x}", object.address())
}

// TODO(phase-c): the Windows branch of php_uname is missing. There PHP reports "Windows NT" as the
// sysname and derives release/version from the OS version APIs rather than uname(2).
pub fn php_uname(mode: &str) -> String {
    let Ok(utsname) = nix::sys::utsname::uname() else {
        return String::new();
    };
    let field = |value: &std::ffi::OsStr| value.to_string_lossy().into_owned();
    match mode {
        "s" => field(utsname.sysname()),
        "n" => field(utsname.nodename()),
        "r" => field(utsname.release()),
        "v" => field(utsname.version()),
        "m" => field(utsname.machine()),
        // "a" is the default and any other mode falls back to it: every field in the order
        // sysname, nodename, release, version, machine.
        _ => format!(
            "{} {} {} {} {}",
            field(utsname.sysname()),
            field(utsname.nodename()),
            field(utsname.release()),
            field(utsname.version()),
            field(utsname.machine()),
        ),
    }
}

pub fn trigger_error(_message: &str, _error_level: i64) {
    // TODO(php-runtime): emitting a PHP error obeys error_reporting and the installed error handler
    // (both runtime state not modeled here); writing unconditionally to stderr would diverge.
    todo!()
}

pub fn trigger_deprecation(_package: &str, _version: &str, _message: &str, _arg: &str) {
    // TODO(php-runtime): symfony/deprecation-contracts triggers an E_USER_DEPRECATED via the error
    // subsystem, which is not modeled (see trigger_error).
    todo!()
}

pub fn usleep(microseconds: u64) {
    std::thread::sleep(std::time::Duration::from_micros(microseconds));
}

/// Equivalent to PHP's __DIR__ magic constant
pub fn dir() -> String {
    // TODO(php-runtime): __DIR__ is the directory of the source file at compile time; it must be supplied
    // per call site (e.g. via a macro), not from a runtime shim function.
    todo!()
}

pub fn memory_get_usage() -> i64 {
    // TODO(phase-c): return PHP's actual emalloc-tracked memory usage instead of a stub 0.
    0
}

pub fn memory_get_peak_usage(_real_usage: bool) -> i64 {
    // TODO(phase-c): return PHP's actual emalloc-tracked peak memory usage instead of a stub 0.
    0
}

pub fn ini_set(_varname: &str, _value: &str) -> Option<String> {
    // TODO(php-runtime): ini_set must return the previous value and have its override observed by a
    // subsequent ini_get; ini_get is currently a static lookup, so overrides cannot be wired up yet.
    todo!()
}

pub fn sapi_windows_vt100_support(_resource: &crate::PhpResource) -> bool {
    // TODO(phase-c): Windows-only SAPI function; not defined on the non-Windows target this build
    // models (function_exists reports it absent).
    todo!()
}

pub fn sapi_windows_cp_get(_kind: Option<&str>) -> i64 {
    // TODO(phase-c): Windows-only SAPI function; see sapi_windows_vt100_support.
    todo!()
}

pub fn sapi_windows_cp_set(_codepage: i64) -> bool {
    // TODO(phase-c): Windows-only SAPI function; see sapi_windows_vt100_support.
    todo!()
}

pub fn sapi_windows_cp_conv(_in_codepage: i64, _out_codepage: i64, _subject: &str) -> String {
    // TODO(phase-c): Windows-only SAPI function; see sapi_windows_vt100_support.
    todo!()
}
