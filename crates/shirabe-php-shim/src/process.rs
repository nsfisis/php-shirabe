use crate::{PhpMixed, PhpResource};
use indexmap::IndexMap;

pub const SIGINT: i64 = 2;
pub const SIGTERM: i64 = 15;
pub const SIGUSR1: i64 = 10;
pub const SIGUSR2: i64 = 12;

pub fn exec(
    command: &str,
    output: Option<&mut Vec<String>>,
    exit_code: Option<&mut i64>,
) -> Option<String> {
    let result = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .output()
        .ok()?;
    if let Some(code) = exit_code {
        *code = result.status.code().unwrap_or(-1) as i64;
    }
    let stdout = String::from_utf8_lossy(&result.stdout);
    let mut lines: Vec<String> = stdout
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
        .collect();
    // Drop the single trailing empty line produced by a terminating newline.
    if lines.last().map(String::is_empty).unwrap_or(false) {
        lines.pop();
    }
    let last = lines.last().cloned().unwrap_or_default();
    if let Some(out) = output {
        // PHP appends to the array rather than replacing it.
        out.extend(lines);
    }
    Some(last)
}

pub fn shell_exec(command: &str) -> Option<String> {
    let result = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&result.stdout).into_owned();
    // PHP returns null when the command produces no output.
    if stdout.is_empty() {
        None
    } else {
        Some(stdout)
    }
}

pub fn system(command: &str, result_code: Option<&mut i64>) -> Option<String> {
    use std::io::Write;
    let result = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .output()
        .ok()?;
    if let Some(code) = result_code {
        *code = result.status.code().unwrap_or(-1) as i64;
    }
    // PHP system() passes the command output straight through to the script's output.
    // TODO(phase-d): PHP flushes line by line as the command runs; here the whole output is captured
    // and emitted once the command finishes, which changes interleaving/streaming timing.
    let _ = std::io::stdout().write_all(&result.stdout);
    let _ = std::io::stdout().flush();
    let stdout = String::from_utf8_lossy(&result.stdout);
    let mut lines: Vec<String> = stdout
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
        .collect();
    if lines.last().map(String::is_empty).unwrap_or(false) {
        lines.pop();
    }
    Some(lines.last().cloned().unwrap_or_default())
}

// Port of PHP's escapeshellcmd (Unix branch). Shell metacharacters are backslash-escaped; quote
// characters are escaped only when unpaired, paired quotes being left intact.
pub fn escapeshellcmd(command: &str) -> String {
    let bytes = command.as_bytes();
    let len = bytes.len();
    let mut out: Vec<u8> = Vec::with_capacity(len);
    // Byte index of the matching closing quote while inside a paired quote run.
    let mut paired: Option<usize> = None;
    let mut x = 0;
    while x < len {
        let c = bytes[x];
        match c {
            b'"' | b'\'' => {
                if paired.is_none() {
                    if let Some(rel) = bytes[x + 1..].iter().position(|&b| b == c) {
                        paired = Some(x + 1 + rel);
                    } else {
                        out.push(b'\\');
                    }
                } else if paired == Some(x) {
                    paired = None;
                } else {
                    out.push(b'\\');
                }
                out.push(c);
            }
            b'#' | b'&' | b';' | b'`' | b'|' | b'*' | b'?' | b'~' | b'<' | b'>' | b'^' | b'('
            | b')' | b'[' | b']' | b'{' | b'}' | b'$' | b'\\' | 0x0A | 0xFF => {
                out.push(b'\\');
                out.push(c);
            }
            _ => out.push(c),
        }
        x += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// Unix branch of PHP's escapeshellarg: wrap in single quotes, escaping embedded single quotes.
pub fn escapeshellarg(arg: &str) -> String {
    let mut out = String::with_capacity(arg.len() + 2);
    out.push('\'');
    for c in arg.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

// TODO(phase-c): reports proc_open as unavailable (returns false), so callers fall back to
// their defaults. A real implementation requires holding the child process and its pipes; defer
// it to the broader process-subsystem work (ProcessExecutor).
pub fn proc_open(
    _command: &str,
    _descriptorspec: &[PhpMixed],
    _pipes: &mut PhpMixed,
    _cwd: Option<&str>,
    _env: Option<&[String]>,
    _options: Option<&IndexMap<String, PhpMixed>>,
) -> PhpMixed {
    PhpMixed::Bool(false)
}

pub fn proc_close(_process: PhpMixed) -> i64 {
    -1
}

pub fn proc_get_status(_process: &PhpMixed) -> IndexMap<String, PhpMixed> {
    // TODO(phase-d): depends on proc_open returning a real process handle, which is itself deferred
    // (see proc_open above). Without a live child there is no status to report.
    todo!()
}

pub fn proc_terminate(_process: &PhpMixed, _signal: i64) -> bool {
    // TODO(phase-d): depends on proc_open returning a real process handle (see proc_open above).
    todo!()
}

pub fn getmypid() -> i64 {
    std::process::id() as i64
}

pub fn cli_set_process_title(_title: &str) -> bool {
    // TODO(phase-d): changing the process title visible to ps(1) requires platform-specific calls
    // (prctl/setproctitle) not available without a libc/syscall crate.
    todo!()
}

pub fn setproctitle(_title: &str) {
    // TODO(phase-d): see cli_set_process_title; requires platform-specific process-title support.
    todo!()
}

// No-op until real signal handling is wired up; signal registration itself is
// deferred (see the TODO(plugin) notes in SignalRegistry::register).
pub fn pcntl_async_signals(_enable: bool) {}

pub fn pcntl_signal(_signal: i64, _handler: PhpMixed) -> bool {
    // TODO(phase-d): registering a signal handler requires the signal-handling subsystem to be wired
    // up (cf. SignalRegistry / the TODO(plugin) notes), plus a syscall crate for sigaction.
    todo!()
}

pub fn pcntl_signal_get_handler(_signal: i64) -> PhpMixed {
    // TODO(phase-d): see pcntl_signal; needs the signal-handling subsystem.
    todo!()
}

pub fn posix_getuid() -> i64 {
    // TODO(phase-d): getuid(2) is not reachable without a libc/syscall crate.
    todo!()
}

pub fn posix_geteuid() -> i64 {
    // TODO(phase-d): geteuid(2) is not reachable without a libc/syscall crate.
    todo!()
}

pub fn posix_getpwuid(_uid: i64) -> PhpMixed {
    // TODO(phase-d): getpwuid(3) is not reachable without a libc/syscall crate.
    todo!()
}

pub fn posix_isatty(stream: PhpResource) -> bool {
    use std::io::IsTerminal;
    match stream {
        PhpResource::Stdin => std::io::stdin().is_terminal(),
        PhpResource::Stdout => std::io::stdout().is_terminal(),
        PhpResource::Stderr => std::io::stderr().is_terminal(),
        // A regular file or in-memory stream is never a tty.
        PhpResource::Stream(_) => false,
    }
}

pub fn posix_kill(_pid: i64, _signal: i64) -> bool {
    // TODO(phase-d): kill(2) is not reachable without a libc/syscall crate.
    todo!()
}

pub fn get_current_user() -> String {
    // TODO(phase-d): PHP returns the owner name of the running script file, which needs stat(2) plus
    // getpwuid(3); neither is reachable without a libc/syscall crate.
    todo!()
}
