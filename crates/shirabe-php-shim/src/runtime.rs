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
    // TODO(phase-d): resolving a constant by name needs a runtime constant registry, which the shim
    // does not provide (constants are ported as Rust `const`s, not looked up by string).
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
    // TODO(phase-d): requires runtime class/method reflection, which PhpMixed::Object does not carry.
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
    // Mirrors the set recognized by extension_loaded().
    // TODO(phase-d): this models only the Composer-relevant subset, not PHP's full extension list
    // (Core, standard, date, pcre, ...).
    [
        "Phar", "curl", "filter", "hash", "iconv", "intl", "mbstring", "openssl", "zip", "zlib",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

pub fn phpversion(_extension: &str) -> Option<String> {
    if _extension.is_empty() {
        Some(PHP_VERSION.to_string())
    } else {
        // TODO(phase-d): per-extension version strings are not modeled; PHP returns the extension's
        // own version, or false when the extension is not loaded.
        todo!()
    }
}

// TODO(php-runtime): the callback should be registered in PHP runtime.
pub fn set_error_handler(_callback: fn(i64, &str, &str, i64) -> bool) {}

pub fn debug_backtrace() -> Vec<IndexMap<String, PhpMixed>> {
    // TODO(phase-d): capturing a PHP-style call stack requires runtime introspection of the
    // interpreter frames, which has no equivalent in the compiled shim.
    todo!()
}

/// Equivalent to PHP `include $file;`
pub fn include_file(file: &str) -> PhpMixed {
    let _ = file;
    // TODO(phase-d): `include` evaluates a PHP source file at runtime; there is no PHP interpreter.
    todo!()
}

pub fn spl_autoload_register(
    callback: Box<dyn Fn(&str) -> PhpMixed + Send + Sync>,
    throw: bool,
    prepend: bool,
) -> bool {
    let _ = (callback, throw, prepend);
    // TODO(phase-d): class autoloading has no analogue in compiled Rust (classes are not loaded by
    // name at runtime).
    todo!()
}

pub fn spl_autoload_unregister(callback: Box<dyn Fn(&str) -> PhpMixed + Send + Sync>) -> bool {
    let _ = callback;
    // TODO(phase-d): see spl_autoload_register.
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
    // TODO(phase-d): see spl_autoload_register; no autoload registry exists.
    todo!()
}

pub fn version_compare(_v1: &str, _v2: &str, _op: &str) -> bool {
    let c = php_version_compare(_v1, _v2);
    match _op {
        "<" | "lt" => c < 0,
        "<=" | "le" => c <= 0,
        ">" | "gt" => c > 0,
        ">=" | "ge" => c >= 0,
        "==" | "=" | "eq" => c == 0,
        "!=" | "<>" | "ne" => c != 0,
        // TODO(phase-d): PHP returns null for an unknown operator; this bool signature reports false.
        _ => false,
    }
}

pub fn version_compare_2(_v1: &str, _v2: &str) -> i64 {
    php_version_compare(_v1, _v2) as i64
}

// Port of PHP's php_version_compare (ext/standard/versioning.c). Returns -1, 0 or 1.
fn php_version_compare(v1: &str, v2: &str) -> i32 {
    if v1.is_empty() || v2.is_empty() {
        return match (v1.is_empty(), v2.is_empty()) {
            (true, true) => 0,
            (false, _) => 1,
            (_, false) => -1,
        };
    }
    let c1 = canonicalize_version(v1);
    let c2 = canonicalize_version(v2);
    let t1: Vec<&str> = c1.split('.').filter(|s| !s.is_empty()).collect();
    let t2: Vec<&str> = c2.split('.').filter(|s| !s.is_empty()).collect();

    let mut compare = 0;
    let mut i = 0;
    while i < t1.len() && i < t2.len() && compare == 0 {
        compare = version_token_compare(t1[i], t2[i]);
        i += 1;
    }
    if compare == 0 {
        // A leftover numeric token wins; a leftover special form is compared against the implicit
        // release baseline ("#", order 4).
        if i < t1.len() {
            let p = t1[i];
            compare = if p.as_bytes()[0].is_ascii_digit() {
                1
            } else {
                special_form_order(p).cmp(&4) as i32
            };
        } else if i < t2.len() {
            let p = t2[i];
            compare = if p.as_bytes()[0].is_ascii_digit() {
                -1
            } else {
                4.cmp(&special_form_order(p)) as i32
            };
        }
    }
    compare
}

// PHP's php_canonicalize_version: separators (-, _, +, .) collapse to a single '.', and a '.' is
// inserted at every digit <-> non-digit boundary.
fn canonicalize_version(version: &str) -> String {
    let bytes = version.as_bytes();
    if bytes.is_empty() {
        return String::new();
    }
    let mut q: Vec<u8> = Vec::with_capacity(bytes.len() * 2);
    q.push(bytes[0]);
    for &raw in &bytes[1..] {
        let ch = if matches!(raw, b'-' | b'_' | b'+') {
            b'.'
        } else {
            raw
        };
        let last = *q.last().unwrap();
        if ch == b'.' {
            if last != b'.' {
                q.push(b'.');
            }
        } else if last.is_ascii_digit() != ch.is_ascii_digit() {
            q.push(b'.');
            q.push(ch);
        } else {
            q.push(ch);
        }
    }
    String::from_utf8_lossy(&q).into_owned()
}

fn version_token_compare(t1: &str, t2: &str) -> i32 {
    let d1 = t1.as_bytes()[0].is_ascii_digit();
    let d2 = t2.as_bytes()[0].is_ascii_digit();
    if d1 && d2 {
        let l1 = t1.parse::<i64>().unwrap_or(0);
        let l2 = t2.parse::<i64>().unwrap_or(0);
        l1.cmp(&l2) as i32
    } else if !d1 && !d2 {
        special_form_order(t1).cmp(&special_form_order(t2)) as i32
    } else if d1 {
        // A numeric token is treated as the "#" form (order 4).
        4.cmp(&special_form_order(t2)) as i32
    } else {
        special_form_order(t1).cmp(&4) as i32
    }
}

fn special_form_order(form: &str) -> i32 {
    const FORMS: &[(&str, i32)] = &[
        ("dev", 0),
        ("alpha", 1),
        ("a", 1),
        ("beta", 2),
        ("b", 2),
        ("RC", 3),
        ("rc", 3),
        ("#", 4),
        ("pl", 5),
        ("p", 5),
    ];
    for (name, order) in FORMS {
        if form.starts_with(name) {
            return *order;
        }
    }
    -1
}

// TODO(php-runtime): the previous handler should be restored in the PHP runtime.
// Paired with set_error_handler, which is a no-op in this shim.
pub fn restore_error_handler() {}

pub fn spl_object_hash<T: ?Sized>(_object: &T) -> String {
    // PHP returns a unique 32-char hex id per object instance; the object's address serves as the
    // identity here.
    // TODO(phase-d): as in PHP, an address can be reused after an object is freed, so uniqueness is
    // not guaranteed across an object's whole lifetime without an object store.
    format!("{:032x}", _object as *const T as *const u8 as usize)
}

pub fn spl_object_hash_process<T>(_object: &T) -> String {
    format!("{:032x}", _object as *const T as usize)
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
        // TODO(phase-d): the remaining php_uname() modes ("n", "v", "m", "a") need uname(2) fields
        // (nodename/version/machine) that are not reachable without a libc/syscall crate.
        _ => todo!(),
    }
}

pub fn trigger_error(_message: &str, _error_level: i64) {
    // TODO(phase-d): emitting a PHP error obeys error_reporting and the installed error handler
    // (both runtime state not modeled here); writing unconditionally to stderr would diverge.
    todo!()
}

pub fn trigger_deprecation(_package: &str, _version: &str, _message: &str, _arg: &str) {
    // TODO(phase-d): symfony/deprecation-contracts triggers an E_USER_DEPRECATED via the error
    // subsystem, which is not modeled (see trigger_error).
    todo!()
}

pub fn usleep(_microseconds: u64) {
    std::thread::sleep(std::time::Duration::from_micros(_microseconds));
}

/// Equivalent to PHP's __DIR__ magic constant
pub fn php_dir() -> String {
    // TODO(phase-d): __DIR__ is the directory of the source file at compile time; it must be supplied
    // per call site (e.g. via a macro), not from a runtime shim function.
    todo!()
}

pub fn dir() -> String {
    // TODO(phase-d): see php_dir; __DIR__ is a per-source-file compile-time value.
    todo!()
}

/// Equivalent to PHP's `require <file>` returning the file's return value
pub fn require_php_file(_filename: &str) -> PhpMixed {
    // TODO(phase-d): `require` evaluates a PHP source file at runtime; there is no PHP interpreter.
    todo!()
}

pub fn php_require(_file: &str) -> PhpMixed {
    // TODO(phase-d): see require_php_file.
    todo!()
}

pub fn r#eval(_code: &str) -> PhpMixed {
    // TODO(phase-d): `eval` runs arbitrary PHP source at runtime; there is no PHP interpreter.
    todo!()
}

pub fn memory_get_usage() -> i64 {
    // TODO(phase-d): PHP reports memory tracked by its own allocator (emalloc); Rust has no
    // equivalent accounting, and process RSS would not match PHP's semantics.
    todo!()
}

pub fn memory_get_peak_usage(_real_usage: bool) -> i64 {
    // TODO(phase-d): see memory_get_usage; PHP's peak emalloc figure has no Rust equivalent.
    todo!()
}

pub fn call_user_func<T>(_callback: &str, _args: &[PhpMixed]) -> T
where
    T: From<PhpMixed>,
{
    // TODO(phase-d): invoking a function by name needs a runtime function registry; the shim has no
    // way to resolve a callable from a string.
    todo!()
}

pub fn call_user_func_array(_callback: &str, _args: &PhpMixed) -> PhpMixed {
    // TODO(phase-d): see call_user_func.
    todo!()
}

pub fn call_php_callable(_callback: &PhpMixed, _args: &[PhpMixed]) -> PhpMixed {
    // TODO(phase-d): PhpMixed carries no callable variant; a runtime callable cannot be invoked.
    todo!()
}

// The shim does not raise PHP-level errors, so there is never a last error.
pub fn error_get_last() -> Option<IndexMap<String, PhpMixed>> {
    None
}

pub fn globals_get(_name: &str) -> PhpMixed {
    // TODO(phase-d): the PHP $GLOBALS superglobal is not modeled in the shim.
    todo!()
}

pub fn globals_set(_name: &str, _value: PhpMixed) {
    // TODO(phase-d): the PHP $GLOBALS superglobal is not modeled in the shim.
    todo!()
}

pub fn clone<T: Clone>(_value: T) -> T {
    // PHP's `clone` makes a (shallow) copy of an object; Rust's Clone is the closest equivalent.
    _value.clone()
}

pub fn ini_set(_varname: &str, _value: &str) -> Option<String> {
    // TODO(phase-d): ini_set must return the previous value and have its override observed by a
    // subsequent ini_get; ini_get is currently a static lookup, so overrides cannot be wired up yet.
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
    // TODO(phase-d): COMPOSER_DEV_WARNING_TIME is a build-time constant baked into Composer's release
    // artifact; it has no fixed value in source and must be provided by the build process.
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
    // TODO(phase-d): depends on the react/promise port (shirabe_external_packages), which is not yet
    // available.
    todo!()
}

pub fn ioncube_loader_iversion() -> i64 {
    // TODO(phase-d): the ionCube loader is not present (extension_loaded reports it absent), so this
    // function is never defined at runtime; left unimplemented.
    todo!()
}

pub fn ioncube_loader_version() -> String {
    // TODO(phase-d): see ioncube_loader_iversion.
    todo!()
}

pub fn phpinfo(_what: i64) {
    // TODO(phase-d): phpinfo() dumps the full PHP runtime configuration, which the shim does not
    // model.
    todo!()
}

pub fn exit(status: i64) -> ! {
    // PHP runs registered shutdown functions before terminating.
    run_shutdown_functions();
    std::process::exit(status as i32);
}

pub fn sapi_windows_vt100_support(_resource: &crate::PhpResource) -> bool {
    // TODO(phase-d): Windows-only SAPI function; not defined on the non-Windows target this build
    // models (function_exists reports it absent).
    todo!()
}

pub fn sapi_windows_cp_get(_kind: Option<&str>) -> i64 {
    // TODO(phase-d): Windows-only SAPI function; see sapi_windows_vt100_support.
    todo!()
}

pub fn sapi_windows_cp_set(_codepage: i64) -> bool {
    // TODO(phase-d): Windows-only SAPI function; see sapi_windows_vt100_support.
    todo!()
}

pub fn sapi_windows_cp_conv(_in_codepage: i64, _out_codepage: i64, _subject: &str) -> String {
    // TODO(phase-d): Windows-only SAPI function; see sapi_windows_vt100_support.
    todo!()
}
