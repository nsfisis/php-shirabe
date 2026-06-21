use crate::{PhpMixed, PhpResource};
use indexmap::IndexMap;

pub const STREAM_NOTIFY_FAILURE: i64 = 9;
pub const STREAM_NOTIFY_FILE_SIZE_IS: i64 = 5;
pub const STREAM_NOTIFY_PROGRESS: i64 = 7;

pub const STDERR: i64 = 2;

pub fn stream_get_contents(_stream: PhpMixed) -> Option<String> {
    todo!()
}

pub fn stream_resolve_include_path(filename: &str) -> Option<String> {
    let _ = filename;
    todo!()
}

pub fn stream_get_contents_with_max(stream: PhpMixed, max_length: Option<i64>) -> Option<String> {
    let _ = (stream, max_length);
    todo!()
}

pub fn stream_context_create(
    _options: &IndexMap<String, PhpMixed>,
    _params: Option<&IndexMap<String, PhpMixed>>,
) -> PhpMixed {
    todo!()
}

pub fn stream_isatty(stream: PhpResource) -> bool {
    stream_isatty_resource(&stream)
}

pub fn stream_get_wrappers() -> Vec<String> {
    todo!()
}

pub fn stream_copy_to_stream(_source: PhpMixed, _dest: PhpMixed) -> Option<i64> {
    todo!()
}

pub fn stream_isatty_resource(resource: &PhpResource) -> bool {
    use std::io::IsTerminal;
    match resource {
        PhpResource::Stdin => std::io::stdin().is_terminal(),
        PhpResource::Stdout => std::io::stdout().is_terminal(),
        PhpResource::Stderr => std::io::stderr().is_terminal(),
        PhpResource::File(_) => false,
    }
}

pub fn stream_get_meta_data(_resource: &PhpResource) -> IndexMap<String, PhpMixed> {
    todo!()
}

pub fn stream_set_blocking(_resource: &PhpResource, _enable: bool) -> bool {
    todo!()
}

pub fn stream_select(
    _read: &mut Vec<PhpResource>,
    _write: &mut Vec<PhpResource>,
    _except: &mut Vec<PhpResource>,
    _seconds: i64,
    _microseconds: Option<i64>,
) -> i64 {
    todo!()
}

pub fn stream_get_contents3(_stream: PhpMixed, _max_length: i64, _offset: i64) -> Option<String> {
    todo!()
}

pub fn is_resource_value(_resource: &PhpResource) -> bool {
    true
}

pub fn get_resource_type(_resource: &PhpResource) -> String {
    "stream".to_string()
}

pub fn php_fopen_resource(path: &str, mode: &str) -> PhpResource {
    match path {
        "php://output" | "php://stdout" => return PhpResource::Stdout,
        "php://stderr" => return PhpResource::Stderr,
        "php://stdin" | "php://input" => return PhpResource::Stdin,
        _ => {}
    }
    // Strip the binary/text flags PHP accepts as part of the mode.
    let base_mode: String = mode.chars().filter(|c| *c != 'b' && *c != 't').collect();
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
        _ => options.read(true),
    };
    let file = options
        .open(path)
        .unwrap_or_else(|e| panic!("php_fopen_resource failed to open {path:?}: {e}"));
    PhpResource::File(std::rc::Rc::new(std::cell::RefCell::new(file)))
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
