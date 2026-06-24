use crate::PhpMixed;
use crate::PhpResource;
use crate::StreamBacking;
use crate::StreamState;
use crate::UnexpectedValueException;
use indexmap::IndexMap;

pub const PHP_EOL: &str = "\n";

pub const FILE_APPEND: i64 = 8;

pub const STDIN: PhpResource = PhpResource::Stdin;
pub const STDOUT: PhpResource = PhpResource::Stdout;
pub const STDERR: PhpResource = PhpResource::Stderr;

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
    // TODO(phase-d): DirectoryIterator is a unit struct carrying no entry data; giving it real
    // behavior requires the same field redesign as RecursiveIteratorFileInfo. It has no callers, so
    // it is left unimplemented.
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
pub struct RecursiveDirectoryIterator {
    root: std::path::PathBuf,
    flags: i64,
}

impl RecursiveDirectoryIterator {
    pub const SKIP_DOTS: i64 = 4096;
    pub const FOLLOW_SYMLINKS: i64 = 512;
}

#[derive(Debug)]
pub struct RecursiveIteratorIterator {
    entries: Vec<RecursiveIteratorFileInfo>,
    // Index of the entry the iteration is currently on, so get_sub_pathname() can report it.
    cursor: std::cell::Cell<usize>,
}

impl RecursiveIteratorIterator {
    pub const SELF_FIRST: i64 = 0;
    pub const CHILD_FIRST: i64 = 16;

    pub fn get_sub_pathname(&self) -> String {
        self.entries[self.cursor.get()].sub_pathname()
    }
}

pub struct RecursiveIteratorIter<'a> {
    inner: &'a RecursiveIteratorIterator,
    index: usize,
}

impl Iterator for RecursiveIteratorIter<'_> {
    type Item = RecursiveIteratorFileInfo;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.inner.entries.len() {
            // Publish the current position so get_sub_pathname() called inside the loop sees it.
            self.inner.cursor.set(self.index);
            let item = self.inner.entries[self.index].clone();
            self.index += 1;
            Some(item)
        } else {
            None
        }
    }
}

impl<'a> IntoIterator for &'a RecursiveIteratorIterator {
    type Item = RecursiveIteratorFileInfo;
    type IntoIter = RecursiveIteratorIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        RecursiveIteratorIter {
            inner: self,
            index: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecursiveIteratorFileInfo {
    path: std::path::PathBuf,
    root: std::path::PathBuf,
}

impl RecursiveIteratorFileInfo {
    pub fn is_dir(&self) -> bool {
        // SplFileInfo::isDir() follows symlinks.
        std::fs::metadata(&self.path)
            .map(|m| m.is_dir())
            .unwrap_or(false)
    }

    pub fn is_file(&self) -> bool {
        std::fs::metadata(&self.path)
            .map(|m| m.is_file())
            .unwrap_or(false)
    }

    pub fn is_link(&self) -> bool {
        std::fs::symlink_metadata(&self.path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
    }

    pub fn get_pathname(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }

    pub fn get_size(&self) -> i64 {
        std::fs::metadata(&self.path)
            .map(|m| m.len() as i64)
            .unwrap_or(0)
    }

    fn sub_pathname(&self) -> String {
        self.path
            .strip_prefix(&self.root)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| self.get_pathname())
    }
}

pub fn recursive_directory_iterator(
    _path: impl AsRef<std::path::Path>,
    _flags: i64,
) -> Result<RecursiveDirectoryIterator, UnexpectedValueException> {
    let root = _path.as_ref().to_path_buf();
    if !root.is_dir() {
        return Err(UnexpectedValueException {
            message: format!(
                "RecursiveDirectoryIterator::__construct({}): Failed to open directory",
                root.to_string_lossy()
            ),
            code: 0,
        });
    }
    Ok(RecursiveDirectoryIterator {
        root,
        flags: _flags,
    })
}

pub fn recursive_iterator_iterator(
    _iter: RecursiveDirectoryIterator,
    _mode: i64,
) -> RecursiveIteratorIterator {
    let mut entries = Vec::new();
    rii_walk(&_iter.root, &_iter.root, _iter.flags, _mode, &mut entries);
    RecursiveIteratorIterator {
        entries,
        cursor: std::cell::Cell::new(0),
    }
}

// Recursively collects directory entries in filesystem order, matching SplFileInfo recursion:
// real subdirectories are descended into (also symlinked dirs when FOLLOW_SYMLINKS is set), with the
// directory itself yielded before its children for SELF_FIRST and after them for CHILD_FIRST.
fn rii_walk(
    dir: &std::path::Path,
    root: &std::path::Path,
    flags: i64,
    mode: i64,
    out: &mut Vec<RecursiveIteratorFileInfo>,
) {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    for entry in rd.flatten() {
        let path = entry.path();
        let is_real_dir = std::fs::symlink_metadata(&path)
            .map(|m| m.is_dir())
            .unwrap_or(false);
        let follows_symlink_dir = (flags & RecursiveDirectoryIterator::FOLLOW_SYMLINKS != 0)
            && std::fs::metadata(&path)
                .map(|m| m.is_dir())
                .unwrap_or(false);
        let info = RecursiveIteratorFileInfo {
            path: path.clone(),
            root: root.to_path_buf(),
        };
        if is_real_dir || follows_symlink_dir {
            if mode == RecursiveIteratorIterator::CHILD_FIRST {
                rii_walk(&path, root, flags, mode, out);
                out.push(info);
            } else {
                out.push(info);
                rii_walk(&path, root, flags, mode, out);
            }
        } else {
            out.push(info);
        }
    }
}

pub fn directory_iterator(_path: &str) -> Vec<DirectoryIteratorEntry> {
    // TODO(phase-d): see DirectoryIteratorEntry; the entry type carries no data yet and there are no
    // callers.
    todo!()
}

/// PHP `fopen()`. Returns a stream resource, or an error mirroring PHP's `false`-on-failure
/// (the I/O error carries the reason so callers that surface the warning text can use it).
pub fn fopen(file: &str, mode: &str) -> Result<PhpResource, std::io::Error> {
    match file {
        "php://output" | "php://stdout" => return Ok(PhpResource::Stdout),
        "php://stderr" => return Ok(PhpResource::Stderr),
        "php://stdin" | "php://input" => return Ok(PhpResource::Stdin),
        _ => {}
    }
    // Strip the binary/text flags PHP accepts as part of the mode.
    let base_mode: String = mode.chars().filter(|c| *c != 'b' && *c != 't').collect();
    let (readable, writable) = match base_mode.as_str() {
        "r" => (true, false),
        "w" | "a" | "x" | "c" => (false, true),
        _ => (true, true), // r+, w+, a+, x+, c+, rw, ...
    };
    // php://memory and php://temp are in-memory streams.
    if file == "php://memory" || file.starts_with("php://temp") {
        return Ok(StreamState::new(
            StreamBacking::Memory(std::io::Cursor::new(Vec::new())),
            readable,
            writable,
            mode.to_string(),
            file.to_string(),
        ));
    }
    let uri = file.to_string();
    let mut options = std::fs::OpenOptions::new();
    match base_mode.as_str() {
        "r" => options.read(true),
        "r+" => options.read(true).write(true),
        "w" => options.write(true).create(true).truncate(true),
        "w+" => options.read(true).write(true).create(true).truncate(true),
        "a" => options.append(true).create(true),
        "a+" => options.read(true).append(true).create(true),
        "x" => options.write(true).create_new(true),
        "x+" => options.read(true).write(true).create_new(true),
        // "c"/"c+": open or create without truncating, position at start.
        "c" => options.write(true).create(true),
        "c+" => options.read(true).write(true).create(true),
        _ => options.read(true),
    };
    let file = options.open(file)?;
    Ok(StreamState::new(
        StreamBacking::File(file),
        readable,
        writable,
        mode.to_string(),
        uri,
    ))
}

/// PHP `fwrite()`. `length` caps the number of bytes written (`None` = whole string).
/// Returns the byte count written, or `None` for PHP's `false`-on-failure.
pub fn fwrite(stream: &PhpResource, data: &str, length: Option<i64>) -> Option<i64> {
    use std::io::Write;
    let bytes = data.as_bytes();
    let bytes = match length {
        Some(l) if l >= 0 => &bytes[..(l as usize).min(bytes.len())],
        _ => bytes,
    };
    match stream {
        PhpResource::Stdin | PhpResource::Process(_) => None,
        PhpResource::Stdout => std::io::stdout()
            .write_all(bytes)
            .ok()
            .map(|_| bytes.len() as i64),
        PhpResource::Stderr => std::io::stderr()
            .write_all(bytes)
            .ok()
            .map(|_| bytes.len() as i64),
        PhpResource::Stream(state) => {
            let mut state = state.borrow_mut();
            if state.closed || !state.writable {
                return None;
            }
            let n = state.backing.as_rws().write(bytes).ok()?;
            Some(n as i64)
        }
    }
}

/// PHP `fread()`. Reads up to `length` bytes.
/// TODO(phase-e): byte-string semantics — should return Vec<u8>; from_utf8_lossy can corrupt
/// binary reads (filesAreEqual / binary copy).
pub fn fread(stream: &PhpResource, length: i64) -> Option<String> {
    use std::io::Read;
    let cap = length.max(0) as usize;
    match stream {
        PhpResource::Stdin => {
            let mut buf = vec![0u8; cap];
            let n = std::io::stdin().read(&mut buf).ok()?;
            buf.truncate(n);
            Some(String::from_utf8_lossy(&buf).into_owned())
        }
        PhpResource::Stdout | PhpResource::Stderr | PhpResource::Process(_) => None,
        PhpResource::Stream(state) => {
            let mut state = state.borrow_mut();
            if state.closed || !state.readable {
                return None;
            }
            let mut buf = vec![0u8; cap];
            let n = state.backing.as_rws().read(&mut buf).ok()?;
            if cap > 0 && n == 0 {
                state.eof = true;
            }
            buf.truncate(n);
            Some(String::from_utf8_lossy(&buf).into_owned())
        }
    }
}

/// PHP `feof()`: true only after a read has hit end-of-stream.
pub fn feof(stream: &PhpResource) -> bool {
    match stream {
        PhpResource::Stdin
        | PhpResource::Stdout
        | PhpResource::Stderr
        | PhpResource::Process(_) => false,
        PhpResource::Stream(state) => state.borrow().eof,
    }
}

/// PHP `fclose()`. Marks the stream closed; the backing is released when the last clone drops.
pub fn fclose(stream: &PhpResource) -> bool {
    match stream {
        PhpResource::Stdin | PhpResource::Stdout | PhpResource::Stderr => true,
        PhpResource::Process(_) => false,
        PhpResource::Stream(state) => {
            let mut state = state.borrow_mut();
            if state.closed {
                return false;
            }
            use std::io::Write;
            let _ = state.backing.as_rws().flush();
            state.closed = true;
            true
        }
    }
}

/// PHP `fgets()`. Reads one line, including the trailing newline, capped at `length-1` bytes
/// when given (matching PHP's `length` parameter).
/// TODO(phase-e): byte-string semantics — should return Vec<u8>; from_utf8_lossy can corrupt
/// binary reads.
pub fn fgets(stream: &PhpResource, length: Option<i64>) -> Option<String> {
    let limit = match length {
        Some(l) if l > 0 => Some((l - 1) as usize),
        _ => None,
    };
    match stream {
        PhpResource::Stdin => {
            let stdin = std::io::stdin();
            let line = fgets_read_line(&mut stdin.lock(), limit).ok()?;
            if line.is_empty() {
                return None;
            }
            Some(String::from_utf8_lossy(&line).into_owned())
        }
        PhpResource::Stdout | PhpResource::Stderr | PhpResource::Process(_) => None,
        PhpResource::Stream(state) => {
            let mut state = state.borrow_mut();
            if state.closed || !state.readable {
                return None;
            }
            let line = fgets_read_line(state.backing.as_rws(), limit).ok()?;
            if line.is_empty() {
                state.eof = true;
                return None;
            }
            Some(String::from_utf8_lossy(&line).into_owned())
        }
    }
}

// Reads one byte at a time up to and including the next newline, stopping early at `limit` bytes.
fn fgets_read_line<R: std::io::Read + ?Sized>(
    r: &mut R,
    limit: Option<usize>,
) -> std::io::Result<Vec<u8>> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if let Some(max) = limit {
            if line.len() >= max {
                break;
            }
        }
        let n = r.read(&mut byte)?;
        if n == 0 {
            break;
        }
        line.push(byte[0]);
        if byte[0] == b'\n' {
            break;
        }
    }
    Ok(line)
}

/// PHP `fgetc()`: reads a single byte, or `None` at end-of-stream.
/// TODO(phase-e): byte-string semantics — should return Vec<u8>.
pub fn fgetc(stream: &PhpResource) -> Option<String> {
    use std::io::Read;
    let mut byte = [0u8; 1];
    match stream {
        PhpResource::Stdin => {
            let n = std::io::stdin().read(&mut byte).ok()?;
            if n == 0 {
                return None;
            }
            Some(String::from_utf8_lossy(&byte).into_owned())
        }
        PhpResource::Stdout | PhpResource::Stderr | PhpResource::Process(_) => None,
        PhpResource::Stream(state) => {
            let mut state = state.borrow_mut();
            if state.closed || !state.readable {
                return None;
            }
            let n = state.backing.as_rws().read(&mut byte).ok()?;
            if n == 0 {
                state.eof = true;
                return None;
            }
            Some(String::from_utf8_lossy(&byte).into_owned())
        }
    }
}

/// PHP `ftell()`: the current position, or `None` for `false`-on-failure.
pub fn ftell(stream: &PhpResource) -> Option<i64> {
    use std::io::Seek;
    match stream {
        PhpResource::Stdin
        | PhpResource::Stdout
        | PhpResource::Stderr
        | PhpResource::Process(_) => None,
        PhpResource::Stream(state) => {
            let mut state = state.borrow_mut();
            if state.closed {
                return None;
            }
            state
                .backing
                .as_rws()
                .stream_position()
                .ok()
                .map(|p| p as i64)
        }
    }
}

/// PHP `fseek()`. Returns 0 on success, -1 on failure.
pub fn fseek(stream: &PhpResource, offset: i64, whence: i64) -> i64 {
    use std::io::Seek;
    let from = match whence {
        SEEK_CUR => std::io::SeekFrom::Current(offset),
        SEEK_END => std::io::SeekFrom::End(offset),
        _ => std::io::SeekFrom::Start(offset.max(0) as u64),
    };
    match stream {
        PhpResource::Stdin
        | PhpResource::Stdout
        | PhpResource::Stderr
        | PhpResource::Process(_) => -1,
        PhpResource::Stream(state) => {
            let mut state = state.borrow_mut();
            if state.closed {
                return -1;
            }
            match state.backing.as_rws().seek(from) {
                Ok(_) => {
                    state.eof = false;
                    0
                }
                Err(_) => -1,
            }
        }
    }
}

/// PHP `rewind()`: seek to the start.
pub fn rewind(stream: &PhpResource) -> bool {
    fseek(stream, 0, SEEK_SET) == 0
}

/// PHP `fstat()`: the stat array of an open stream, or `None` for `false`-on-failure.
pub fn fstat(stream: &PhpResource) -> Option<IndexMap<String, PhpMixed>> {
    match stream {
        // TODO(phase-d): the stdio streams expose no fd to stat without a syscall crate; report
        // failure rather than fabricate fields.
        PhpResource::Stdin
        | PhpResource::Stdout
        | PhpResource::Stderr
        | PhpResource::Process(_) => None,
        PhpResource::Stream(state) => {
            let mut state = state.borrow_mut();
            if state.closed {
                return None;
            }
            let (size, file_meta) = match &mut state.backing {
                StreamBacking::File(f) => {
                    let m = f.metadata().ok()?;
                    (m.len(), Some(m))
                }
                StreamBacking::Memory(c) => (c.get_ref().len() as u64, None),
                StreamBacking::Pipe(_) => return None,
            };
            Some(build_stat_map(size, file_meta.as_ref()))
        }
    }
}

// Builds the 13-field PHP stat array (indexed 0..12 and by name). For in-memory streams only
// `size` is meaningful; the rest are reported as 0, matching PHP fstat on php://temp.
fn build_stat_map(size: u64, file_meta: Option<&std::fs::Metadata>) -> IndexMap<String, PhpMixed> {
    use std::os::unix::fs::MetadataExt;
    let fields: [(&str, i64); 13] = match file_meta {
        Some(m) => [
            ("dev", m.dev() as i64),
            ("ino", m.ino() as i64),
            ("mode", m.mode() as i64),
            ("nlink", m.nlink() as i64),
            ("uid", m.uid() as i64),
            ("gid", m.gid() as i64),
            ("rdev", m.rdev() as i64),
            ("size", m.size() as i64),
            ("atime", m.atime()),
            ("mtime", m.mtime()),
            ("ctime", m.ctime()),
            ("blksize", m.blksize() as i64),
            ("blocks", m.blocks() as i64),
        ],
        None => [
            ("dev", 0),
            ("ino", 0),
            ("mode", 0),
            ("nlink", 0),
            ("uid", 0),
            ("gid", 0),
            ("rdev", 0),
            ("size", size as i64),
            ("atime", 0),
            ("mtime", 0),
            ("ctime", 0),
            ("blksize", 0),
            ("blocks", 0),
        ],
    };
    let mut map = IndexMap::new();
    for (i, (_, v)) in fields.iter().enumerate() {
        map.insert(i.to_string(), PhpMixed::Int(*v));
    }
    for (name, v) in &fields {
        map.insert(name.to_string(), PhpMixed::Int(*v));
    }
    map
}

/// PHP `fflush()`.
pub fn fflush(stream: &PhpResource) -> bool {
    use std::io::Write;
    match stream {
        PhpResource::Stdin => true,
        PhpResource::Stdout => std::io::stdout().flush().is_ok(),
        PhpResource::Stderr => std::io::stderr().flush().is_ok(),
        PhpResource::Process(_) => false,
        PhpResource::Stream(state) => {
            let mut state = state.borrow_mut();
            if state.closed {
                return false;
            }
            state.backing.as_rws().flush().is_ok()
        }
    }
}

pub fn lstat(_filename: &str) -> Option<IndexMap<String, PhpMixed>> {
    use std::os::unix::fs::MetadataExt;
    let m = std::fs::symlink_metadata(_filename).ok()?;
    // PHP stat/lstat return the 13 fields both by numeric index (0..12) and by name.
    let fields: [(&str, i64); 13] = [
        ("dev", m.dev() as i64),
        ("ino", m.ino() as i64),
        ("mode", m.mode() as i64),
        ("nlink", m.nlink() as i64),
        ("uid", m.uid() as i64),
        ("gid", m.gid() as i64),
        ("rdev", m.rdev() as i64),
        ("size", m.size() as i64),
        ("atime", m.atime()),
        ("mtime", m.mtime()),
        ("ctime", m.ctime()),
        ("blksize", m.blksize() as i64),
        ("blocks", m.blocks() as i64),
    ];
    let mut map = IndexMap::new();
    for (i, (_, v)) in fields.iter().enumerate() {
        map.insert(i.to_string(), PhpMixed::Int(*v));
    }
    for (name, v) in &fields {
        map.insert(name.to_string(), PhpMixed::Int(*v));
    }
    Some(map)
}

pub fn touch(_path: &str) -> bool {
    // TODO(phase-d): for an existing file PHP also bumps its mtime/atime to now; std exposes no
    // portable utime, so only the create-if-absent case is handled here.
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(_path)
        .is_ok()
}

pub fn fflush_resource(resource: &PhpResource) {
    fflush(resource);
}

pub fn fwrite_resource(resource: &PhpResource, data: &str) {
    fwrite(resource, data, None);
}

pub fn touch2(_path: &str, _mtime: i64) -> bool {
    // TODO(phase-d): setting an explicit mtime needs utimensat(2), not exposed by std (no
    // libc/filetime crate available).
    todo!()
}

pub fn touch3(_path: &str, _mtime: i64, _atime: i64) -> bool {
    // TODO(phase-d): setting explicit mtime/atime needs utimensat(2); see touch2.
    todo!()
}

pub fn chmod(_path: &str, _mode: u32) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(_path, std::fs::Permissions::from_mode(_mode)).is_ok()
}

pub fn fileperms(_path: &str) -> i64 {
    use std::os::unix::fs::MetadataExt;
    // PHP returns the full st_mode (file type bits included).
    // TODO(phase-d): PHP returns false on error; this i64 signature reports 0 instead.
    std::fs::metadata(_path)
        .map(|m| m.mode() as i64)
        .unwrap_or(0)
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
    use std::os::unix::fs::PermissionsExt;
    // TODO(phase-d): like is_writable, this only inspects the permission bits and ignores the
    // effective user/group, so it can diverge from PHP's access(2, X_OK) check.
    match std::fs::metadata(_path) {
        Ok(m) => (m.permissions().mode() & 0o111) != 0,
        Err(_) => false,
    }
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
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(_filename).ok().map(|m| m.uid() as i64)
}

pub fn unlink(path: impl AsRef<std::path::Path>) -> bool {
    std::fs::remove_file(path).is_ok()
}

pub fn unlink_silent(_path: &str) -> bool {
    // PHP's `@unlink`: delete the file, suppressing any warning.
    std::fs::remove_file(_path).is_ok()
}

pub fn file_put_contents(_path: &str, _data: &[u8]) -> Option<i64> {
    std::fs::write(_path, _data)
        .ok()
        .map(|_| _data.len() as i64)
}

pub fn file_put_contents3(_filename: &str, _data: &str, _flags: i64) -> Option<i64> {
    use std::io::Write;
    // TODO(phase-d): the LOCK_EX and FILE_USE_INCLUDE_PATH flags are ignored; only FILE_APPEND is
    // honored.
    let append = _flags & FILE_APPEND != 0;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true);
    if append {
        opts.append(true);
    } else {
        opts.truncate(true);
    }
    let mut file = opts.open(_filename).ok()?;
    file.write_all(_data.as_bytes()).ok()?;
    Some(_data.len() as i64)
}

pub fn file_get_contents(path: impl AsRef<std::path::Path>) -> Option<String> {
    let path = path.as_ref();
    // PHP supports the file:// stream wrapper; strip it to read the local file.
    let path = path
        .to_str()
        .and_then(|s| s.strip_prefix("file://"))
        .map_or(path, std::path::Path::new);
    std::fs::read(path)
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
    // TODO(phase-d): the stream $context and FILE_USE_INCLUDE_PATH are ignored; only $offset and
    // $length are applied (to the file read from the local filesystem).
    let bytes = std::fs::read(_path).ok()?;
    let len = bytes.len() as i64;
    let start = if _offset < 0 {
        (len + _offset).max(0)
    } else {
        _offset.min(len)
    } as usize;
    let slice = &bytes[start..];
    let slice = match _length {
        Some(l) if l >= 0 => &slice[..(l as usize).min(slice.len())],
        _ => slice,
    };
    Some(String::from_utf8_lossy(slice).into_owned())
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
    glob_with_flags(_pattern, 0)
}

pub const FILE_SKIP_EMPTY_LINES: i64 = 4;

pub fn file(_filename: &str, _flags: i64) -> Option<Vec<String>> {
    let content = std::fs::read(_filename).ok()?;
    let s = String::from_utf8_lossy(&content);
    let ignore_newlines = _flags & FILE_IGNORE_NEW_LINES != 0;
    let skip_empty = _flags & FILE_SKIP_EMPTY_LINES != 0;
    let mut lines = Vec::new();
    // PHP keeps the trailing newline on each element unless FILE_IGNORE_NEW_LINES is set.
    for line in s.split_inclusive('\n') {
        let mut l = line.to_string();
        if ignore_newlines {
            if l.ends_with('\n') {
                l.pop();
            }
            if l.ends_with('\r') {
                l.pop();
            }
        }
        if skip_empty && l.is_empty() {
            continue;
        }
        lines.push(l);
    }
    Some(lines)
}

pub fn umask() -> u32 {
    // Linux exposes the current umask via /proc/self/status.
    // TODO(phase-d): other platforms have no /proc; reading the umask there needs the
    // read-modify-write umask(2), which std does not expose (no libc/syscall crate available).
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                line.strip_prefix("Umask:")
                    .and_then(|v| u32::from_str_radix(v.trim(), 8).ok())
            })
        })
        .unwrap_or(0o022)
}

pub fn mkdir(_pathname: &str, _mode: u32, _recursive: bool) -> bool {
    use std::os::unix::fs::DirBuilderExt;
    // DirBuilder::mode passes the mode to mkdir(2), which applies the process umask, matching PHP.
    let mut builder = std::fs::DirBuilder::new();
    builder.mode(_mode).recursive(_recursive);
    builder.create(_pathname).is_ok()
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

pub fn ftruncate(stream: &PhpResource, size: i64) -> bool {
    match stream {
        PhpResource::Stdin
        | PhpResource::Stdout
        | PhpResource::Stderr
        | PhpResource::Process(_) => false,
        PhpResource::Stream(state) => {
            let mut state = state.borrow_mut();
            if state.closed || !state.writable {
                return false;
            }
            let size = size.max(0) as u64;
            match &mut state.backing {
                StreamBacking::File(f) => f.set_len(size).is_ok(),
                StreamBacking::Memory(c) => {
                    // Grow or shrink the buffer; PHP ftruncate leaves the position unchanged.
                    let buf = c.get_mut();
                    buf.resize(size as usize, 0);
                    true
                }
                StreamBacking::Pipe(_) => false,
            }
        }
    }
}

pub fn symlink(_target: &str, _link: &str) -> bool {
    std::os::unix::fs::symlink(_target, _link).is_ok()
}

pub fn sys_get_temp_dir() -> String {
    std::env::temp_dir().to_string_lossy().into_owned()
}

pub fn tempnam(_dir: &str, _prefix: &str) -> Option<String> {
    use std::os::unix::fs::PermissionsExt;
    // TODO(phase-d): PHP falls back to the system temp dir when $dir is not writable; that fallback
    // is not implemented here.
    for _ in 0..1000 {
        let name = format!("{}{:08x}", _prefix, fastrand::u32(..));
        let path = std::path::Path::new(_dir).join(name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => {
                // PHP creates the file with 0600 permissions.
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
                return path.to_str().map(ToOwned::to_owned);
            }
            Err(_) => continue,
        }
    }
    None
}

// A directory-handle resource. This is a distinct resource kind from the byte streams modeled by
// PhpResource; readdir/closedir have no callers yet, so it only records the opened path.
// TODO(phase-d): give it real readdir/closedir behavior (cursor over the entries) when needed.
#[derive(Debug)]
pub struct PhpDirHandle {
    pub path: std::path::PathBuf,
}

pub fn opendir(path: &str) -> Option<PhpDirHandle> {
    // opendir succeeds iff the path is a readable directory.
    std::fs::read_dir(path).ok()?;
    Some(PhpDirHandle {
        path: std::path::PathBuf::from(path),
    })
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
    // TODO(phase-d): reading free space for an arbitrary path requires statvfs(3); std exposes no
    // equivalent and no /proc file gives per-path free space (no libc/syscall crate available).
    todo!()
}

pub const GLOB_MARK: i64 = 8;
pub const GLOB_ONLYDIR: i64 = 1024;
pub const GLOB_BRACE: i64 = 4096;

pub fn glob_with_flags(_pattern: &str, _flags: i64) -> Vec<String> {
    let patterns = if _flags & GLOB_BRACE != 0 {
        glob_expand_braces(_pattern)
    } else {
        vec![_pattern.to_string()]
    };
    let mut results: Vec<String> = Vec::new();
    for pattern in patterns {
        glob_collect(&pattern, _flags, &mut results);
    }
    // PHP sorts the result set by default (GLOB_NOSORT is not modeled here).
    results.sort();
    results.dedup();
    results
}

fn glob_collect(pattern: &str, flags: i64, out: &mut Vec<String>) {
    let (mut current, rest) = match pattern.strip_prefix('/') {
        Some(rest) => (vec!["/".to_string()], rest),
        None => (vec![String::new()], pattern),
    };
    let segments: Vec<&str> = rest.split('/').collect();
    for (idx, seg) in segments.iter().enumerate() {
        let is_last = idx == segments.len() - 1;
        let mut next: Vec<String> = Vec::new();
        for base in &current {
            if seg.is_empty() {
                next.push(base.clone());
                continue;
            }
            if glob_has_wildcard(seg) {
                let read_base = if base.is_empty() { "." } else { base.as_str() };
                if let Ok(rd) = std::fs::read_dir(read_base) {
                    for entry in rd.flatten() {
                        let name = entry.file_name().to_string_lossy().into_owned();
                        if glob_fnmatch(seg, &name) {
                            let path = glob_join(base, &name);
                            if is_last || std::path::Path::new(&path).is_dir() {
                                next.push(path);
                            }
                        }
                    }
                }
            } else {
                let path = glob_join(base, seg);
                let p = std::path::Path::new(&path);
                if (is_last && p.exists()) || (!is_last && p.is_dir()) {
                    next.push(path);
                }
            }
        }
        current = next;
    }
    for mut path in current {
        let is_dir = std::path::Path::new(&path).is_dir();
        if flags & GLOB_ONLYDIR != 0 && !is_dir {
            continue;
        }
        if flags & GLOB_MARK != 0 && is_dir && !path.ends_with('/') {
            path.push('/');
        }
        out.push(path);
    }
}

fn glob_join(base: &str, seg: &str) -> String {
    if base.is_empty() {
        seg.to_string()
    } else if base == "/" {
        format!("/{}", seg)
    } else {
        format!("{}/{}", base, seg)
    }
}

fn glob_has_wildcard(seg: &str) -> bool {
    seg.bytes().any(|b| matches!(b, b'*' | b'?' | b'['))
}

fn glob_fnmatch(pattern: &str, name: &str) -> bool {
    // A leading '.' is only matched by an explicit leading '.' in the pattern.
    if name.starts_with('.') && !pattern.starts_with('.') {
        return false;
    }
    glob_fnmatch_bytes(pattern.as_bytes(), name.as_bytes())
}

fn glob_fnmatch_bytes(p: &[u8], s: &[u8]) -> bool {
    let mut pi = 0;
    let mut si = 0;
    let mut star: Option<usize> = None;
    let mut star_s = 0;
    while si < s.len() {
        if pi < p.len() {
            match p[pi] {
                b'*' => {
                    star = Some(pi);
                    star_s = si;
                    pi += 1;
                    continue;
                }
                b'?' => {
                    pi += 1;
                    si += 1;
                    continue;
                }
                b'[' => {
                    if let Some((matched, next_pi)) = glob_match_bracket(p, pi, s[si]) {
                        if matched {
                            pi = next_pi;
                            si += 1;
                            continue;
                        }
                    } else if p[pi] == s[si] {
                        // Unterminated '[' is treated as a literal.
                        pi += 1;
                        si += 1;
                        continue;
                    }
                }
                c => {
                    if c == s[si] {
                        pi += 1;
                        si += 1;
                        continue;
                    }
                }
            }
        }
        if let Some(sp) = star {
            pi = sp + 1;
            star_s += 1;
            si = star_s;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

// Returns (matched, index-after-']') for a `[...]` class, or None when the bracket is unterminated.
fn glob_match_bracket(p: &[u8], start: usize, c: u8) -> Option<(bool, usize)> {
    let mut i = start + 1;
    if i >= p.len() {
        return None;
    }
    let negate = p[i] == b'!' || p[i] == b'^';
    if negate {
        i += 1;
    }
    let mut matched = false;
    let mut first = true;
    while i < p.len() {
        if p[i] == b']' && !first {
            return Some((matched ^ negate, i + 1));
        }
        first = false;
        if i + 2 < p.len() && p[i + 1] == b'-' && p[i + 2] != b']' {
            if p[i] <= c && c <= p[i + 2] {
                matched = true;
            }
            i += 3;
        } else {
            if p[i] == c {
                matched = true;
            }
            i += 1;
        }
    }
    None
}

fn glob_expand_braces(pattern: &str) -> Vec<String> {
    let bytes = pattern.as_bytes();
    let Some(open) = pattern.find('{') else {
        return vec![pattern.to_string()];
    };
    // Find the matching '}'.
    let mut depth = 0;
    let mut close = None;
    for (i, &b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(close) = close else {
        return vec![pattern.to_string()];
    };
    let prefix = &pattern[..open];
    let suffix = &pattern[close + 1..];
    let inner = &pattern[open + 1..close];
    let mut result = Vec::new();
    for alt in glob_split_top_commas(inner) {
        let combined = format!("{}{}{}", prefix, alt, suffix);
        result.extend(glob_expand_braces(&combined));
    }
    result
}

fn glob_split_top_commas(inner: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    let bytes = inner.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b',' if depth == 0 => {
                parts.push(inner[start..i].to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(inner[start..].to_string());
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_stream_round_trips() {
        let stream = fopen("php://temp", "w+").unwrap();

        assert_eq!(fwrite(&stream, "hello\n", None), Some(6));
        assert_eq!(fwrite(&stream, "world", None), Some(5));
        assert_eq!(ftell(&stream), Some(11));

        // Truncation past the end leaves the position untouched and reports the new size.
        assert!(rewind(&stream));
        assert_eq!(ftell(&stream), Some(0));
        assert!(!feof(&stream));

        // fgets reads up to and including the newline.
        assert_eq!(fgets(&stream, None).as_deref(), Some("hello\n"));
        // fread reads the requested number of bytes.
        assert_eq!(fread(&stream, 3).as_deref(), Some("wor"));
        assert_eq!(fgetc(&stream).as_deref(), Some("l"));
        assert_eq!(fread(&stream, 10).as_deref(), Some("d"));
        // A read past the end sets eof.
        assert_eq!(fread(&stream, 10).as_deref(), Some(""));
        assert!(feof(&stream));

        // fstat reports the buffer size.
        let stat = fstat(&stream).unwrap();
        assert_eq!(stat.get("size"), Some(&PhpMixed::Int(11)));

        // seek + truncate.
        assert_eq!(fseek(&stream, 5, SEEK_SET), 0);
        assert!(!feof(&stream)); // seek clears eof
        assert!(ftruncate(&stream, 5));
        assert_eq!(fstat(&stream).unwrap().get("size"), Some(&PhpMixed::Int(5)));

        assert!(fclose(&stream));
    }

    #[test]
    fn fopen_missing_file_is_err() {
        assert!(fopen("/nonexistent/shirabe/test/path", "r").is_err());
    }
}
