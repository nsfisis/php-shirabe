use crate::PhpMixed;
use crate::PhpResource;
use crate::UnexpectedValueException;
use indexmap::IndexMap;

pub const PHP_EOL: &str = "\n";

pub const FILE_APPEND: i64 = 8;

pub const STDIN: PhpResource = PhpResource::Stdin;

pub const PATHINFO_FILENAME: i64 = 64;
pub const PATHINFO_EXTENSION: i64 = 4;
pub const PATHINFO_DIRNAME: i64 = 1;
pub const PATHINFO_BASENAME: i64 = 2;

pub const PATH_SEPARATOR: &str = ":";
pub const DIRECTORY_SEPARATOR: &str = "/";

pub const FILE_IGNORE_NEW_LINES: i64 = 2;

pub const SEEK_SET: i64 = 0;
pub const SEEK_CUR: i64 = 1;
pub const SEEK_END: i64 = 2;

pub const SKIP_DOTS: i64 = 4096;
pub const CHILD_FIRST: i64 = 16;
pub const SELF_FIRST: i64 = 0;

pub struct FilesystemIterator;

impl FilesystemIterator {
    pub const KEY_AS_PATHNAME: i64 = 256;
    pub const CURRENT_AS_FILEINFO: i64 = 0;
}

#[derive(Debug)]
pub struct DirectoryIteratorEntry;
impl DirectoryIteratorEntry {
    pub fn get_basename(&self) -> String {
        todo!()
    }
    pub fn is_file(&self) -> bool {
        todo!()
    }
    pub fn get_extension(&self) -> String {
        todo!()
    }
}

#[derive(Debug)]
pub struct RecursiveDirectoryIterator;

impl RecursiveDirectoryIterator {
    pub const SKIP_DOTS: i64 = 4096;
    pub const FOLLOW_SYMLINKS: i64 = 512;
}

#[derive(Debug)]
pub struct RecursiveIteratorIterator;

impl RecursiveIteratorIterator {
    pub const SELF_FIRST: i64 = 0;
    pub const CHILD_FIRST: i64 = 16;

    pub fn get_sub_pathname(&self) -> String {
        todo!()
    }
}

impl IntoIterator for &RecursiveIteratorIterator {
    type Item = RecursiveIteratorFileInfo;
    type IntoIter = std::vec::IntoIter<RecursiveIteratorFileInfo>;

    fn into_iter(self) -> Self::IntoIter {
        todo!()
    }
}

#[derive(Debug)]
pub struct RecursiveIteratorFileInfo;

impl RecursiveIteratorFileInfo {
    pub fn is_dir(&self) -> bool {
        todo!()
    }

    pub fn is_file(&self) -> bool {
        todo!()
    }

    pub fn is_link(&self) -> bool {
        todo!()
    }

    pub fn get_pathname(&self) -> String {
        todo!()
    }

    pub fn get_size(&self) -> i64 {
        todo!()
    }
}

pub fn recursive_directory_iterator(
    _path: impl AsRef<std::path::Path>,
    _flags: i64,
) -> Result<RecursiveDirectoryIterator, UnexpectedValueException> {
    todo!()
}

pub fn recursive_iterator_iterator(
    _iter: RecursiveDirectoryIterator,
    _mode: i64,
) -> RecursiveIteratorIterator {
    todo!()
}

pub fn directory_iterator(_path: &str) -> Vec<DirectoryIteratorEntry> {
    todo!()
}

pub fn fopen(_file: &str, _mode: &str) -> PhpMixed {
    todo!()
}

pub fn fwrite(_file: PhpMixed, _data: &str, _length: i64) -> Option<i64> {
    todo!()
}

pub fn fread(_handle: PhpMixed, _length: i64) -> Option<String> {
    todo!()
}

pub fn feof(_stream: PhpMixed) -> bool {
    todo!()
}

pub fn fclose(_file: PhpMixed) {
    todo!()
}

pub fn fgets(_handle: PhpMixed) -> Option<String> {
    todo!()
}

pub fn fgetc(_resource: &PhpResource) -> Option<String> {
    todo!()
}

pub fn ftell(_resource: &PhpResource) -> i64 {
    todo!()
}

pub fn fseek(_stream: PhpMixed, _offset: i64) -> i64 {
    todo!()
}

pub fn rewind(_stream: PhpMixed) -> bool {
    todo!()
}

pub fn fstat(_stream: PhpResource) -> PhpMixed {
    todo!()
}

pub fn lstat(_filename: &str) -> Option<IndexMap<String, PhpMixed>> {
    todo!()
}

/// PHP `ftell()` over a PhpMixed stream resource. (`ftell` itself is already defined for the
/// `PhpResource`-typed stream API used elsewhere.)
pub fn ftell_stream(_stream: &PhpMixed) -> i64 {
    todo!()
}

pub fn fseek3(_stream: PhpMixed, _offset: i64, _whence: i64) -> i64 {
    todo!()
}

pub fn touch(_path: &str) -> bool {
    todo!()
}

pub fn fflush_resource(resource: &PhpResource) {
    use std::io::Write;
    match resource {
        PhpResource::Stdin => {}
        PhpResource::Stdout => {
            let _ = std::io::stdout().flush();
        }
        PhpResource::Stderr => {
            let _ = std::io::stderr().flush();
        }
        PhpResource::File(file) => {
            let _ = file.borrow_mut().flush();
        }
    }
}

pub fn fwrite_resource(resource: &PhpResource, data: &str) {
    use std::io::Write;
    let bytes = data.as_bytes();
    match resource {
        PhpResource::Stdin => {}
        PhpResource::Stdout => {
            let _ = std::io::stdout().write_all(bytes);
        }
        PhpResource::Stderr => {
            let _ = std::io::stderr().write_all(bytes);
        }
        PhpResource::File(file) => {
            let _ = file.borrow_mut().write_all(bytes);
        }
    }
}

pub fn touch2(_path: &str, _mtime: i64) -> bool {
    todo!()
}

pub fn touch3(_path: &str, _mtime: i64, _atime: i64) -> bool {
    todo!()
}

pub fn chmod(_path: &str, _mode: u32) -> bool {
    todo!()
}

pub fn fileperms(_path: &str) -> i64 {
    todo!()
}

pub fn filesize(path: impl AsRef<std::path::Path>) -> Option<i64> {
    std::fs::metadata(path).ok().map(|m| m.len() as i64)
}

pub fn file_exists(path: impl AsRef<std::path::Path>) -> bool {
    path.as_ref().exists()
}

// TODO(phase-c): PHP's is_writable() resolves to access(2) with W_OK, honoring the effective
// user/group and ACLs. This std-only approximation only inspects the permission bits, so it can
// diverge for files the current user does not own. Refine with a syscall (libc/rustix) crate later.
pub fn is_writable(_path: &str) -> bool {
    match std::fs::metadata(_path) {
        Ok(meta) => !meta.permissions().readonly(),
        Err(_) => false,
    }
}

pub fn is_readable(_path: &str) -> bool {
    let path = std::path::Path::new(_path);
    match std::fs::metadata(path) {
        Ok(meta) => {
            if meta.is_dir() {
                std::fs::read_dir(path).is_ok()
            } else {
                std::fs::File::open(path).is_ok()
            }
        }
        Err(_) => false,
    }
}

pub fn is_executable(_path: &str) -> bool {
    todo!()
}

pub fn is_file(path: impl AsRef<std::path::Path>) -> bool {
    path.as_ref().is_file()
}

pub fn is_link(path: impl AsRef<std::path::Path>) -> bool {
    std::fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

pub fn is_dir(path: impl AsRef<std::path::Path>) -> bool {
    path.as_ref().is_dir()
}

pub fn fileatime(_filename: &str) -> Option<i64> {
    std::fs::metadata(_filename)
        .ok()
        .and_then(|m| m.accessed().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
}

pub fn filemtime(_filename: &str) -> Option<i64> {
    std::fs::metadata(_filename)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
}

pub fn fileowner(_filename: &str) -> Option<i64> {
    todo!()
}

pub fn unlink(path: impl AsRef<std::path::Path>) -> bool {
    std::fs::remove_file(path).is_ok()
}

pub fn unlink_silent(_path: &str) -> bool {
    todo!()
}

pub fn file_put_contents(_path: &str, _data: &[u8]) -> Option<i64> {
    std::fs::write(_path, _data)
        .ok()
        .map(|_| _data.len() as i64)
}

pub fn file_put_contents3(_filename: &str, _data: &str, _flags: i64) -> Option<i64> {
    todo!()
}

pub fn file_get_contents(_path: &str) -> Option<String> {
    std::fs::read(_path)
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

pub fn file_get_contents5(
    _path: &str,
    _use_include_path: bool,
    _context: PhpMixed,
    _offset: i64,
    _length: Option<i64>,
) -> Option<String> {
    todo!()
}

pub fn getcwd() -> Option<String> {
    std::env::current_dir()
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

pub fn chdir(_path: &str) -> anyhow::Result<()> {
    Ok(std::env::set_current_dir(_path)?)
}

pub fn glob(_pattern: &str) -> Vec<String> {
    todo!()
}

pub fn file(_filename: &str, _flags: i64) -> Option<Vec<String>> {
    todo!()
}

pub fn umask() -> u32 {
    todo!()
}

pub fn mkdir(_pathname: &str, _mode: u32, _recursive: bool) -> bool {
    todo!()
}

pub fn rmdir(dir: impl AsRef<std::path::Path>) -> bool {
    std::fs::remove_dir(dir).is_ok()
}

pub fn rename(
    old_name: impl AsRef<std::path::Path>,
    new_name: impl AsRef<std::path::Path>,
) -> bool {
    std::fs::rename(old_name, new_name).is_ok()
}

pub fn copy(_source: &str, _dest: &str) -> bool {
    std::fs::copy(_source, _dest).is_ok()
}

pub fn ftruncate(_stream: &PhpMixed, _size: i64) -> bool {
    todo!()
}

pub fn symlink(_target: &str, _link: &str) -> bool {
    todo!()
}

pub fn sys_get_temp_dir() -> String {
    std::env::temp_dir().to_string_lossy().into_owned()
}

pub fn tempnam(_dir: &str, _prefix: &str) -> Option<String> {
    todo!()
}

pub fn opendir(_path: &str) -> Option<PhpMixed> {
    todo!()
}

pub fn pathinfo(path: PhpMixed, option: i64) -> PhpMixed {
    let path = path.as_string().unwrap_or("");
    let component = match option {
        PATHINFO_DIRNAME => dirname(path),
        PATHINFO_BASENAME => basename(path),
        PATHINFO_EXTENSION => {
            let base = basename(path);
            match base.rfind('.') {
                Some(index) => base[index + 1..].to_string(),
                None => String::new(),
            }
        }
        PATHINFO_FILENAME => {
            let base = basename(path);
            match base.rfind('.') {
                Some(index) => base[..index].to_string(),
                None => base,
            }
        }
        _ => unreachable!("pathinfo called with an unsupported single-component option"),
    };
    PhpMixed::String(component)
}

// TODO(phase-c): takes &Path and returns Option<PathBuf>
pub fn realpath(path: &str) -> Option<String> {
    std::path::Path::new(path)
        .canonicalize()
        .ok()
        .and_then(|p| p.to_str().map(ToOwned::to_owned))
}

pub fn dirname(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    match std::path::Path::new(path).parent() {
        // No parent: the root itself, or a path made up solely of slashes.
        None => "/".to_string(),
        // Path::parent yields an empty path where PHP's dirname returns ".".
        Some(parent) if parent.as_os_str().is_empty() => ".".to_string(),
        Some(parent) => parent.to_str().expect("input was valid UTF-8").to_string(),
    }
}

pub fn dirname_levels(path: &str, levels: i64) -> String {
    let mut result = path.to_string();
    for _ in 0..levels {
        result = dirname(&result);
    }
    result
}

pub fn basename(path: &str) -> String {
    // PHP basename(): the trailing name component, after stripping trailing directory separators.
    let trimmed = path.trim_end_matches(['/', '\\']);
    match trimmed.rfind(['/', '\\']) {
        Some(index) => trimmed[index + 1..].to_string(),
        None => trimmed.to_string(),
    }
}

pub fn basename_with_suffix(path: &str, suffix: &str) -> String {
    let base = basename(path);
    // PHP strips the suffix only when it is a proper trailing part of the name,
    // never when it equals the whole basename.
    if base != suffix && base.ends_with(suffix) {
        base[..base.len() - suffix.len()].to_string()
    } else {
        base
    }
}

pub fn clearstatcache() {
    // Rust performs a fresh syscall for every metadata query; there is no stat
    // cache to invalidate.
}

pub fn clearstatcache2(_clear_realpath_cache: bool, _filename: &str) {
    // Rust performs a fresh syscall for every metadata query; there is no stat
    // cache to invalidate.
}

pub fn disk_free_space(_directory: &str) -> Option<f64> {
    todo!()
}

pub const GLOB_MARK: i64 = 8;
pub const GLOB_ONLYDIR: i64 = 1024;
pub const GLOB_BRACE: i64 = 4096;

pub fn glob_with_flags(_pattern: &str, _flags: i64) -> Vec<String> {
    todo!()
}
