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

pub fn stream_context_create(
    _options: &IndexMap<String, PhpMixed>,
    _params: Option<&IndexMap<String, PhpMixed>>,
) -> PhpMixed {
    todo!()
}

pub fn stream_context_get_options(_stream_or_context: &PhpMixed) -> IndexMap<String, PhpMixed> {
    todo!()
}

pub fn stream_context_get_params(_stream_or_context: &PhpMixed) -> IndexMap<String, PhpMixed> {
    todo!()
}

pub fn stream_isatty(stream: PhpResource) -> bool {
    stream_isatty_resource(&stream)
}

pub fn stream_get_wrappers() -> Vec<String> {
    todo!()
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

pub fn stream_set_blocking(_resource: &PhpResource, _enable: bool) -> bool {
    // TODO(phase-d): toggling O_NONBLOCK requires fcntl(2); a syscall crate is intentionally not
    // introduced here.
    todo!("stream_set_blocking requires fcntl(2) (syscall crate not available)")
}

/// PHP `stream_select`. Returns the number of changed streams, or `None` for the PHP `false`
/// returned when the underlying `select` is interrupted/fails.
pub fn stream_select(
    _read: &mut Vec<PhpResource>,
    _write: &mut Vec<PhpResource>,
    _except: &mut Vec<PhpResource>,
    _seconds: i64,
    _microseconds: Option<i64>,
) -> Option<i64> {
    // TODO(phase-d): multiplexing readiness requires select(2)/poll(2); a syscall crate is
    // intentionally not introduced here.
    todo!("stream_select requires select(2)/poll(2) (syscall crate not available)")
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

pub fn is_resource_value(_resource: &PhpResource) -> bool {
    true
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
