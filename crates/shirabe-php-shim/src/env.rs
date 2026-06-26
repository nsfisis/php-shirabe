//! Porting rules for PHP environment variables: see `docs/dev/env-vars-porting.md`.

pub fn getenv_all() -> std::env::VarsOs {
    std::env::vars_os()
}

pub fn getenv<K: AsRef<std::ffi::OsStr>>(key: K) -> Option<std::ffi::OsString> {
    std::env::var_os(key)
}

/// # Safety
///
/// Wraps [`std::env::set_var`], which is unsafe: the caller must ensure no other
/// thread is concurrently reading or writing the process environment for the
/// duration of this call.
pub unsafe fn putenv<K: AsRef<std::ffi::OsStr>, V: AsRef<std::ffi::OsStr>>(key: K, value: V) {
    // TODO: validate key and value format to avoid panic?
    unsafe { std::env::set_var(key, value) }
}

/// # Safety
///
/// Wraps [`std::env::remove_var`], which is unsafe: the caller must ensure no other
/// thread is concurrently reading or writing the process environment for the
/// duration of this call.
pub unsafe fn putenv_clear<K: AsRef<std::ffi::OsStr>>(key: K) {
    // TODO: validate key and value format to avoid panic?
    unsafe { std::env::remove_var(key) }
}

pub struct Superglobal {
    vars: indexmap::IndexMap<std::ffi::OsString, std::ffi::OsString>,
}

pub struct SuperglobalServer(Superglobal);

impl Superglobal {
    fn from_env_vars() -> Self {
        let vars = std::env::vars_os().collect();
        Self { vars }
    }

    pub fn get_all(&self) -> impl Iterator<Item = (std::ffi::OsString, std::ffi::OsString)> + '_ {
        self.vars.iter().map(|(k, v)| (k.clone(), v.clone()))
    }

    pub fn get<K: AsRef<std::ffi::OsStr>>(&self, key: K) -> Option<std::ffi::OsString> {
        self.vars.get(key.as_ref()).cloned()
    }

    pub fn put(&mut self, key: std::ffi::OsString, value: std::ffi::OsString) {
        self.vars.insert(key, value);
    }

    pub fn clear<K: AsRef<std::ffi::OsStr>>(&mut self, key: K) {
        self.vars.shift_remove(key.as_ref());
    }
}

impl SuperglobalServer {
    fn from_env_vars() -> Self {
        Self(Superglobal::from_env_vars())
    }

    pub fn get_all(&self) -> impl Iterator<Item = (std::ffi::OsString, std::ffi::OsString)> + '_ {
        self.0.get_all()
    }

    pub fn get<K: AsRef<std::ffi::OsStr>>(&self, key: K) -> Option<std::ffi::OsString> {
        self.0.get(key)
    }

    pub fn put(&mut self, key: std::ffi::OsString, value: std::ffi::OsString) {
        self.0.put(key, value)
    }

    pub fn clear<K: AsRef<std::ffi::OsStr>>(&mut self, key: K) {
        self.0.clear(key)
    }

    pub fn argv(&self) -> std::env::ArgsOs {
        std::env::args_os()
    }

    pub fn php_self(&self) -> Option<std::ffi::OsString> {
        self.argv().next()
    }
}

/// PHP superglobal $_SERVER. $_SERVER is a snapshot at startup. Modifying it does not affect the
/// real environment variables, while putenv() does.
/// TODO(php-runtime): modify the real PHP's $_SERVER.
pub static PHP_SERVER: std::sync::LazyLock<std::sync::Mutex<SuperglobalServer>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(SuperglobalServer::from_env_vars()));

/// PHP superglobal $_ENV. $_ENV is a snapshot at startup. Modifying it does not affect the real
/// environment variables, while putenv() does.
/// TODO(php-runtime): modify the real PHP's $_ENV.
pub static PHP_ENV: std::sync::LazyLock<std::sync::Mutex<Superglobal>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(Superglobal::from_env_vars()));
