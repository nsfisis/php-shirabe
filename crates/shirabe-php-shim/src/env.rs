use crate::PhpMixed;
use indexmap::IndexMap;

pub fn getenv(_name: &str) -> Option<String> {
    std::env::var(_name).ok()
}

// TODO(phase-c): only the simple `^(\w+)(=(.+))?$` form is supported.
pub fn putenv(setting: &str) -> bool {
    let is_word =
        |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
    // A setting without `=` deletes the variable, mirroring PHP's putenv('NAME').
    match setting.split_once('=') {
        Some((name, value)) => {
            if !is_word(name) {
                panic!("putenv: unsupported setting format: {setting:?}");
            }
            unsafe {
                std::env::set_var(name, value);
            }
        }
        None => {
            if !is_word(setting) {
                panic!("putenv: unsupported setting format: {setting:?}");
            }
            unsafe {
                std::env::remove_var(setting);
            }
        }
    }
    true
}

/// PHP superglobal $_SERVER access. In the CLI SAPI $_SERVER is populated from
/// the environment, which is the only source modeled here.
pub fn server_get(name: &str) -> Option<String> {
    // TODO: is var_os() better?
    std::env::var(name).ok()
}

// TODO(php-runtime): modify the real PHP's $_SERVER.
pub fn server_set(_name: &str, _value: String) {}

// TODO(php-runtime): modify the real PHP's $_SERVER.
pub fn server_unset(_name: &str) {}

pub fn server_contains_key(name: &str) -> bool {
    std::env::var_os(name).is_some()
}

/// PHP superglobal $_ENV access.
pub fn env_get(name: &str) -> Option<String> {
    // TODO: is var_os() better?
    std::env::var(name).ok()
}

// TODO(php-runtime): modify the real PHP's $_ENV.
pub fn env_set(_name: &str, _value: String) {}

// TODO(php-runtime): modify the real PHP's $_ENV.
pub fn env_unset(_name: &str) {}

pub fn env_contains_key(name: &str) -> bool {
    std::env::var_os(name).is_some()
}

/// PHP `getenv()` with no argument: all environment variables.
pub fn getenv_all() -> IndexMap<String, String> {
    todo!()
}

/// PHP superglobal `$_ENV`.
pub fn php_env() -> IndexMap<String, PhpMixed> {
    todo!()
}

/// PHP superglobal `$_SERVER`.
pub fn php_server() -> IndexMap<String, PhpMixed> {
    todo!()
}

pub fn server(key: &str) -> String {
    if key == "PHP_SELF" {
        return server_php_self();
    }
    server_get(key).unwrap_or_default()
}

pub fn server_argv() -> Vec<String> {
    std::env::args().collect()
}

pub fn server_php_self() -> String {
    // CLI SAPI: $_SERVER['PHP_SELF'] is the path of the executed script, i.e. argv[0].
    std::env::args().next().unwrap_or_default()
}

pub fn server_shell() -> Option<String> {
    todo!()
}
