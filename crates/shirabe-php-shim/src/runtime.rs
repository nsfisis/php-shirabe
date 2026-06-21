use crate::PhpMixed;
use indexmap::IndexMap;

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

pub const INFO_GENERAL: i64 = 1;
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

pub fn constant(_name: &str) -> PhpMixed {
    todo!()
}

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

pub fn method_exists(_object: &PhpMixed, _method_name: &str) -> bool {
    todo!()
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
    todo!()
}

pub fn phpversion(_extension: &str) -> Option<String> {
    todo!()
}

// TODO(php-runtime): the callback should be registered in PHP runtime.
pub fn set_error_handler(_callback: fn(i64, &str, &str, i64) -> bool) {}

pub fn debug_backtrace() -> Vec<IndexMap<String, PhpMixed>> {
    todo!()
}

/// Equivalent to PHP `include $file;`
pub fn include_file(file: &str) -> PhpMixed {
    let _ = file;
    todo!()
}

pub fn spl_autoload_register(
    callback: Box<dyn Fn(&str) -> PhpMixed + Send + Sync>,
    throw: bool,
    prepend: bool,
) -> bool {
    let _ = (callback, throw, prepend);
    todo!()
}

pub fn spl_autoload_unregister(callback: Box<dyn Fn(&str) -> PhpMixed + Send + Sync>) -> bool {
    let _ = callback;
    todo!()
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
    todo!()
}

pub fn version_compare(_v1: &str, _v2: &str, _op: &str) -> bool {
    todo!()
}

pub fn version_compare_2(_v1: &str, _v2: &str) -> i64 {
    todo!()
}

// TODO(php-runtime): the previous handler should be restored in the PHP runtime.
// Paired with set_error_handler, which is a no-op in this shim.
pub fn restore_error_handler() {}

pub fn spl_object_hash<T: ?Sized>(_object: &T) -> String {
    todo!()
}

pub fn spl_object_hash_process<T>(_object: &T) -> String {
    todo!()
}

pub fn php_uname(mode: &str) -> String {
    match mode {
        // sysname, as reported by uname(2). On Windows PHP returns "Windows NT",
        // which differs from PHP_OS.
        "s" => match std::env::consts::OS {
            "linux" => "Linux",
            "macos" => "Darwin",
            "windows" => "Windows NT",
            "freebsd" => "FreeBSD",
            "netbsd" => "NetBSD",
            "openbsd" => "OpenBSD",
            "dragonfly" => "DragonFly",
            "solaris" => "SunOS",
            other => other,
        }
        .to_string(),
        // TODO(phase-c): use libc?
        // release, as reported by uname(2). On Linux this matches the contents
        // of /proc/sys/kernel/osrelease.
        "r" => std::fs::read_to_string("/proc/sys/kernel/osrelease")
            .map(|s| s.trim_end().to_string())
            .unwrap_or_default(),
        _ => todo!(),
    }
}

pub fn trigger_error(_message: &str, _error_level: i64) {
    todo!()
}

pub fn trigger_deprecation(_package: &str, _version: &str, _message: &str, _arg: &str) {
    todo!()
}

pub fn usleep(_microseconds: u64) {
    std::thread::sleep(std::time::Duration::from_micros(_microseconds));
}

/// Equivalent to PHP's __DIR__ magic constant
pub fn php_dir() -> String {
    todo!()
}

pub fn dir() -> String {
    todo!()
}

/// Equivalent to PHP's `require <file>` returning the file's return value
pub fn require_php_file(_filename: &str) -> PhpMixed {
    todo!()
}

pub fn php_require(_file: &str) -> PhpMixed {
    todo!()
}

pub fn r#eval(_code: &str) -> PhpMixed {
    todo!()
}

pub fn memory_get_usage() -> i64 {
    todo!()
}

pub fn memory_get_peak_usage(_real_usage: bool) -> i64 {
    todo!()
}

pub fn call_user_func<T>(_callback: &str, _args: &[PhpMixed]) -> T
where
    T: From<PhpMixed>,
{
    todo!()
}

pub fn call_user_func_array(_callback: &str, _args: &PhpMixed) -> PhpMixed {
    todo!()
}

pub fn call_php_callable(_callback: &PhpMixed, _args: &[PhpMixed]) -> PhpMixed {
    todo!()
}

// The shim does not raise PHP-level errors, so there is never a last error.
pub fn error_get_last() -> Option<IndexMap<String, PhpMixed>> {
    None
}

pub fn globals_get(_name: &str) -> PhpMixed {
    todo!()
}

pub fn globals_set(_name: &str, _value: PhpMixed) {
    todo!()
}

pub fn clone<T: Clone>(_value: T) -> T {
    todo!()
}

pub fn ini_set(_varname: &str, _value: &str) -> Option<String> {
    todo!()
}

thread_local! {
    static SHUTDOWN_FUNCTIONS: std::cell::RefCell<Vec<Box<dyn Fn()>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

pub fn register_shutdown_function(callback: Box<dyn Fn()>) {
    SHUTDOWN_FUNCTIONS.with(|f| f.borrow_mut().push(callback));
}

// Runs the registered shutdown functions in registration order, mirroring PHP
// executing them at the end of the request. Must be invoked at every process exit.
pub fn run_shutdown_functions() {
    let functions = SHUTDOWN_FUNCTIONS.with(|f| std::mem::take(&mut *f.borrow_mut()));
    for callback in &functions {
        callback();
    }
}

pub fn composer_dev_warning_time() -> i64 {
    todo!()
}

pub fn gc_collect_cycles() -> i64 {
    // Rust has no cycle collector; nothing is collected.
    0
}

pub fn gc_disable() {
    // Rust has no cycle collector to disable.
}

pub fn gc_enable() {
    // Rust has no cycle collector to enable.
}

pub fn react_promise_resolve(_value: PhpMixed) -> PhpMixed {
    todo!()
}

pub fn ioncube_loader_iversion() -> i64 {
    todo!()
}

pub fn ioncube_loader_version() -> String {
    todo!()
}

pub fn phpinfo(_what: i64) {
    todo!()
}

pub fn exit(status: i64) -> ! {
    // PHP runs registered shutdown functions before terminating.
    run_shutdown_functions();
    std::process::exit(status as i32);
}

pub fn sapi_windows_vt100_support(_resource: &crate::PhpResource) -> bool {
    todo!()
}

pub fn sapi_windows_cp_get(_kind: Option<&str>) -> i64 {
    todo!()
}

pub fn sapi_windows_cp_set(_codepage: i64) -> bool {
    todo!()
}

pub fn sapi_windows_cp_conv(_in_codepage: i64, _out_codepage: i64, _subject: &str) -> String {
    todo!()
}
