//! Rust-to-PHP RPC over a Unix domain socket. See `docs/dev/php-rpc.md`.

use shirabe_external_packages::symfony::process::PhpExecutableFinder;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// PHP `\PHP_VERSION`.
pub fn get_php_version() -> String {
    call("get_php_version").unwrap_or_default()
}

/// PHP `\PHP_BINARY`.
pub fn get_php_binary() -> String {
    call("get_php_binary").unwrap_or_default()
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
    fn request(&mut self, name: &str) -> Option<String> {
        write_frame(&mut self.stream, name.as_bytes()).ok()?;
        let payload = read_frame(&mut self.stream).ok()?;
        parse_serialized_string(&payload)
    }
}

static WORKER: LazyLock<Mutex<Option<Worker>>> = LazyLock::new(|| Mutex::new(spawn_worker()));

fn call(name: &str) -> Option<String> {
    let mut guard = WORKER.lock().ok()?;
    guard.as_mut()?.request(name)
}

fn spawn_worker() -> Option<Worker> {
    let php = PhpExecutableFinder::new().find(false)?;

    let tempdir = tempfile::tempdir().ok()?;
    let socket_path = tempdir.path().join("rpc.sock");
    let script_path = tempdir.path().join("worker.php");
    std::fs::write(&script_path, GLUE_SCRIPT).ok()?;

    // Bind before spawning so the socket exists when the child connects.
    let listener = UnixListener::bind(&socket_path).ok()?;
    listener.set_nonblocking(true).ok()?;

    let child = std::process::Command::new(&php)
        .arg(&script_path)
        .arg(&socket_path)
        .spawn()
        .ok()?;

    // Poll for the child's connection with a bounded deadline so a child that never connects does
    // not hang the caller.
    let deadline = Instant::now() + Duration::from_secs(10);
    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return None;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => return None,
        }
    };
    stream.set_nonblocking(false).ok()?;

    Some(Worker {
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
