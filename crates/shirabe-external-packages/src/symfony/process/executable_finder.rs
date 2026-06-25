//! ref: composer/vendor/symfony/process/ExecutableFinder.php

use shirabe_php_shim::{self as php, PhpMixed};

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

    /// Replaces default suffixes of executable.
    pub fn set_suffixes(&mut self, suffixes: Vec<String>) {
        self.suffixes = suffixes;
    }

    /// Adds new possible suffix to check for executable.
    pub fn add_suffix(&mut self, suffix: &str) {
        self.suffixes.push(suffix.to_string());
    }

    pub fn find(&self, name: &str, default: Option<&str>, extra_dirs: &[String]) -> Option<String> {
        // windows built-in commands that are present in cmd.exe should not be resolved using PATH as they do not exist as exes
        if php::DIRECTORY_SEPARATOR == "\\"
            && CMD_BUILTINS.contains(&php::strtolower(name).as_str())
        {
            return Some(name.to_string());
        }

        let path = php::getenv("PATH")
            .or_else(|| php::getenv("Path"))
            .map(|v| v.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut dirs = php::explode(php::PATH_SEPARATOR, &path);
        dirs.extend_from_slice(extra_dirs);

        let mut suffixes: Vec<String> = vec![];
        if php::DIRECTORY_SEPARATOR == "\\" {
            let path_ext = php::getenv("PATHEXT").map(|v| v.to_string_lossy().into_owned());
            suffixes = self.suffixes.clone();
            let exts = match path_ext {
                Some(ref ext) if !ext.is_empty() => php::explode(php::PATH_SEPARATOR, ext),
                _ => vec![
                    ".exe".to_string(),
                    ".bat".to_string(),
                    ".cmd".to_string(),
                    ".com".to_string(),
                ],
            };
            suffixes.extend(exts);
        }
        suffixes = if !php::pathinfo(PhpMixed::String(name.to_string()), php::PATHINFO_EXTENSION)
            .as_string()
            .unwrap_or("")
            .is_empty()
        {
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
                let file = format!("{dir}{}{name}{suffix}", php::DIRECTORY_SEPARATOR);
                if php::is_file(&file)
                    && (php::DIRECTORY_SEPARATOR == "\\" || php::is_executable(&file))
                {
                    return Some(file);
                }

                if !php::is_dir(dir)
                    && php::basename(dir) == format!("{name}{suffix}")
                    && php::is_executable(dir)
                {
                    return Some(dir.to_string());
                }
            }
        }

        if php::DIRECTORY_SEPARATOR == "\\"
            || name.len() != php::strcspn(name, &format!("/{}", php::DIRECTORY_SEPARATOR))
        {
            return default.map(ToString::to_string);
        }

        let exec_result = php::exec(
            &format!("command -v -- {}", php::escapeshellarg(name)),
            None,
            None,
        )
        .unwrap_or_default();

        let executable_path = php::substr(
            &exec_result,
            0,
            php::strpos(&exec_result, php::PHP_EOL).map(|i| i as i64),
        );
        if !executable_path.is_empty() && php::is_executable(&executable_path) {
            return Some(executable_path);
        }

        default.map(ToString::to_string)
    }
}
