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
    // TODO(php-semantics): validate key and value format to avoid panic?
    record(EnvStorageKind::Process, key.as_ref(), Some(value.as_ref()));
    unsafe { std::env::set_var(key, value) }
}

/// # Safety
///
/// Wraps [`std::env::remove_var`], which is unsafe: the caller must ensure no other
/// thread is concurrently reading or writing the process environment for the
/// duration of this call.
pub unsafe fn putenv_clear<K: AsRef<std::ffi::OsStr>>(key: K) {
    // TODO(php-semantics): validate key and value format to avoid panic?
    record(EnvStorageKind::Process, key.as_ref(), None);
    unsafe { std::env::remove_var(key) }
}

/// Which of the three environment storages a recorded mutation writes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvStorageKind {
    /// The real process environment, as `putenv()` writes it.
    Process,
    /// The `$_ENV` superglobal.
    Env,
    /// The `$_SERVER` superglobal.
    Server,
}

/// One recorded write. `value` is `None` for an unset.
#[derive(Debug, Clone)]
pub struct EnvMutation {
    pub storage: EnvStorageKind,
    pub key: std::ffi::OsString,
    pub value: Option<std::ffi::OsString>,
}

static ENV_MUTATIONS: std::sync::Mutex<Vec<EnvMutation>> = std::sync::Mutex::new(Vec::new());

fn record(storage: EnvStorageKind, key: &std::ffi::OsStr, value: Option<&std::ffi::OsStr>) {
    ENV_MUTATIONS.lock().unwrap().push(EnvMutation {
        storage,
        key: key.to_os_string(),
        value: value.map(|value| value.to_os_string()),
    });
}

/// The mutations recorded after the first `cursor` ones, and the cursor that follows them.
///
/// A separate PHP runtime holds the environment it was handed when it started; replaying these
/// writes in order is what brings its three storages back in line with this process's.
pub fn env_mutations_since(cursor: usize) -> (usize, Vec<EnvMutation>) {
    let mutations = ENV_MUTATIONS.lock().unwrap();
    (mutations.len(), mutations[cursor..].to_vec())
}

pub struct Superglobal {
    storage: EnvStorageKind,
    vars: indexmap::IndexMap<std::ffi::OsString, std::ffi::OsString>,
}

pub struct SuperglobalServer(Superglobal);

impl Superglobal {
    fn from_env_vars(storage: EnvStorageKind) -> Self {
        let vars = std::env::vars_os().collect();
        Self { storage, vars }
    }

    pub fn get_all(&self) -> impl Iterator<Item = (std::ffi::OsString, std::ffi::OsString)> + '_ {
        self.vars.iter().map(|(k, v)| (k.clone(), v.clone()))
    }

    pub fn get<K: AsRef<std::ffi::OsStr>>(&self, key: K) -> Option<std::ffi::OsString> {
        self.vars.get(key.as_ref()).cloned()
    }

    pub fn put(&mut self, key: std::ffi::OsString, value: std::ffi::OsString) {
        record(self.storage, &key, Some(&value));
        self.vars.insert(key, value);
    }

    pub fn clear<K: AsRef<std::ffi::OsStr>>(&mut self, key: K) {
        record(self.storage, key.as_ref(), None);
        self.vars.shift_remove(key.as_ref());
    }
}

impl SuperglobalServer {
    fn from_env_vars() -> Self {
        Self(Superglobal::from_env_vars(EnvStorageKind::Server))
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
/// TODO(php-runtime): a write PHP code makes to its own $_SERVER is not reflected back here.
pub static PHP_SERVER: std::sync::LazyLock<std::sync::Mutex<SuperglobalServer>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(SuperglobalServer::from_env_vars()));

/// PHP superglobal $_ENV. $_ENV is a snapshot at startup. Modifying it does not affect the real
/// environment variables, while putenv() does.
/// TODO(php-runtime): a write PHP code makes to its own $_ENV is not reflected back here.
pub static PHP_ENV: std::sync::LazyLock<std::sync::Mutex<Superglobal>> =
    std::sync::LazyLock::new(|| {
        std::sync::Mutex::new(Superglobal::from_env_vars(EnvStorageKind::Env))
    });
