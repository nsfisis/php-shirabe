use crate::{ChildPipe, PhpMixed, PhpResource, StreamBacking, StreamState};
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
    use std::io::Write as _;
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

/// State held behind a `PhpResource::Process` handle returned by `proc_open`.
#[derive(Debug)]
pub struct ProcessState {
    /// The spawned child. Taken by `proc_close`/`wait`; once taken the handle is closed.
    child: Option<std::process::Child>,
    /// The command line passed to `proc_open`, reported back by `proc_get_status`.
    command: String,
}

/// One entry of the `descriptorspec` array passed to `proc_open`. Unlike PHP's array this is a
/// native type so it can carry a live `PhpResource` (e.g. a `/dev/null` stream).
#[derive(Debug)]
pub enum Descriptor {
    /// `['pipe', mode]` — `mode` is `"r"`/`"w"` from the child's point of view.
    Pipe(String),
    /// `['file', path, mode]`.
    File(String, String),
    /// `['pty']`.
    Pty,
    /// An already-opened stream resource used directly as the descriptor.
    Resource(PhpResource),
    /// A descriptor index left unspecified by a sparse PHP descriptorspec; the child inherits the
    /// corresponding parent fd.
    Inherit,
}

/// Extracts a `try_clone`d `std::fs::File` from a file-backed stream resource so it can be handed
/// to `Stdio::from` as a `proc_open` descriptor.
fn resource_to_file(resource: &PhpResource) -> std::io::Result<std::fs::File> {
    match resource {
        PhpResource::Stream(state) => {
            let state = state.borrow();
            match &state.backing {
                StreamBacking::File(f) => f.try_clone(),
                _ => Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "proc_open descriptor resource is not a file-backed stream",
                )),
            }
        }
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "proc_open descriptor is not a stream resource",
        )),
    }
}

/// PHP `proc_open`. Returns the process resource on success; the PHP `false` return is modeled as
/// `Err`. The `pipes` out-parameter is filled with the parent-side pipe streams keyed by fd index.
pub fn proc_open(
    command: &str,
    descriptorspec: &[Descriptor],
    pipes: &mut IndexMap<i64, PhpResource>,
    cwd: Option<&str>,
    env: Option<&[String]>,
    options: Option<&IndexMap<String, PhpMixed>>,
) -> std::io::Result<PhpResource> {
    // Windows-oriented options (bypass_shell, create_process_group, ...) have no effect here.
    let _ = options;

    let mut cmd = std::process::Command::new("/bin/sh");
    cmd.arg("-c").arg(command);

    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }

    if let Some(env) = env {
        // A provided environment replaces the inherited one, matching proc_open.
        cmd.env_clear();
        for pair in env {
            match pair.split_once('=') {
                Some((k, v)) => cmd.env(k, v),
                None => cmd.env(pair, ""),
            };
        }
    }

    // Remember which fds requested a pipe so their ends can be taken after spawn.
    let mut pipe_modes: Vec<(i64, String)> = Vec::new();
    for (index, descriptor) in descriptorspec.iter().enumerate() {
        let fd = index as i64;
        let stdio = match descriptor {
            Descriptor::Pipe(mode) => {
                pipe_modes.push((fd, mode.clone()));
                std::process::Stdio::piped()
            }
            Descriptor::File(path, mode) => {
                std::process::Stdio::from(resource_to_file(&crate::fs::fopen(path, mode)?)?)
            }
            Descriptor::Resource(resource) => {
                std::process::Stdio::from(resource_to_file(resource)?)
            }
            Descriptor::Inherit => std::process::Stdio::inherit(),
            Descriptor::Pty => {
                // TODO(phase-d): pty descriptors need a pseudo-terminal (openpty/ioctl); a syscall
                // crate is intentionally not introduced here.
                todo!("proc_open: pty descriptors require a pseudo-terminal (syscall)")
            }
        };
        match fd {
            0 => cmd.stdin(stdio),
            1 => cmd.stdout(stdio),
            2 => cmd.stderr(stdio),
            _ => {
                // TODO(phase-d): inheriting fds >= 3 (e.g. the --enable-sigchild pipe 3) requires
                // dup2/pre_exec; a syscall crate is intentionally not introduced here.
                todo!("proc_open: descriptors with fd >= 3 require fd inheritance (syscall)")
            }
        };
    }

    let mut child = cmd.spawn()?;

    for (fd, mode) in pipe_modes {
        // fd 0 is the child's stdin: the parent-side handle is writable. fds 1/2 are stdout/stderr:
        // the parent reads them.
        let (pipe, readable, writable) = match fd {
            0 => (ChildPipe::In(child.stdin.take().unwrap()), false, true),
            1 => (ChildPipe::Out(child.stdout.take().unwrap()), true, false),
            2 => (ChildPipe::Err(child.stderr.take().unwrap()), true, false),
            _ => unreachable!(),
        };
        let resource =
            PhpResource::Stream(std::rc::Rc::new(std::cell::RefCell::new(StreamState::new(
                StreamBacking::Pipe(pipe),
                readable,
                writable,
                mode,
                format!("pipe:fd{}", fd),
            ))));
        pipes.insert(fd, resource);
    }

    Ok(PhpResource::Process(std::rc::Rc::new(
        std::cell::RefCell::new(ProcessState {
            child: Some(child),
            command: command.to_string(),
        }),
    )))
}

/// PHP `proc_close`. Waits for the process to terminate and returns its exit code (-1 on failure).
/// Pipes are expected to have been closed by the caller beforehand.
pub fn proc_close(process: &PhpResource) -> i64 {
    if let PhpResource::Process(state) = process {
        let mut state = state.borrow_mut();
        if let Some(mut child) = state.child.take() {
            return match child.wait() {
                Ok(status) => status.code().map(|c| c as i64).unwrap_or(-1),
                Err(_) => -1,
            };
        }
    }
    -1
}

/// PHP `proc_get_status`. Reports the live status of the process behind the resource.
pub fn proc_get_status(process: &PhpResource) -> IndexMap<String, PhpMixed> {
    use std::os::unix::process::ExitStatusExt;

    let mut status = IndexMap::new();
    let PhpResource::Process(state) = process else {
        return status;
    };
    let mut state = state.borrow_mut();

    let pid = state.child.as_ref().map(|c| c.id() as i64).unwrap_or(-1);
    let mut running = false;
    let mut signaled = false;
    let mut exitcode = -1i64;
    let mut termsig = 0i64;

    if let Some(child) = state.child.as_mut() {
        match child.try_wait() {
            Ok(None) => running = true,
            Ok(Some(exit)) => {
                if let Some(code) = exit.code() {
                    exitcode = code as i64;
                }
                if let Some(sig) = exit.signal() {
                    signaled = true;
                    termsig = sig as i64;
                }
            }
            Err(_) => {}
        }
    }

    status.insert(
        "command".to_string(),
        PhpMixed::String(state.command.clone()),
    );
    status.insert("pid".to_string(), PhpMixed::Int(pid));
    status.insert("running".to_string(), PhpMixed::Bool(running));
    status.insert("signaled".to_string(), PhpMixed::Bool(signaled));
    status.insert("stopped".to_string(), PhpMixed::Bool(false));
    status.insert("exitcode".to_string(), PhpMixed::Int(exitcode));
    status.insert("termsig".to_string(), PhpMixed::Int(termsig));
    status.insert("stopsig".to_string(), PhpMixed::Int(0));
    status
}

pub fn proc_terminate(process: &PhpResource, signal: i64) -> bool {
    let _ = (process, signal);
    // TODO(phase-d): sending an arbitrary signal requires kill(2); std's Child::kill only sends
    // SIGKILL and a syscall crate is intentionally not introduced here.
    todo!(
        "proc_terminate: arbitrary signal delivery requires kill(2) (syscall crate not available)"
    )
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

#[repr(C)]
struct Passwd {
    pw_name: *const std::os::raw::c_char,
    pw_passwd: *const std::os::raw::c_char,
    pw_uid: u32,
    pw_gid: u32,
    pw_gecos: *const std::os::raw::c_char,
    pw_dir: *const std::os::raw::c_char,
    pw_shell: *const std::os::raw::c_char,
}

unsafe extern "C" {
    fn getuid() -> u32;
    fn geteuid() -> u32;
    fn getpwuid(uid: u32) -> *const Passwd;
}

pub fn posix_getuid() -> i64 {
    // getuid(2) cannot fail; libc is already linked into every binary, so no extra crate is needed.
    (unsafe { getuid() }) as i64
}

pub fn posix_geteuid() -> i64 {
    // geteuid(2) cannot fail; libc is already linked into every binary, so no extra crate is needed.
    (unsafe { geteuid() }) as i64
}

pub fn posix_getpwuid(uid: i64) -> PhpMixed {
    // getpwuid(3) via libc (already linked); mirrors PHP posix_getpwuid returning an associative
    // array of the passwd entry, or false when no entry matches the uid.
    let pw = unsafe { getpwuid(uid as u32) };
    if pw.is_null() {
        return PhpMixed::Bool(false);
    }
    let cstr = |p: *const std::os::raw::c_char| -> String {
        if p.is_null() {
            String::new()
        } else {
            unsafe { std::ffi::CStr::from_ptr(p) }
                .to_string_lossy()
                .into_owned()
        }
    };
    let pw = unsafe { &*pw };
    let mut entry = indexmap::IndexMap::new();
    entry.insert("name".to_string(), PhpMixed::String(cstr(pw.pw_name)));
    entry.insert("passwd".to_string(), PhpMixed::String(cstr(pw.pw_passwd)));
    entry.insert("uid".to_string(), PhpMixed::Int(pw.pw_uid as i64));
    entry.insert("gid".to_string(), PhpMixed::Int(pw.pw_gid as i64));
    entry.insert("gecos".to_string(), PhpMixed::String(cstr(pw.pw_gecos)));
    entry.insert("dir".to_string(), PhpMixed::String(cstr(pw.pw_dir)));
    entry.insert("shell".to_string(), PhpMixed::String(cstr(pw.pw_shell)));
    PhpMixed::Array(entry)
}

pub fn posix_isatty(stream: PhpResource) -> bool {
    use std::io::IsTerminal;
    match stream {
        PhpResource::Stdin => std::io::stdin().is_terminal(),
        PhpResource::Stdout => std::io::stdout().is_terminal(),
        PhpResource::Stderr => std::io::stderr().is_terminal(),
        // A regular file, in-memory stream or process handle is never a tty.
        PhpResource::Stream(_) | PhpResource::Process(_) => false,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{fclose, fwrite};
    use crate::stream::stream_get_contents;

    #[test]
    fn proc_open_reads_stdout_and_reports_status() {
        let mut pipes = IndexMap::new();
        let process = proc_open(
            "echo hi",
            &[
                Descriptor::Inherit,
                Descriptor::Pipe("w".to_string()),
                Descriptor::Inherit,
            ],
            &mut pipes,
            None,
            None,
            None,
        )
        .unwrap();

        let stdout = pipes.get(&1).unwrap();
        assert_eq!(stream_get_contents(stdout).unwrap(), "hi\n");

        // Reading to EOF means the child has finished; the status converges to "not running".
        let status = loop {
            let status = proc_get_status(&process);
            if !crate::php_truthy(status.get("running").unwrap()) {
                break status;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert_eq!(status.get("command").unwrap().as_string(), Some("echo hi"));
        assert!(status.get("pid").unwrap().as_int().unwrap() > 0);
        assert_eq!(status.get("exitcode").unwrap().as_int(), Some(0));
        assert_eq!(status.get("signaled").unwrap().as_bool(), Some(false));

        for (_, pipe) in &pipes {
            fclose(pipe);
        }
        assert_eq!(proc_close(&process), 0);
    }

    #[test]
    fn proc_open_writes_stdin_pipe() {
        let mut pipes = IndexMap::new();
        let process = proc_open(
            "cat",
            &[
                Descriptor::Pipe("r".to_string()),
                Descriptor::Pipe("w".to_string()),
                Descriptor::Inherit,
            ],
            &mut pipes,
            None,
            None,
            None,
        )
        .unwrap();

        fwrite(pipes.get(&0).unwrap(), "ping\n", None);
        fclose(pipes.get(&0).unwrap());
        // Dropping the last handle closes the fd so `cat` sees end-of-input.
        pipes.shift_remove(&0);

        assert_eq!(
            stream_get_contents(pipes.get(&1).unwrap()).unwrap(),
            "ping\n"
        );

        assert_eq!(proc_close(&process), 0);
    }

    #[test]
    fn proc_open_redirects_stdout_to_file() {
        let path =
            std::env::temp_dir().join(format!("shirabe_proc_open_{}.txt", std::process::id()));
        let path_str = path.to_str().unwrap();

        let mut pipes = IndexMap::new();
        let process = proc_open(
            "echo filetest",
            &[
                Descriptor::Inherit,
                Descriptor::File(path_str.to_string(), "w".to_string()),
                Descriptor::Inherit,
            ],
            &mut pipes,
            None,
            None,
            None,
        )
        .unwrap();
        assert!(pipes.is_empty());
        assert_eq!(proc_close(&process), 0);

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "filetest\n");
        std::fs::remove_file(&path).ok();
    }
}
