//! ref: composer/src/Composer/Util/Platform.php

use crate::util::ProcessExecutor;
use bstr::{BString, ByteSlice};
use shirabe_php_shim::{
    PHP_ENV, PHP_SERVER, PhpMixed, PhpResource, PregMatches, RuntimeException, STDOUT, bconcat,
    defined, file_exists, file_get_contents, function_exists, getcwd, getenv, ini_get, is_readable,
    mb_strlen, os_str_from_bytes, path_to_bytes, php_os_family, php_regex, posix_geteuid,
    posix_getpwuid, posix_getuid, preg_is_match, preg_replace_callback, putenv, putenv_clear,
    realpath, stream_isatty, stripos, strlen, strtoupper, substr, usleep,
};
use std::sync::Mutex;

/// Platform helper for uniform platform-specific tests.
pub struct Platform;

static IS_VIRTUAL_BOX_GUEST: Mutex<Option<bool>> = Mutex::new(None);
static IS_WINDOWS_SUBSYSTEM_FOR_LINUX: Mutex<Option<bool>> = Mutex::new(None);
static IS_DOCKER: Mutex<Option<bool>> = Mutex::new(None);

impl Platform {
    /// getcwd() equivalent which always returns a string
    pub fn get_cwd(allow_empty: bool) -> anyhow::Result<std::path::PathBuf> {
        let mut cwd = getcwd();

        // fallback to realpath('') just in case this works but odds are it would break as well if we are in a case where getcwd fails
        if cwd.is_none() {
            cwd = realpath("");
        }

        // crappy state, assume '' and hopefully relative paths allow things to continue
        if cwd.is_none() {
            if allow_empty {
                return Ok(std::path::PathBuf::new());
            }

            return Err(RuntimeException::new(
                "Could not determine the current working directory".to_string(),
            )
            .into());
        }

        Ok(cwd.unwrap())
    }

    /// Infallible realpath version that falls back on the given $path if realpath is not working
    pub fn realpath(path: impl AsRef<std::path::Path>) -> std::path::PathBuf {
        let path = path.as_ref();
        let real_path = realpath(path);
        if real_path.is_none() {
            return path.to_path_buf();
        }

        real_path.unwrap()
    }

    /// getenv() equivalent but reads from the runtime global variables first
    pub fn get_env(name: impl AsRef<[u8]>) -> Option<BString> {
        let name = os_str_from_bytes(&name);
        let value = PHP_SERVER.lock().unwrap().get(name);
        let value = value.or_else(|| PHP_ENV.lock().unwrap().get(name));
        let value = value.or_else(|| getenv(name))?;

        Some(BString::from(path_to_bytes(&value)))
    }

    /// putenv() equivalent but updates the runtime global variables too
    pub fn put_env(name: impl AsRef<[u8]>, value: impl AsRef<[u8]>) {
        let name = os_str_from_bytes(&name);
        let value = os_str_from_bytes(&value);
        unsafe { putenv(name, value) };
        PHP_SERVER.lock().unwrap().put(name.into(), value.into());
        PHP_ENV.lock().unwrap().put(name.into(), value.into());
    }

    /// putenv('X') equivalent but updates the runtime global variables too
    pub fn clear_env(name: impl AsRef<[u8]>) {
        let name = os_str_from_bytes(&name);
        unsafe { putenv_clear(name) };
        PHP_SERVER.lock().unwrap().clear(name);
        PHP_ENV.lock().unwrap().clear(name);
    }

    /// Parses tildes and environment variables in paths.
    pub fn expand_path(path: impl AsRef<[u8]>) -> BString {
        let path = path.as_ref();
        if preg_is_match(php_regex!(r"#^~[\\/]#"), path) {
            return bconcat!(Self::get_user_directory().unwrap(), substr(path, 1, None));
        }

        // Regex pattern compatibility:
        // The original pattern uses a conditional subpattern to make the trailing `%` required
        // only for the `%VAR%` form. The Rust regex crate does not support conditionals, so the
        // two forms are written as an explicit alternation: `$VAR` or `%VAR%`.
        preg_replace_callback(
            php_regex!(r"#^(?:\$(?P<dvar>\w+)|%(?P<pvar>\w+)%)(?P<path>.*)#"),
            |matches: &PregMatches| -> anyhow::Result<BString> {
                let var = matches
                    .name("dvar")
                    .or_else(|| matches.name("pvar"))
                    .unwrap_or_default();
                let path_part = matches.name("path").unwrap_or_default();
                // Treat HOME as an alias for USERPROFILE on Windows for legacy reasons
                if Platform::is_windows() && var == "HOME" {
                    let home =
                        Platform::get_env("HOME").filter(|v| PhpMixed::string(v.clone()).to_bool());
                    if let Some(home) = home {
                        return Ok(bconcat!(home, path_part));
                    }

                    return Ok(bconcat!(
                        Platform::get_env("USERPROFILE").unwrap_or_default(),
                        path_part,
                    ));
                }

                Ok(bconcat!(
                    Platform::get_env(var).unwrap_or_default(),
                    path_part,
                ))
            },
            path,
        )
        .expect("the replacement callback cannot fail")
    }

    pub fn get_user_directory() -> anyhow::Result<BString> {
        if let Some(home) = Self::get_env("HOME") {
            return Ok(home);
        }

        if Self::is_windows()
            && let Some(home) = Self::get_env("USERPROFILE")
        {
            return Ok(home);
        }

        if function_exists("posix_getuid")
            && function_exists("posix_getpwuid")
            && let Some(info) = posix_getpwuid(posix_getuid())
        {
            return Ok(BString::from(path_to_bytes(&info.dir)));
        }

        Err(RuntimeException::new("Could not determine user directory".to_string()).into())
    }

    pub fn is_windows_subsystem_for_linux() -> bool {
        let mut cached = IS_WINDOWS_SUBSYSTEM_FOR_LINUX.lock().unwrap();
        if cached.is_none() {
            *cached = Some(false);

            // while WSL will be hosted within windows, WSL itself cannot be windows based itself.
            if Self::is_windows() {
                *cached = Some(false);
                return false;
            }

            let file_contents = file_get_contents("/proc/version").unwrap_or_default();
            if !ini_get("open_basedir").is_some_and(|s| PhpMixed::string(s).to_bool())
                && is_readable("/proc/version")
                // TODO(bytes)
                && stripos(&file_contents, "microsoft").is_some()
                && !Self::is_docker()
            // Docker and Podman running inside WSL should not be seen as WSL
            {
                *cached = Some(true);
                return true;
            }
        }

        cached.unwrap()
    }

    pub fn is_windows() -> bool {
        defined("PHP_WINDOWS_VERSION_BUILD")
    }

    pub fn is_docker() -> bool {
        let mut cached = IS_DOCKER.lock().unwrap();
        if let Some(v) = *cached {
            return v;
        }

        // cannot check so assume no
        if ini_get("open_basedir").is_some_and(|s| PhpMixed::string(s).to_bool()) {
            *cached = Some(false);
            return false;
        }

        // .dockerenv and .containerenv are present in some cases but not reliably
        if file_exists("/.dockerenv")
            || file_exists("/run/.containerenv")
            || file_exists("/var/run/.containerenv")
        {
            *cached = Some(true);
            return true;
        }

        // see https://www.baeldung.com/linux/is-process-running-inside-container
        let cgroups = vec![
            "/proc/self/mountinfo", // cgroup v2
            "/proc/1/cgroup",       // cgroup v1
        ];
        for cgroup in cgroups {
            if !is_readable(cgroup) {
                continue;
            }
            // suppress errors as some environments have these files as readable but system restrictions prevent the read from succeeding
            // see https://github.com/composer/composer/issues/12095
            let data = match file_get_contents(cgroup) {
                Ok(d) => d,
                Err(_) => continue,
            };
            // detect default mount points created by Docker/containerd
            if data.contains_str("/var/lib/docker/")
                || data.contains_str("/io.containerd.snapshotter")
            {
                *cached = Some(true);
                return true;
            }
        }

        *cached = Some(false);
        false
    }

    pub fn strlen(str: &str) -> i64 {
        static USE_MB_STRING: Mutex<Option<bool>> = Mutex::new(None);
        let mut use_mb_string = USE_MB_STRING.lock().unwrap();
        if use_mb_string.is_none() {
            *use_mb_string = Some(
                ini_get("mbstring.func_overload").is_some_and(|s| PhpMixed::string(s).to_bool()),
            );
        }

        if use_mb_string.unwrap() {
            return mb_strlen(str, "8bit");
        }

        strlen(str)
    }

    pub fn is_tty(fd: Option<PhpResource>) -> bool {
        let fd = fd.unwrap_or(STDOUT);

        // detect msysgit/mingw and assume this is a tty because detection
        // does not work correctly, see https://github.com/composer/composer/issues/9690
        if matches!(
            strtoupper(Self::get_env("MSYSTEM").unwrap_or_default()).as_slice(),
            b"MINGW32" | b"MINGW64"
        ) {
            return true;
        }

        stream_isatty(fd)
    }

    /// Whether the current command is for bash completion
    pub fn is_input_completion_process() -> bool {
        std::env::args_os()
            .nth(1)
            .is_some_and(|arg| arg == "_complete")
    }

    pub fn workaround_filesystem_issues() {
        if Self::is_virtual_box_guest() {
            usleep(200_000);
        }
    }

    /// Attempts detection of VirtualBox guest VMs
    ///
    /// This works based on the process' user being "vagrant", the COMPOSER_RUNTIME_ENV env var being set to "virtualbox", or lsmod showing the virtualbox guest additions are loaded
    fn is_virtual_box_guest() -> bool {
        let mut cached = IS_VIRTUAL_BOX_GUEST.lock().unwrap();
        if cached.is_none() {
            *cached = Some(false);
            if Self::is_windows() {
                return cached.unwrap();
            }

            if function_exists("posix_getpwuid") && function_exists("posix_geteuid") {
                let process_user = posix_getpwuid(posix_geteuid());
                if process_user.is_some_and(|process_user| process_user.name == "vagrant") {
                    *cached = Some(true);
                    return true;
                }
            }

            if Self::get_env("COMPOSER_RUNTIME_ENV").is_some_and(|v| v == "virtualbox") {
                *cached = Some(true);
                return true;
            }

            if php_os_family() == "Linux" {
                let process = ProcessExecutor::new(None);
                let mut output = String::new();
                let result: anyhow::Result<()> = (|| {
                    if process.execute(&["lsmod".to_string()], &mut output, None)? == 0
                        && output.contains("vboxguest")
                    {
                        *cached = Some(true);
                        return Ok(());
                    }
                    Ok(())
                })();
                if result.is_ok() && cached.unwrap_or(false) {
                    return true;
                }
                // noop on error
            }
        }

        cached.unwrap_or(false)
    }

    pub fn get_dev_null() -> &'static str {
        if Self::is_windows() {
            return "NUL";
        }

        "/dev/null"
    }
}
