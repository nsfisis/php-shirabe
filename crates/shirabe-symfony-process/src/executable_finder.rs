//! ref: composer/vendor/symfony/process/ExecutableFinder.php

use shirabe_php_shim::{
    PATHINFO_EXTENSION, PHP_EOL, basename, bconcat, escapeshellarg, exec, getenv, is_dir,
    is_executable, is_file, legacy_lossy, path_from_bytes, pathinfo, strcspn, strpos, strtolower,
    substr,
};

const CMD_BUILTINS: &[&str] = &[
    "assoc", "break", "call", "cd", "chdir", "cls", "color", "copy", "date", "del", "dir", "echo",
    "endlocal", "erase", "exit", "for", "ftype", "goto", "help", "if", "label", "md", "mkdir",
    "mklink", "move", "path", "pause", "popd", "prompt", "pushd", "rd", "rem", "ren", "rename",
    "rmdir", "set", "setlocal", "shift", "start", "time", "title", "type", "ver", "vol",
];

#[derive(Debug)]
pub struct ExecutableFinder {
    suffixes: Vec<String>,
}

impl Default for ExecutableFinder {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutableFinder {
    pub fn new() -> Self {
        Self { suffixes: vec![] }
    }

    pub fn find(&self, name: &str, default: Option<&str>, extra_dirs: &[String]) -> Option<String> {
        // windows built-in commands that are present in cmd.exe should not be resolved using PATH as they do not exist as exes
        if cfg!(windows) && CMD_BUILTINS.iter().any(|b| strtolower(name) == *b) {
            return Some(name.to_string());
        }

        let path = getenv("PATH")
            .or_else(|| getenv("Path"))
            .unwrap_or_default();
        let mut dirs: Vec<String> = std::env::split_paths(&path)
            .map(|dir| dir.into_os_string().into_string().unwrap())
            .collect();
        dirs.extend_from_slice(extra_dirs);

        let mut suffixes: Vec<String> = vec![];
        if cfg!(windows) {
            let path_ext = getenv("PATHEXT");
            suffixes = self.suffixes.clone();
            let exts = match path_ext {
                Some(ref ext) if !ext.is_empty() => std::env::split_paths(ext)
                    .map(|e| e.into_os_string().into_string().unwrap())
                    .collect(),
                _ => vec![
                    ".exe".to_string(),
                    ".bat".to_string(),
                    ".cmd".to_string(),
                    ".com".to_string(),
                ],
            };
            suffixes.extend(exts);
        }
        suffixes = if !pathinfo(name, PATHINFO_EXTENSION).is_empty() {
            let mut s = vec![String::new()];
            s.extend(suffixes);
            s
        } else {
            suffixes.push(String::new());
            suffixes
        };
        for suffix in &suffixes {
            for dir in &dirs {
                let dir = if dir.is_empty() { "." } else { dir.as_str() };
                let file = std::path::Path::new(dir)
                    .join(format!("{name}{suffix}"))
                    .into_os_string()
                    .into_string()
                    .unwrap();
                if is_file(&file) && (cfg!(windows) || is_executable(&file)) {
                    return Some(file);
                }

                if !is_dir(dir) && basename(dir) == format!("{name}{suffix}") && is_executable(dir)
                {
                    return Some(dir.to_string());
                }
            }
        }

        if cfg!(windows) || name.len() != strcspn(name, format!("/{}", std::path::MAIN_SEPARATOR)) {
            return default.map(ToString::to_string);
        }

        let exec_result =
            exec(bconcat!("command -v -- ", escapeshellarg(name)), None, None).unwrap_or_default();

        let executable_path = substr(
            &exec_result,
            0,
            strpos(&exec_result, PHP_EOL).map(|i| i as i64),
        );
        if !executable_path.is_empty() && is_executable(path_from_bytes(&executable_path)) {
            return Some(legacy_lossy(&executable_path).into_owned());
        }

        default.map(ToString::to_string)
    }
}
