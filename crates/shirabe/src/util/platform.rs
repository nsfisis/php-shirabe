//! ref: composer/src/Composer/Util/Platform.php

use std::sync::Mutex;

use anyhow::Result;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{
    defined, env_contains_key, env_get, env_set, env_unset, file_exists, file_get_contents,
    fopen, fstat, function_exists, getcwd, getenv, in_array, ini_get, is_array, is_readable,
    mb_strlen, posix_geteuid, posix_getpwuid, posix_getuid, posix_isatty, putenv, realpath,
    server_argv, server_contains_key, server_get, server_set, server_unset, stream_isatty,
    stripos, strlen, strtoupper, substr, usleep, PhpMixed, RuntimeException,
};

use crate::util::process_executor::ProcessExecutor;
use crate::util::silencer::Silencer;

/// Platform helper for uniform platform-specific tests.
pub struct Platform;

static IS_VIRTUAL_BOX_GUEST: Mutex<Option<bool>> = Mutex::new(None);
static IS_WINDOWS_SUBSYSTEM_FOR_LINUX: Mutex<Option<bool>> = Mutex::new(None);
static IS_DOCKER: Mutex<Option<bool>> = Mutex::new(None);

impl Platform {
    /// getcwd() equivalent which always returns a string
    ///
    /// @throws \RuntimeException
    pub fn get_cwd(allow_empty: bool) -> Result<String> {
        let mut cwd = getcwd();

        // fallback to realpath('') just in case this works but odds are it would break as well if we are in a case where getcwd fails
        if cwd.is_none() {
            cwd = realpath("");
        }

        // crappy state, assume '' and hopefully relative paths allow things to continue
        if cwd.is_none() {
            if allow_empty {
                return Ok(String::new());
            }

            return Err(RuntimeException {
                message: "Could not determine the current working directory".to_string(),
                code: 0,
            }
            .into());
        }

        Ok(cwd.unwrap())
    }

    /// Infallible realpath version that falls back on the given $path if realpath is not working
    pub fn realpath(path: &str) -> String {
        let real_path = realpath(path);
        if real_path.is_none() {
            return path.to_string();
        }

        real_path.unwrap()
    }

    /// getenv() equivalent but reads from the runtime global variables first
    ///
    /// @param non-empty-string $name
    ///
    /// @return string|false
    pub fn get_env(name: &str) -> Option<String> {
        if server_contains_key(name) {
            return Some(server_get(name).unwrap_or_default());
        }
        if env_contains_key(name) {
            return Some(env_get(name).unwrap_or_default());
        }

        getenv(name)
    }

    /// putenv() equivalent but updates the runtime global variables too
    pub fn put_env(name: &str, value: &str) {
        putenv(&format!("{}={}", name, value));
        server_set(name, value.to_string());
        env_set(name, value.to_string());
    }

    /// putenv('X') equivalent but updates the runtime global variables too
    pub fn clear_env(name: &str) {
        putenv(name);
        server_unset(name);
        env_unset(name);
    }

    /// Parses tildes and environment variables in paths.
    pub fn expand_path(path: &str) -> String {
        if Preg::is_match(r"#^~[\\/]#", path) {
            return format!("{}{}", Self::get_user_directory().unwrap(), substr(path, 1, None));
        }

        Preg::replace_callback(
            r"#^(\$|(?P<percent>%))(?P<var>\w++)(?(percent)%)(?P<path>.*)#",
            |matches| -> String {
                // Treat HOME as an alias for USERPROFILE on Windows for legacy reasons
                if Platform::is_windows()
                    && matches.get("var").map(|s| s.as_str()).unwrap_or("") == "HOME"
                {
                    if Platform::get_env("HOME").is_some() {
                        return format!(
                            "{}{}",
                            Platform::get_env("HOME").unwrap_or_default(),
                            matches.get("path").map(|s| s.as_str()).unwrap_or(""),
                        );
                    }

                    return format!(
                        "{}{}",
                        Platform::get_env("USERPROFILE").unwrap_or_default(),
                        matches.get("path").map(|s| s.as_str()).unwrap_or(""),
                    );
                }

                format!(
                    "{}{}",
                    Platform::get_env(matches.get("var").map(|s| s.as_str()).unwrap_or(""))
                        .unwrap_or_default(),
                    matches.get("path").map(|s| s.as_str()).unwrap_or(""),
                )
            },
            path,
        )
    }

    /// @throws \RuntimeException If the user home could not reliably be determined
    /// @return string            The formal user home as detected from environment parameters
    pub fn get_user_directory() -> Result<String> {
        if let Some(home) = Self::get_env("HOME") {
            return Ok(home);
        }

        if Self::is_windows() {
            if let Some(home) = Self::get_env("USERPROFILE") {
                return Ok(home);
            }
        }

        if function_exists("posix_getuid") && function_exists("posix_getpwuid") {
            let info = posix_getpwuid(posix_getuid());

            if is_array(&info) {
                if let Some(arr) = info.as_array() {
                    if let Some(dir) = arr.get("dir") {
                        if let Some(s) = dir.as_string() {
                            return Ok(s.to_string());
                        }
                    }
                }
            }
        }

        Err(RuntimeException {
            message: "Could not determine user directory".to_string(),
            code: 0,
        }
        .into())
    }

    /// @return bool Whether the host machine is running on the Windows Subsystem for Linux (WSL)
    pub fn is_windows_subsystem_for_linux() -> bool {
        let mut cached = IS_WINDOWS_SUBSYSTEM_FOR_LINUX.lock().unwrap();
        if cached.is_none() {
            *cached = Some(false);

            // while WSL will be hosted within windows, WSL itself cannot be windows based itself.
            if Self::is_windows() {
                *cached = Some(false);
                return false;
            }

            // TODO(phase-b): Silencer::call returns Result; PHP returns the value or false on error
            let file_contents = Silencer::call(|| Ok(file_get_contents("/proc/version")))
                .ok()
                .flatten()
                .unwrap_or_default();
            if !(ini_get("open_basedir").map(|s| !s.is_empty()).unwrap_or(false))
                && is_readable("/proc/version")
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

    /// @return bool Whether the host machine is running a Windows OS
    pub fn is_windows() -> bool {
        defined("PHP_WINDOWS_VERSION_BUILD")
    }

    pub fn is_docker() -> bool {
        let mut cached = IS_DOCKER.lock().unwrap();
        if let Some(v) = *cached {
            return v;
        }

        // cannot check so assume no
        if ini_get("open_basedir").map(|s| !s.is_empty()).unwrap_or(false) {
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
            let data = match Silencer::call(|| Ok(file_get_contents(cgroup))) {
                Ok(d) => d,
                Err(_) => break,
            };
            let data = match data {
                Some(d) => d,
                None => continue,
            };
            // detect default mount points created by Docker/containerd
            if shirabe_php_shim::str_contains(&data, "/var/lib/docker/")
                || shirabe_php_shim::str_contains(&data, "/io.containerd.snapshotter")
            {
                *cached = Some(true);
                return true;
            }
        }

        *cached = Some(false);
        false
    }

    /// @return int    return a guaranteed binary length of the string, regardless of silly mbstring configs
    pub fn strlen(str: &str) -> i64 {
        // TODO(phase-b): function-local static; collapse to a Mutex<Option<bool>> in Phase B
        static USE_MB_STRING: Mutex<Option<bool>> = Mutex::new(None);
        let mut use_mb_string = USE_MB_STRING.lock().unwrap();
        if use_mb_string.is_none() {
            *use_mb_string = Some(
                function_exists("mb_strlen")
                    && ini_get("mbstring.func_overload")
                        .map(|s| !s.is_empty())
                        .unwrap_or(false),
            );
        }

        if use_mb_string.unwrap() {
            return mb_strlen(str, "8bit");
        }

        strlen(str)
    }

    /// @param  ?resource $fd Open file descriptor or null to default to STDOUT
    pub fn is_tty(fd: Option<PhpMixed>) -> bool {
        let fd = match fd {
            Some(f) => f,
            None => {
                if defined("STDOUT") {
                    // TODO(phase-b): map STDOUT to the runtime stdout resource
                    todo!("STDOUT constant")
                } else {
                    let fd = fopen("php://stdout", "w");
                    if matches!(fd, PhpMixed::Bool(false)) {
                        return false;
                    }
                    fd
                }
            }
        };

        // detect msysgit/mingw and assume this is a tty because detection
        // does not work correctly, see https://github.com/composer/composer/issues/9690
        if in_array(
            PhpMixed::String(strtoupper(
                &Self::get_env("MSYSTEM").unwrap_or_default(),
            )),
            &PhpMixed::List(vec![
                Box::new(PhpMixed::String("MINGW32".to_string())),
                Box::new(PhpMixed::String("MINGW64".to_string())),
            ]),
            true,
        ) {
            return true;
        }

        // modern cross-platform function, includes the fstat
        // fallback so if it is present we trust it
        if function_exists("stream_isatty") {
            return stream_isatty(fd);
        }

        // only trusting this if it is positive, otherwise prefer fstat fallback
        if function_exists("posix_isatty") && posix_isatty(fd.clone()) {
            return true;
        }

        // TODO(phase-b): Silencer::call wraps the fstat call (`@fstat($fd)`)
        let stat = Silencer::call(|| Ok(fstat(fd)));
        let stat = match stat {
            Ok(s) => s,
            Err(_) => return false,
        };
        if matches!(stat, PhpMixed::Bool(false)) {
            return false;
        }

        // Check if formatted mode is S_IFCHR
        if let Some(arr) = stat.as_array() {
            if let Some(mode) = arr.get("mode").and_then(|v| v.as_int()) {
                return 0o020000 == (mode & 0o170000);
            }
        }

        false
    }

    /// @return bool Whether the current command is for bash completion
    pub fn is_input_completion_process() -> bool {
        // PHP: $_SERVER['argv'][1] ?? null
        let argv = server_argv();
        argv.get(1).map(|s| s.as_str()) == Some("_complete")
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
                if is_array(&process_user) {
                    if let Some(arr) = process_user.as_array() {
                        if arr
                            .get("name")
                            .and_then(|v| v.as_string())
                            .map(|s| s == "vagrant")
                            .unwrap_or(false)
                        {
                            *cached = Some(true);
                            return true;
                        }
                    }
                }
            }

            if Self::get_env("COMPOSER_RUNTIME_ENV").as_deref() == Some("virtualbox") {
                *cached = Some(true);
                return true;
            }

            if defined("PHP_OS_FAMILY")
                // TODO(phase-b): PHP_OS_FAMILY constant comparison
                && true
            {
                let process = ProcessExecutor::new();
                // TODO(phase-b): inner Result for catch(\Exception); use anyhow::Result<Result<_, _>>
                let mut output = String::new();
                let result: Result<()> = (|| {
                    if process.execute(&["lsmod"], &mut output)? == 0
                        && shirabe_php_shim::str_contains(&output, "vboxguest")
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

    /// @return 'NUL'|'/dev/null'
    pub fn get_dev_null() -> String {
        if Self::is_windows() {
            return "NUL".to_string();
        }

        "/dev/null".to_string()
    }
}
