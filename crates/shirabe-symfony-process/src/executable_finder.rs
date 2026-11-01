//! ref: composer/vendor/symfony/process/ExecutableFinder.php

use bstr::BString;
use shirabe_php_shim::{
    PATHINFO_EXTENSION, PHP_EOL, basename, bconcat, escapeshellarg, exec, getenv, is_dir,
    is_executable, is_file, path_from_bytes, path_to_bytes, pathinfo, strcspn, strpos, strtolower,
    substr,
};
use std::path::PathBuf;

const CMD_BUILTINS: &[&str] = &[
    "assoc", "break", "call", "cd", "chdir", "cls", "color", "copy", "date", "del", "dir", "echo",
    "endlocal", "erase", "exit", "for", "ftype", "goto", "help", "if", "label", "md", "mkdir",
    "mklink", "move", "path", "pause", "popd", "prompt", "pushd", "rd", "rem", "ren", "rename",
    "rmdir", "set", "setlocal", "shift", "start", "time", "title", "type", "ver", "vol",
];

#[derive(Debug)]
pub struct ExecutableFinder {
    suffixes: Vec<BString>,
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

    pub fn find(
        &self,
        name: impl AsRef<[u8]>,
        default: Option<&str>,
        extra_dirs: &[String],
    ) -> Option<BString> {
        let name = name.as_ref();
        // windows built-in commands that are present in cmd.exe should not be resolved using PATH as they do not exist as exes
        if cfg!(windows) && CMD_BUILTINS.iter().any(|b| strtolower(name) == *b) {
            return Some(name.into());
        }

        let path = getenv("PATH")
            .or_else(|| getenv("Path"))
            .unwrap_or_default();
        let mut dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
        dirs.extend(extra_dirs.iter().map(PathBuf::from));

        let mut suffixes: Vec<BString> = vec![];
        if cfg!(windows) {
            let path_ext = getenv("PATHEXT");
            suffixes = self.suffixes.clone();
            let exts = match path_ext {
                Some(ref ext) if !ext.is_empty() => std::env::split_paths(ext)
                    .map(|e| BString::from(path_to_bytes(&e)))
                    .collect(),
                _ => vec![".exe".into(), ".bat".into(), ".cmd".into(), ".com".into()],
            };
            suffixes.extend(exts);
        }
        suffixes = if !pathinfo(name, PATHINFO_EXTENSION).is_empty() {
            let mut s = vec![BString::default()];
            s.extend(suffixes);
            s
        } else {
            suffixes.push(BString::default());
            suffixes
        };
        for suffix in &suffixes {
            for dir in &dirs {
                let dir = if dir.as_os_str().is_empty() {
                    std::path::Path::new(".")
                } else {
                    dir.as_path()
                };
                let file_name = bconcat!(name, suffix);
                let file = dir.join(path_from_bytes(&file_name));
                if is_file(&file) && (cfg!(windows) || is_executable(&file)) {
                    return Some(path_to_bytes(&file).into());
                }

                if !is_dir(dir) && basename(path_to_bytes(dir)) == file_name && is_executable(dir) {
                    return Some(path_to_bytes(dir).into());
                }
            }
        }

        if cfg!(windows) || name.len() != strcspn(name, format!("/{}", std::path::MAIN_SEPARATOR)) {
            return default.map(BString::from);
        }

        let exec_result =
            exec(bconcat!("command -v -- ", escapeshellarg(name)), None, None).unwrap_or_default();

        let executable_path = substr(
            &exec_result,
            0,
            strpos(&exec_result, PHP_EOL).map(|i| i as i64),
        );
        if !executable_path.is_empty() && is_executable(path_from_bytes(&executable_path)) {
            return Some(executable_path);
        }

        default.map(BString::from)
    }
}
