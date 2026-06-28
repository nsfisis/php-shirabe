//! ref: composer/vendor/symfony/process/ExecutableFinder.php

use shirabe_php_shim::PhpMixed;

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
        if shirabe_php_shim::DIRECTORY_SEPARATOR == "\\"
            && CMD_BUILTINS.contains(&shirabe_php_shim::strtolower(name).as_str())
        {
            return Some(name.to_string());
        }

        let path = shirabe_php_shim::getenv("PATH")
            .or_else(|| shirabe_php_shim::getenv("Path"))
            .map(|v| v.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut dirs = shirabe_php_shim::explode(shirabe_php_shim::PATH_SEPARATOR, &path);
        dirs.extend_from_slice(extra_dirs);

        let mut suffixes: Vec<String> = vec![];
        if shirabe_php_shim::DIRECTORY_SEPARATOR == "\\" {
            let path_ext =
                shirabe_php_shim::getenv("PATHEXT").map(|v| v.to_string_lossy().into_owned());
            suffixes = self.suffixes.clone();
            let exts = match path_ext {
                Some(ref ext) if !ext.is_empty() => {
                    shirabe_php_shim::explode(shirabe_php_shim::PATH_SEPARATOR, ext)
                }
                _ => vec![
                    ".exe".to_string(),
                    ".bat".to_string(),
                    ".cmd".to_string(),
                    ".com".to_string(),
                ],
            };
            suffixes.extend(exts);
        }
        suffixes = if !shirabe_php_shim::pathinfo(
            PhpMixed::String(name.to_string()),
            shirabe_php_shim::PATHINFO_EXTENSION,
        )
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
                let file = format!(
                    "{dir}{}{name}{suffix}",
                    shirabe_php_shim::DIRECTORY_SEPARATOR
                );
                if shirabe_php_shim::is_file(&file)
                    && (shirabe_php_shim::DIRECTORY_SEPARATOR == "\\"
                        || shirabe_php_shim::is_executable(&file))
                {
                    return Some(file);
                }

                if !shirabe_php_shim::is_dir(dir)
                    && shirabe_php_shim::basename(dir) == format!("{name}{suffix}")
                    && shirabe_php_shim::is_executable(dir)
                {
                    return Some(dir.to_string());
                }
            }
        }

        if shirabe_php_shim::DIRECTORY_SEPARATOR == "\\"
            || name.len()
                != shirabe_php_shim::strcspn(
                    name,
                    &format!("/{}", shirabe_php_shim::DIRECTORY_SEPARATOR),
                )
        {
            return default.map(ToString::to_string);
        }

        let exec_result = shirabe_php_shim::exec(
            &format!("command -v -- {}", shirabe_php_shim::escapeshellarg(name)),
            None,
            None,
        )
        .unwrap_or_default();

        let executable_path = shirabe_php_shim::substr(
            &exec_result,
            0,
            shirabe_php_shim::strpos(&exec_result, shirabe_php_shim::PHP_EOL).map(|i| i as i64),
        );
        if !executable_path.is_empty() && shirabe_php_shim::is_executable(&executable_path) {
            return Some(executable_path);
        }

        default.map(ToString::to_string)
    }
}
