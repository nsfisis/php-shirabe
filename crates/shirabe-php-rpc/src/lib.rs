//! Rust-to-PHP RPC over a Unix domain socket. See `docs/dev/php-rpc.md`.

use shirabe_external_packages::symfony::process::PhpExecutableFinder;
use shirabe_php_shim::PhpMixed;
use std::io::{Read as _, Write as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// PHP `\PHP_VERSION`.
pub fn get_php_version() -> String {
    match get_constant("PHP_VERSION") {
        PhpMixed::String(s) => s,
        other => panic!("PHP RPC: PHP_VERSION constant did not resolve to a string: {other:?}"),
    }
}

/// PHP `\PHP_BINARY`.
pub fn get_php_binary() -> String {
    match get_constant("PHP_BINARY") {
        PhpMixed::String(s) => s,
        other => panic!("PHP RPC: PHP_BINARY constant did not resolve to a string: {other:?}"),
    }
}

/// PHP `defined($name)`.
pub fn has_constant(name: &str) -> bool {
    match call("defined", name) {
        PhpMixed::Bool(b) => b,
        other => panic!("PHP RPC: `defined` did not return a bool: {other:?}"),
    }
}

/// PHP `constant($name)`.
pub fn get_constant(name: &str) -> PhpMixed {
    call("constant", name)
}

/// PHP `inet_pton($address)`.
pub fn inet_pton(address: &str) -> PhpMixed {
    call("inet_pton", address)
}

/// PHP `curl_version()['version']`.
pub fn curl_version() -> Option<String> {
    match call("curl_version", "") {
        PhpMixed::String(s) => Some(s),
        PhpMixed::Null => None,
        other => panic!("PHP RPC: `curl_version` returned an unexpected value: {other:?}"),
    }
}

/// PHP `(new \ReflectionExtension($name))->info()` output.
pub fn get_extension_info(name: &str) -> String {
    match call("extension_info", name) {
        PhpMixed::String(s) => s,
        other => panic!("PHP RPC: `extension_info` did not return a string: {other:?}"),
    }
}

/// PHP `phpversion($extension)`.
pub fn phpversion(extension: &str) -> Option<String> {
    match call("phpversion", extension) {
        PhpMixed::String(s) => Some(s),
        PhpMixed::Bool(false) => None,
        other => panic!("PHP RPC: `phpversion` returned an unexpected value: {other:?}"),
    }
}

/// PHP `get_loaded_extensions()`.
///
/// Extension names are joined with `,` on the PHP side and split back here; real
/// extension names never contain a comma.
pub fn get_loaded_extensions() -> Vec<String> {
    match call("get_loaded_extensions", "") {
        PhpMixed::String(s) if s.is_empty() => Vec::new(),
        PhpMixed::String(s) => s.split(',').map(|s| s.to_string()).collect(),
        other => panic!("PHP RPC: `get_loaded_extensions` did not return a string: {other:?}"),
    }
}

const GLUE_SCRIPT: &str = include_str!("../php/worker.php");

struct Worker {
    stream: UnixStream,
    // Kept alive for the process lifetime: the child interpreter and the temp dir holding the socket
    // and glue script. Neither is dropped because the worker lives in a never-dropped static.
    _child: std::process::Child,
    _tempdir: tempfile::TempDir,
}

impl Worker {
    fn request(&mut self, name: &str, arg: &str) -> anyhow::Result<Vec<u8>> {
        let mut payload = name.as_bytes().to_vec();
        payload.push(0);
        payload.extend_from_slice(arg.as_bytes());
        write_frame(&mut self.stream, &payload)?;
        Ok(read_frame(&mut self.stream)?)
    }
}

// TODO(phase-d): every failure here panics rather than propagating a `Result`; this is an interim
// step until PHP RPC gets proper error handling (see docs/dev/php-rpc.md).
static WORKER: LazyLock<Mutex<Worker>> = LazyLock::new(|| {
    Mutex::new(
        spawn_worker().unwrap_or_else(|e| panic!("PHP RPC: failed to spawn PHP worker: {e:#}")),
    )
});

fn call(name: &str, arg: &str) -> PhpMixed {
    let mut guard = WORKER
        .lock()
        .unwrap_or_else(|e| panic!("PHP RPC: worker mutex poisoned: {e}"));
    let payload = guard
        .request(name, arg)
        .unwrap_or_else(|e| panic!("PHP RPC: request `{name}` failed: {e:#}"));
    parse_serialized_scalar(&payload).unwrap_or_else(|| {
        panic!("PHP RPC: request `{name}` returned an unparseable payload: {payload:?}")
    })
}

fn spawn_worker() -> anyhow::Result<Worker> {
    let php = PhpExecutableFinder::new()
        .find(false)
        .ok_or_else(|| anyhow::anyhow!("no PHP executable found"))?;

    let tempdir = tempfile::tempdir()?;
    let socket_path = tempdir.path().join("rpc.sock");
    let script_path = tempdir.path().join("worker.php");
    std::fs::write(&script_path, GLUE_SCRIPT)?;

    // Bind before spawning so the socket exists when the child connects.
    let listener = UnixListener::bind(&socket_path)?;
    listener.set_nonblocking(true)?;

    let child = std::process::Command::new(&php)
        .arg(&script_path)
        .arg(&socket_path)
        .spawn()?;

    // Poll for the child's connection with a bounded deadline so a child that never connects does
    // not hang the caller.
    let deadline = Instant::now() + Duration::from_secs(10);
    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    anyhow::bail!("timed out waiting for the PHP worker to connect");
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => return Err(e.into()),
        }
    };
    stream.set_nonblocking(false)?;

    Ok(Worker {
        stream,
        _child: child,
        _tempdir: tempdir,
    })
}

fn write_frame(stream: &mut UnixStream, payload: &[u8]) -> std::io::Result<()> {
    stream.write_all(&(payload.len() as u64).to_le_bytes())?;
    stream.write_all(payload)?;
    stream.flush()
}

fn read_frame(stream: &mut UnixStream) -> std::io::Result<Vec<u8>> {
    let mut header = [0u8; 8];
    stream.read_exact(&mut header)?;
    let len = u64::from_le_bytes(header) as usize;
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload)?;
    Ok(payload)
}

/// Parse the `s:<len>:"<bytes>";` form; only strings are needed here, so other forms are rejected.
fn parse_serialized_string(payload: &[u8]) -> Option<String> {
    let rest = payload.strip_prefix(b"s:")?;
    let colon = rest.iter().position(|&b| b == b':')?;
    let len: usize = std::str::from_utf8(&rest[..colon]).ok()?.parse().ok()?;
    let after = rest.get(colon + 1..)?;
    let bytes = after.strip_prefix(b"\"")?.get(..len)?;
    Some(String::from_utf8_lossy(bytes).into_owned())
}

/// Parse any of PHP's scalar/null `serialize()` forms: `N;`, `b:0/1;`, `i:<n>;`, `d:<f>;`,
/// `s:<len>:"<bytes>";`.
fn parse_serialized_scalar(payload: &[u8]) -> Option<PhpMixed> {
    if payload == b"N;" {
        return Some(PhpMixed::Null);
    }
    if let Some(rest) = payload.strip_prefix(b"b:") {
        return match rest.strip_suffix(b";")? {
            b"0" => Some(PhpMixed::Bool(false)),
            b"1" => Some(PhpMixed::Bool(true)),
            _ => None,
        };
    }
    if let Some(rest) = payload.strip_prefix(b"i:") {
        let s = std::str::from_utf8(rest.strip_suffix(b";")?).ok()?;
        return s.parse().ok().map(PhpMixed::Int);
    }
    if let Some(rest) = payload.strip_prefix(b"d:") {
        let s = std::str::from_utf8(rest.strip_suffix(b";")?).ok()?;
        return s.parse().ok().map(PhpMixed::Float);
    }
    if payload.starts_with(b"s:") {
        return parse_serialized_string(payload).map(PhpMixed::String);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_string_scalar() {
        assert_eq!(
            parse_serialized_string(b"s:5:\"8.5.7\";").as_deref(),
            Some("8.5.7"),
        );
    }

    #[test]
    fn parses_empty_string() {
        assert_eq!(parse_serialized_string(b"s:0:\"\";").as_deref(), Some(""));
    }

    #[test]
    fn parses_string_with_embedded_quote() {
        assert_eq!(
            parse_serialized_string(b"s:3:\"a\"b\";").as_deref(),
            Some("a\"b"),
        );
    }

    #[test]
    fn rejects_non_string_scalars() {
        assert_eq!(parse_serialized_string(b"i:42;"), None);
        assert_eq!(parse_serialized_string(b"N;"), None);
        assert_eq!(parse_serialized_string(b"b:1;"), None);
    }

    #[test]
    fn rejects_truncated_string() {
        assert_eq!(parse_serialized_string(b"s:5:\"ab\";"), None);
    }

    #[test]
    fn rejects_non_numeric_length() {
        assert_eq!(parse_serialized_string(b"s:x:\"ab\";"), None);
    }

    #[test]
    fn parses_scalar_null() {
        assert_eq!(parse_serialized_scalar(b"N;"), Some(PhpMixed::Null));
    }

    #[test]
    fn parses_scalar_bool() {
        assert_eq!(
            parse_serialized_scalar(b"b:0;"),
            Some(PhpMixed::Bool(false))
        );
        assert_eq!(parse_serialized_scalar(b"b:1;"), Some(PhpMixed::Bool(true)));
    }

    #[test]
    fn parses_scalar_int() {
        assert_eq!(parse_serialized_scalar(b"i:8;"), Some(PhpMixed::Int(8)));
        assert_eq!(parse_serialized_scalar(b"i:-1;"), Some(PhpMixed::Int(-1)));
    }

    #[test]
    fn parses_scalar_float() {
        assert_eq!(
            parse_serialized_scalar(b"d:1.5;"),
            Some(PhpMixed::Float(1.5))
        );
    }

    #[test]
    fn parses_scalar_string() {
        assert_eq!(
            parse_serialized_scalar(b"s:5:\"8.5.7\";"),
            Some(PhpMixed::String("8.5.7".to_string())),
        );
    }

    #[test]
    fn rejects_malformed_scalar() {
        assert_eq!(parse_serialized_scalar(b"b:2;"), None);
        assert_eq!(parse_serialized_scalar(b"i:x;"), None);
        assert_eq!(parse_serialized_scalar(b"d:x;"), None);
        assert_eq!(parse_serialized_scalar(b"garbage"), None);
    }

    #[test]
    fn frame_roundtrip() {
        let (mut a, mut b) = UnixStream::pair().unwrap();
        write_frame(&mut a, b"get_php_version").unwrap();
        assert_eq!(read_frame(&mut b).unwrap(), b"get_php_version");
    }

    #[test]
    fn frame_roundtrip_empty_payload() {
        let (mut a, mut b) = UnixStream::pair().unwrap();
        write_frame(&mut a, b"").unwrap();
        assert_eq!(read_frame(&mut b).unwrap(), b"");
    }

    #[test]
    fn queries_real_php_when_available() {
        if PhpExecutableFinder::new().find(false).is_none() {
            // No PHP in this environment; the worker cannot start.
            return;
        }

        let version = get_php_version();
        assert!(!version.is_empty(), "expected a PHP version");
        assert!(
            version
                .split('.')
                .next()
                .and_then(|n| n.parse::<u32>().ok())
                .is_some(),
            "version should start with a number: {version}",
        );

        let binary = get_php_binary();
        assert!(!binary.is_empty(), "expected a PHP binary path");
        assert!(
            std::path::Path::new(&binary).exists(),
            "binary should exist: {binary}",
        );
    }

    #[test]
    fn queries_constants_when_php_available() {
        if PhpExecutableFinder::new().find(false).is_none() {
            // No PHP in this environment; the worker cannot start.
            return;
        }

        assert!(has_constant("PHP_VERSION"));
        assert!(!has_constant("SHIRABE_DOES_NOT_EXIST_XYZ"));

        assert_eq!(get_constant("PHP_INT_SIZE"), PhpMixed::Int(8));
        assert_eq!(get_constant("SHIRABE_DOES_NOT_EXIST_XYZ"), PhpMixed::Null);
        match get_constant("PHP_VERSION") {
            PhpMixed::String(s) => assert!(!s.is_empty(), "expected a non-empty PHP_VERSION"),
            other => panic!("expected a string, got {other:?}"),
        }
    }
}
