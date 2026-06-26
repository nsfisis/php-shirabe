use crate::{PhpMixed, PhpResource, StreamBacking};
use indexmap::IndexMap;

pub const STREAM_NOTIFY_FAILURE: i64 = 9;
pub const STREAM_NOTIFY_FILE_SIZE_IS: i64 = 5;
pub const STREAM_NOTIFY_PROGRESS: i64 = 7;

/// PHP `stream_get_contents()`: read the remaining bytes from the stream's current position.
/// TODO(phase-e): byte-string semantics — should return Vec<u8>; from_utf8_lossy can corrupt
/// binary reads.
pub fn stream_get_contents(stream: &PhpResource) -> Option<String> {
    stream_read_remaining(stream, None)
}

pub fn stream_resolve_include_path(filename: &str) -> Option<String> {
    // TODO(phase-d): resolution searches the `include_path` ini setting, which the shim does not
    // model; checking only the current directory would silently miss configured include paths.
    let _ = filename;
    todo!()
}

/// PHP `stream_get_contents()` with an explicit max length.
pub fn stream_get_contents_with_max(
    stream: &PhpResource,
    max_length: Option<i64>,
) -> Option<String> {
    stream_read_remaining(stream, max_length)
}

// Reads from the stream's current position: all remaining bytes, or up to `max_length` when given
// (a negative max means "until end").
fn stream_read_remaining(stream: &PhpResource, max_length: Option<i64>) -> Option<String> {
    use std::io::Read;
    match stream {
        PhpResource::Stdin => {
            let mut buf = Vec::new();
            match max_length {
                Some(l) if l >= 0 => {
                    let mut limited = std::io::stdin().take(l as u64);
                    limited.read_to_end(&mut buf).ok()?;
                }
                _ => {
                    std::io::stdin().read_to_end(&mut buf).ok()?;
                }
            }
            Some(String::from_utf8_lossy(&buf).into_owned())
        }
        PhpResource::Stdout | PhpResource::Stderr | PhpResource::Process(_) => None,
        PhpResource::Stream(state) => {
            let mut state = state.borrow_mut();
            if state.closed || !state.readable {
                return None;
            }
            let mut buf = Vec::new();
            let res = match max_length {
                Some(l) if l >= 0 => state.backing.as_rws().take(l as u64).read_to_end(&mut buf),
                _ => state.backing.as_rws().read_to_end(&mut buf),
            };
            res.ok()?;
            Some(String::from_utf8_lossy(&buf).into_owned())
        }
    }
}

// A stream context is modeled as an object holding the two arrays PHP keeps for it: the per-wrapper
// `options` and the `params`. This is a self-contained value, so it round-trips through the
// accessors below without any registry.
pub fn stream_context_create(
    options: &IndexMap<String, PhpMixed>,
    params: Option<&IndexMap<String, PhpMixed>>,
) -> PhpMixed {
    let mut context = IndexMap::new();
    context.insert("options".to_string(), PhpMixed::Array(options.clone()));
    context.insert(
        "params".to_string(),
        PhpMixed::Array(params.cloned().unwrap_or_default()),
    );
    PhpMixed::Object(context)
}

pub fn stream_context_get_options(stream_or_context: &PhpMixed) -> IndexMap<String, PhpMixed> {
    match stream_or_context {
        PhpMixed::Object(context) | PhpMixed::Array(context) => context
            .get("options")
            .and_then(PhpMixed::as_array)
            .cloned()
            .unwrap_or_default(),
        _ => IndexMap::new(),
    }
}

pub fn stream_context_get_params(stream_or_context: &PhpMixed) -> IndexMap<String, PhpMixed> {
    let (options, params) = match stream_or_context {
        PhpMixed::Object(context) | PhpMixed::Array(context) => (
            context.get("options").cloned().unwrap_or_default(),
            context
                .get("params")
                .and_then(PhpMixed::as_array)
                .cloned()
                .unwrap_or_default(),
        ),
        _ => (PhpMixed::default(), IndexMap::new()),
    };
    // PHP exposes the wrapper options under an "options" key alongside the params.
    let mut result = params;
    result.insert("options".to_string(), options);
    result
}

pub fn stream_isatty(stream: PhpResource) -> bool {
    stream_isatty_resource(&stream)
}

pub fn stream_get_wrappers() -> Vec<String> {
    // The full registered set depends on compiled-in extensions and runtime
    // `stream_wrapper_register` calls, which are not modeled. We return the wrappers always
    // registered by a stock PHP CLI build (the environment Composer's tests assume): the core
    // `php`/`file`/`glob`/`data` wrappers, the always-present `phar` wrapper, plus the
    // `compress.*`/`http`/`ftp` family. Consumers only use this to recognise `wrapper://` prefixes.
    [
        "https",
        "ftps",
        "compress.zlib",
        "compress.bzip2",
        "php",
        "file",
        "glob",
        "data",
        "http",
        "ftp",
        "phar",
        "zip",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// PHP `stream_copy_to_stream()`: copy the remaining bytes of `source` into `dest`, returning the
/// number of bytes copied (or `None` for `false`-on-failure).
pub fn stream_copy_to_stream(source: &PhpResource, dest: &PhpResource) -> Option<i64> {
    use std::io::{Read, Write};
    let mut buf = Vec::new();
    match source {
        PhpResource::Stdin => {
            std::io::stdin().read_to_end(&mut buf).ok()?;
        }
        PhpResource::Stdout | PhpResource::Stderr | PhpResource::Process(_) => return None,
        PhpResource::Stream(state) => {
            let mut state = state.borrow_mut();
            if state.closed || !state.readable {
                return None;
            }
            state.backing.as_rws().read_to_end(&mut buf).ok()?;
        }
    }
    match dest {
        PhpResource::Stdin | PhpResource::Process(_) => None,
        PhpResource::Stdout => std::io::stdout()
            .write_all(&buf)
            .ok()
            .map(|_| buf.len() as i64),
        PhpResource::Stderr => std::io::stderr()
            .write_all(&buf)
            .ok()
            .map(|_| buf.len() as i64),
        PhpResource::Stream(state) => {
            let mut state = state.borrow_mut();
            if state.closed || !state.writable {
                return None;
            }
            state.backing.as_rws().write_all(&buf).ok()?;
            Some(buf.len() as i64)
        }
    }
}

pub fn stream_isatty_resource(resource: &PhpResource) -> bool {
    use std::io::IsTerminal;
    match resource {
        PhpResource::Stdin => std::io::stdin().is_terminal(),
        PhpResource::Stdout => std::io::stdout().is_terminal(),
        PhpResource::Stderr => std::io::stderr().is_terminal(),
        PhpResource::Stream(_) | PhpResource::Process(_) => false,
    }
}

pub fn stream_get_meta_data(resource: &PhpResource) -> IndexMap<String, PhpMixed> {
    // (timed_out, blocked, eof, wrapper_type, stream_type, mode, seekable, uri)
    let (eof, wrapper_type, stream_type, mode, seekable, uri) = match resource {
        PhpResource::Process(_) => (false, "PHP", "STDIO", String::new(), false, ""),
        PhpResource::Stdin => (false, "PHP", "STDIO", "r".to_string(), false, "php://stdin"),
        PhpResource::Stdout => (
            false,
            "PHP",
            "STDIO",
            "w".to_string(),
            false,
            "php://stdout",
        ),
        PhpResource::Stderr => (
            false,
            "PHP",
            "STDIO",
            "w".to_string(),
            false,
            "php://stderr",
        ),
        PhpResource::Stream(state) => {
            let state = state.borrow();
            let (wrapper_type, stream_type) = match &state.backing {
                StreamBacking::Memory(_) => {
                    if state.uri.starts_with("php://temp") {
                        ("PHP", "TEMP")
                    } else {
                        ("PHP", "MEMORY")
                    }
                }
                StreamBacking::File(_) => ("plainfile", "STDIO"),
                StreamBacking::Pipe(_) => ("PHP", "STDIO"),
            };
            return build_meta_data(
                state.eof,
                wrapper_type,
                stream_type,
                state.mode.clone(),
                true,
                &state.uri,
            );
        }
    };
    build_meta_data(eof, wrapper_type, stream_type, mode, seekable, uri)
}

fn build_meta_data(
    eof: bool,
    wrapper_type: &str,
    stream_type: &str,
    mode: String,
    seekable: bool,
    uri: &str,
) -> IndexMap<String, PhpMixed> {
    let mut map = IndexMap::new();
    map.insert("timed_out".to_string(), PhpMixed::Bool(false));
    map.insert("blocked".to_string(), PhpMixed::Bool(true));
    map.insert("eof".to_string(), PhpMixed::Bool(eof));
    map.insert(
        "wrapper_type".to_string(),
        PhpMixed::String(wrapper_type.to_string()),
    );
    map.insert(
        "stream_type".to_string(),
        PhpMixed::String(stream_type.to_string()),
    );
    map.insert("mode".to_string(), PhpMixed::String(mode));
    map.insert("unread_bytes".to_string(), PhpMixed::Int(0));
    map.insert("seekable".to_string(), PhpMixed::Bool(seekable));
    map.insert("uri".to_string(), PhpMixed::String(uri.to_string()));
    map
}

// libc is already linked into every binary, so these can be declared directly without an extra
// crate (mirrors the `getuid`/`geteuid` declarations in process.rs).
const F_GETFL: i32 = 3;
const F_SETFL: i32 = 4;
const O_NONBLOCK: i32 = 0o4000;
const FD_SETSIZE: usize = 1024;

unsafe extern "C" {
    fn fcntl(fd: i32, cmd: i32, ...) -> i32;
    fn select(
        nfds: i32,
        readfds: *mut FdSet,
        writefds: *mut FdSet,
        exceptfds: *mut FdSet,
        timeout: *mut TimeVal,
    ) -> i32;
}

#[repr(C)]
struct TimeVal {
    tv_sec: i64,
    tv_usec: i64,
}

// `fd_set` is a bitmap of `FD_SETSIZE` bits laid out as an array of `long` words.
#[repr(C)]
struct FdSet {
    fds_bits: [i64; FD_SETSIZE / (8 * std::mem::size_of::<i64>())],
}

impl FdSet {
    fn zero() -> Self {
        FdSet {
            fds_bits: [0; FD_SETSIZE / (8 * std::mem::size_of::<i64>())],
        }
    }

    fn set(&mut self, fd: i32) {
        let bits = 8 * std::mem::size_of::<i64>();
        self.fds_bits[fd as usize / bits] |= 1i64 << (fd as usize % bits);
    }

    fn is_set(&self, fd: i32) -> bool {
        let bits = 8 * std::mem::size_of::<i64>();
        (self.fds_bits[fd as usize / bits] & (1i64 << (fd as usize % bits))) != 0
    }
}

/// PHP `stream_set_blocking()`: toggle `O_NONBLOCK` on the resource's underlying fd via `fcntl(2)`.
/// Returns `false` (PHP failure) when the resource has no fd or the syscall fails.
pub fn stream_set_blocking(resource: &PhpResource, enable: bool) -> bool {
    let Some(fd) = resource.raw_fd() else {
        return false;
    };
    let flags = unsafe { fcntl(fd, F_GETFL) };
    if flags < 0 {
        return false;
    }
    // `enable` means blocking, i.e. clear O_NONBLOCK.
    let new_flags = if enable {
        flags & !O_NONBLOCK
    } else {
        flags | O_NONBLOCK
    };
    unsafe { fcntl(fd, F_SETFL, new_flags) >= 0 }
}

/// PHP `stream_select`. Returns the number of changed streams, or `None` for the PHP `false`
/// returned when the underlying `select` is interrupted/fails. On success the `read`/`write`/
/// `except` vectors are narrowed to only the resources that became ready, matching PHP's
/// in-place rewrite of the passed arrays.
pub fn stream_select(
    read: &mut Vec<PhpResource>,
    write: &mut Vec<PhpResource>,
    except: &mut Vec<PhpResource>,
    seconds: i64,
    microseconds: Option<i64>,
) -> Option<i64> {
    let mut readfds = FdSet::zero();
    let mut writefds = FdSet::zero();
    let mut exceptfds = FdSet::zero();
    let mut nfds = 0i32;

    // Resources without an fd (in-memory streams, process handles) cannot be waited on; PHP would
    // emit a warning for them. We skip them here, leaving them out of the ready set.
    let mut prepare = |set: &mut FdSet, resources: &[PhpResource]| {
        for resource in resources {
            if let Some(fd) = resource.raw_fd()
                && (fd as usize) < FD_SETSIZE
            {
                set.set(fd);
                if fd + 1 > nfds {
                    nfds = fd + 1;
                }
            }
        }
    };
    prepare(&mut readfds, read);
    prepare(&mut writefds, write);
    prepare(&mut exceptfds, except);

    let mut timeout = TimeVal {
        tv_sec: seconds,
        tv_usec: microseconds.unwrap_or(0),
    };

    let ret = unsafe {
        select(
            nfds,
            &mut readfds,
            &mut writefds,
            &mut exceptfds,
            &mut timeout,
        )
    };

    if ret < 0 {
        // select(2) failed (e.g. EINTR). PHP returns false.
        return None;
    }

    // Narrow each array in place to the resources whose fd is still set in the result bitmap.
    let narrow = |set: &FdSet, resources: &mut Vec<PhpResource>| {
        resources.retain(|resource| match resource.raw_fd() {
            Some(fd) if (fd as usize) < FD_SETSIZE => set.is_set(fd),
            _ => false,
        });
    };
    narrow(&readfds, read);
    narrow(&writefds, write);
    narrow(&exceptfds, except);

    Some(ret as i64)
}

/// PHP `stream_get_contents($stream, $maxlength, $offset)`. A non-negative `offset` seeks there
/// first; `max_length < 0` reads until end.
pub fn stream_get_contents3(stream: &PhpResource, max_length: i64, offset: i64) -> Option<String> {
    if offset >= 0 {
        crate::fs::fseek(stream, offset, crate::fs::SEEK_SET);
    }
    let max = if max_length < 0 {
        None
    } else {
        Some(max_length)
    };
    stream_read_remaining(stream, max)
}

pub fn get_resource_type(resource: &PhpResource) -> String {
    match resource {
        PhpResource::Process(_) => "process".to_string(),
        _ => "stream".to_string(),
    }
}

/// Convenience wrapper over `fopen` for callers that open never-failing `php://` stdio streams and
/// want an infallible `PhpResource`. Panics on failure, matching the previous behavior.
pub fn php_fopen_resource(path: &str, mode: &str) -> PhpResource {
    crate::fs::fopen(path, mode)
        .unwrap_or_else(|e| panic!("php_fopen_resource failed to open {path:?}: {e}"))
}

pub fn php_stdout_resource() -> PhpResource {
    PhpResource::Stdout
}

pub fn php_stderr_resource() -> PhpResource {
    PhpResource::Stderr
}

pub fn stdin() -> PhpResource {
    PhpResource::Stdin
}
