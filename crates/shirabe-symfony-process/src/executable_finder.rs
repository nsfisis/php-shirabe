//! ref: composer/vendor/symfony/process/ExecutableFinder.php sha1:018f8e4843e52d6779af2720440bbdf232aac198

use bstr::{BStr, BString};
use shirabe_php_shim::{
    PATHINFO_EXTENSION, PHP_EOL, basename, bformat, escapeshellarg, exec, getenv, is_dir,
    is_executable, is_file, path_from_bytes, path_to_bytes, pathinfo, php_str, strcspn, strpos,
    strtolower, substr,
};
use std::path::PathBuf;

const CMD_BUILTINS: &[&BStr] = &[
    php_str(b"assoc"),
    php_str(b"break"),
    php_str(b"call"),
    php_str(b"cd"),
    php_str(b"chdir"),
    php_str(b"cls"),
    php_str(b"color"),
    php_str(b"copy"),
    php_str(b"date"),
    php_str(b"del"),
    php_str(b"dir"),
    php_str(b"echo"),
    php_str(b"endlocal"),
    php_str(b"erase"),
    php_str(b"exit"),
    php_str(b"for"),
    php_str(b"ftype"),
    php_str(b"goto"),
    php_str(b"help"),
    php_str(b"if"),
    php_str(b"label"),
    php_str(b"md"),
    php_str(b"mkdir"),
    php_str(b"mklink"),
    php_str(b"move"),
    php_str(b"path"),
    php_str(b"pause"),
    php_str(b"popd"),
    php_str(b"prompt"),
    php_str(b"pushd"),
    php_str(b"rd"),
    php_str(b"rem"),
    php_str(b"ren"),
    php_str(b"rename"),
    php_str(b"rmdir"),
    php_str(b"set"),
    php_str(b"setlocal"),
    php_str(b"shift"),
    php_str(b"start"),
    php_str(b"time"),
    php_str(b"title"),
    php_str(b"type"),
    php_str(b"ver"),
    php_str(b"vol"),
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
        default: Option<&BStr>,
        extra_dirs: &[BString],
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
        dirs.extend(
            extra_dirs
                .iter()
                .map(|dir| path_from_bytes(dir).to_path_buf()),
        );

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
                let file_name = bformat!("{name}{suffix}");
                let file = dir.join(path_from_bytes(&file_name));
                if is_file(&file) && (cfg!(windows) || is_executable(&file)) {
                    return Some(path_to_bytes(&file).into());
                }

                if !is_dir(dir) && basename(path_to_bytes(dir)) == file_name && is_executable(dir) {
                    return Some(path_to_bytes(dir).into());
                }
            }
        }

        if cfg!(windows)
            || name.len() != strcspn(name, bformat!("/{}", std::path::MAIN_SEPARATOR_STR))
        {
            return default.map(BString::from);
        }

        let exec_result = exec(
            bformat!("command -v -- {}", escapeshellarg(name)),
            None,
            None,
        )
        .unwrap_or_default();

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
