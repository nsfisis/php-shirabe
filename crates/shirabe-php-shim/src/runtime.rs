use crate::{PhpResource, bformat, php_str};
use bstr::{BStr, BString, ByteSlice as _};
use shirabe_php_src::standard::versioning::php_version_compare;

pub const E_ALL: i64 = 32767;
pub const E_WARNING: i64 = 2;
pub const E_NOTICE: i64 = 8;
pub const E_USER_WARNING: i64 = 512;
pub const E_USER_NOTICE: i64 = 1024;
pub const E_DEPRECATED: i64 = 8192;
pub const E_USER_DEPRECATED: i64 = 16384;

pub const PHP_BINARY: &BStr = php_str(b"");

pub const PHP_OS: &BStr = php_str(match std::env::consts::OS.as_bytes() {
    b"linux" => b"Linux",
    b"macos" => b"Darwin",
    b"windows" => b"WINNT",
    b"freebsd" => b"FreeBSD",
    b"openbsd" => b"OpenBSD",
    b"netbsd" => b"NetBSD",
    b"dragonfly" => b"DragonFly",
    b"solaris" | b"illumos" => b"SunOS",
    _ => std::env::consts::OS.as_bytes(),
});

// Models the constants defined in a standard modern PHP CLI environment on a
// non-Windows platform with the common extensions loaded (curl, openssl, json).
// Windows-only and Composer-bootstrap constants are reported undefined.
pub fn defined(name: impl AsRef<[u8]>) -> bool {
    let name = BStr::new(name.as_ref());
    matches!(
        name.as_bytes(),
        b"CURLMOPT_MAX_HOST_CONNECTIONS"
            | b"CURL_HTTP_VERSION_2_0"
            | b"CURL_HTTP_VERSION_3"
            | b"CURL_VERSION_HTTP2"
            | b"CURL_VERSION_HTTP3"
            | b"CURL_VERSION_HTTPS_PROXY"
            | b"CURL_VERSION_LIBZ"
            | b"CURL_VERSION_ZSTD"
            | b"GLOB_BRACE"
            | b"OPENSSL_VERSION_TEXT"
            | b"PHP_BINARY"
            | b"SIGINT"
            | b"STDIN"
            | b"STDOUT"
    )
}

// Models the functions available in a standard modern PHP CLI environment on a
// non-Windows platform with the common extensions loaded (curl, mbstring, iconv,
// zlib, posix, pcntl). Opt-in or Windows-only functions are reported absent.
pub fn function_exists(name: impl AsRef<[u8]>) -> bool {
    let name = BStr::new(name.as_ref());
    matches!(
        name.as_bytes(),
        b"bzcompress"
            | b"cli_set_process_title"
            | b"curl_multi_exec"
            | b"curl_multi_init"
            | b"curl_multi_setopt"
            | b"curl_share_init"
            | b"curl_strerror"
            | b"disk_free_space"
            | b"exec"
            | b"filter_var"
            | b"getmypid"
            | b"gzcompress"
            | b"iconv"
            | b"ini_set"
            | b"json_decode"
            | b"mb_check_encoding"
            | b"mb_convert_encoding"
            | b"mb_strlen"
            | b"pcntl_async_signals"
            | b"pcntl_signal"
            | b"php_strip_whitespace"
            | b"php_uname"
            | b"posix_geteuid"
            | b"posix_getpwuid"
            | b"posix_getuid"
            | b"posix_isatty"
            | b"proc_open"
            | b"putenv"
            | b"shell_exec"
            | b"stream_isatty"
            | b"symlink"
    )
}

/// PHP `PHP_OS_FAMILY` constant: the family of the host OS.
/// One of "Windows", "BSD", "Darwin", "Solaris", "Linux", "Unknown".
pub fn php_os_family() -> &'static BStr {
    BStr::new(match std::env::consts::OS {
        "linux" | "android" => "Linux",
        "macos" | "ios" => "Darwin",
        "windows" => "Windows",
        "freebsd" | "dragonfly" | "netbsd" | "openbsd" => "BSD",
        "solaris" | "illumos" => "Solaris",
        _ => "Unknown",
    })
}

// Models the extensions loaded in a standard PHP CLI environment running Composer.
// Opt-in extensions (apcu, xdebug, ionCube, uopz) are reported absent.
pub fn extension_loaded(name: impl AsRef<[u8]>) -> bool {
    matches!(
        name.as_ref().as_bytes(),
        b"Phar"
            | b"curl"
            | b"filter"
            | b"hash"
            | b"iconv"
            | b"intl"
            | b"mbstring"
            | b"openssl"
            | b"pcre"
            | b"zip"
            | b"zlib"
    )
}

// Models the configuration of a standard PHP CLI environment. Settings belonging
// to extensions that are not loaded (apcu, uopz, xdebug) are not registered, so
// PHP's ini_get returns false (None) for them.
pub fn ini_get(option: impl AsRef<[u8]>) -> Option<BString> {
    let option = BStr::new(option.as_ref());
    match option.as_bytes() {
        b"allow_url_fopen" => Some(BString::from("1")),
        b"default_socket_timeout" => Some(BString::from("60")),
        b"disable_functions" => Some(BString::default()),
        b"mbstring.func_overload" => Some(BString::from("0")),
        b"memory_limit" => Some(BString::from("-1")),
        b"open_basedir" => Some(BString::default()),
        _ => None,
    }
}

pub fn get_loaded_extensions() -> Vec<BString> {
    // Mirrors the set recognized by extension_loaded().
    // TODO(php-runtime): this models only the Composer-relevant subset, not PHP's full extension list
    // (Core, standard, date, pcre, ...).
    [
        "Phar", "curl", "filter", "hash", "iconv", "intl", "mbstring", "openssl", "zip", "zlib",
    ]
    .iter()
    .map(|s| BString::from(*s))
    .collect()
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

impl CmpOp {
    /// The operator as PHP's `version_compare()` spells it.
    pub fn as_str(self) -> &'static BStr {
        BStr::new(match self {
            CmpOp::Lt => "<",
            CmpOp::Le => "<=",
            CmpOp::Eq => "==",
            CmpOp::Ne => "!=",
            CmpOp::Ge => ">=",
            CmpOp::Gt => ">",
        })
    }
}

pub fn version_compare(v1: impl AsRef<[u8]>, v2: impl AsRef<[u8]>, op: CmpOp) -> bool {
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

pub fn version_compare_ordering(v1: impl AsRef<[u8]>, v2: impl AsRef<[u8]>) -> std::cmp::Ordering {
    let ord = php_version_compare(v1, v2);
    if ord < 0 {
        std::cmp::Ordering::Less
    } else if ord > 0 {
        std::cmp::Ordering::Greater
    } else {
        std::cmp::Ordering::Equal
    }
}

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

pub fn spl_object_hash<T: HasAddress>(object: T) -> BString {
    use std::io::Write as _;
    let mut out = BString::default();
    write!(out, "{:032x}", object.address()).expect("writing to a Vec cannot fail");
    out
}

// TODO(windows): the Windows branch of php_uname is missing. There PHP reports "Windows NT" as the
// sysname and derives release/version from the OS version APIs rather than uname(2).
pub fn php_uname(mode: impl AsRef<[u8]>) -> BString {
    let Ok(utsname) = nix::sys::utsname::uname() else {
        return BString::default();
    };
    let field = |value: &std::ffi::OsStr| BString::from(value.as_encoded_bytes());
    match mode.as_ref() {
        b"s" => field(utsname.sysname()),
        b"n" => field(utsname.nodename()),
        b"r" => field(utsname.release()),
        b"v" => field(utsname.version()),
        b"m" => field(utsname.machine()),
        // "a" is the default and any other mode falls back to it: every field in the order
        // sysname, nodename, release, version, machine.
        _ => bformat!(
            "{} {} {} {} {}",
            field(utsname.sysname()),
            field(utsname.nodename()),
            field(utsname.release()),
            field(utsname.version()),
            field(utsname.machine())
        ),
    }
}

pub fn trigger_error(_message: impl AsRef<[u8]>, _error_level: i64) {
    let _message = BStr::new(_message.as_ref());
    // TODO(php-runtime): emitting a PHP error obeys error_reporting and the installed error handler
    // (both runtime state not modeled here); writing unconditionally to stderr would diverge.
    todo!()
}

pub fn trigger_deprecation(
    _package: impl AsRef<[u8]>,
    _version: impl AsRef<[u8]>,
    _message: impl AsRef<[u8]>,
    _arg: impl AsRef<[u8]>,
) {
    let _package = BStr::new(_package.as_ref());
    let _version = BStr::new(_version.as_ref());
    let _message = BStr::new(_message.as_ref());
    let _arg = BStr::new(_arg.as_ref());
    // TODO(php-runtime): symfony/deprecation-contracts triggers an E_USER_DEPRECATED via the error
    // subsystem, which is not modeled (see trigger_error).
    todo!()
}

pub fn usleep(microseconds: u64) {
    std::thread::sleep(std::time::Duration::from_micros(microseconds));
}

/// Equivalent to PHP's __DIR__ magic constant
pub fn dir() -> BString {
    // TODO(php-runtime): __DIR__ is the directory of the source file at compile time; it must be supplied
    // per call site (e.g. via a macro), not from a runtime shim function.
    todo!()
}

pub fn ini_set(_varname: impl AsRef<[u8]>, _value: impl AsRef<[u8]>) -> Option<BString> {
    let _varname = BStr::new(_varname.as_ref());
    let _value = BStr::new(_value.as_ref());
    // TODO(php-runtime): ini_set must return the previous value and have its override observed by a
    // subsequent ini_get; ini_get is currently a static lookup, so overrides cannot be wired up yet.
    todo!()
}

pub fn sapi_windows_vt100_support(_resource: &PhpResource) -> bool {
    // TODO(windows): Windows-only SAPI function; not defined on the non-Windows target this build
    // models (function_exists reports it absent).
    todo!()
}

pub fn sapi_windows_cp_get(_kind: Option<&BStr>) -> i64 {
    // TODO(windows): Windows-only SAPI function; see sapi_windows_vt100_support.
    todo!()
}

pub fn sapi_windows_cp_set(_codepage: i64) -> bool {
    // TODO(windows): Windows-only SAPI function; see sapi_windows_vt100_support.
    todo!()
}

pub fn sapi_windows_cp_conv(_in_codepage: i64, _out_codepage: i64, _subject: &[u8]) -> BString {
    // TODO(windows): Windows-only SAPI function; see sapi_windows_vt100_support.
    todo!()
}
