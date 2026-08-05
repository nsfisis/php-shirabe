use crate::{ChildPipe, PhpMixed, PhpResource, StreamBacking, StreamState};
use indexmap::IndexMap;
pub use shirabe_php_src::standard::exec::escapeshellcmd;

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
    // TODO(phase-c): PHP flushes line by line as the command runs; here the whole output is captured
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
    cwd: Option<&std::path::Path>,
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
    // Descriptors beyond stderr, as (target fd, child-side end). `Command` cannot express them, so
    // the child installs them with dup2(2) between fork and exec.
    let mut extra_fds: Vec<(std::os::fd::RawFd, std::os::fd::OwnedFd)> = Vec::new();
    for (index, descriptor) in descriptorspec.iter().enumerate() {
        let fd = index as i64;
        if fd >= 3 {
            match descriptor {
                Descriptor::Pipe(mode) => {
                    // O_CLOEXEC keeps the parent end out of the child; dup2 clears it on the
                    // child end, which is what makes that one survive the exec.
                    let (read_end, write_end) = nix::unistd::pipe2(nix::fcntl::OFlag::O_CLOEXEC)
                        .map_err(std::io::Error::from)?;
                    // The mode is the child's point of view, so "r" means the child reads and the
                    // parent writes.
                    let child_reads = mode.starts_with('r');
                    let (child_end, parent_end) = if child_reads {
                        (read_end, write_end)
                    } else {
                        (write_end, read_end)
                    };
                    extra_fds.push((fd as std::os::fd::RawFd, child_end));
                    pipes.insert(
                        fd,
                        PhpResource::Stream(std::rc::Rc::new(std::cell::RefCell::new(
                            StreamState::new(
                                StreamBacking::Pipe(ChildPipe::Extra(std::fs::File::from(
                                    parent_end,
                                ))),
                                !child_reads,
                                child_reads,
                                mode.clone(),
                                format!("pipe:fd{}", fd),
                            ),
                        ))),
                    );
                }
                Descriptor::File(path, mode) => {
                    extra_fds.push((
                        fd as std::os::fd::RawFd,
                        std::os::fd::OwnedFd::from(resource_to_file(&crate::fs::fopen(
                            path, mode,
                        )?)?),
                    ));
                }
                Descriptor::Resource(resource) => {
                    extra_fds.push((
                        fd as std::os::fd::RawFd,
                        std::os::fd::OwnedFd::from(resource_to_file(resource)?),
                    ));
                }
                // A gap in a sparse descriptorspec: the child keeps whatever the parent has there.
                Descriptor::Inherit => {}
            }
            continue;
        }
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
        };
        match fd {
            0 => cmd.stdin(stdio),
            1 => cmd.stdout(stdio),
            2 => cmd.stderr(stdio),
            _ => unreachable!(),
        };
    }

    if !extra_fds.is_empty() {
        use std::os::fd::{AsRawFd as _, FromRawFd as _, IntoRawFd as _};
        use std::os::unix::process::CommandExt as _;
        let install_extra_fds = move || {
            for (target, child_end) in &extra_fds {
                if child_end.as_raw_fd() == *target {
                    // dup2(fd, fd) is a no-op that leaves FD_CLOEXEC set, which would close the
                    // descriptor on exec; clear the flag by hand instead.
                    nix::fcntl::fcntl(
                        child_end,
                        nix::fcntl::FcntlArg::F_SETFD(nix::fcntl::FdFlag::empty()),
                    )?;
                    continue;
                }
                // SAFETY: dup2_raw closes `target` if it is open and makes it the duplicate of
                // the child end; releasing the returned owner keeps it open across the exec.
                let installed = unsafe {
                    nix::unistd::dup2_raw(child_end, std::os::fd::OwnedFd::from_raw_fd(*target))?
                };
                let _ = installed.into_raw_fd();
            }
            Ok(())
        };
        // SAFETY: the closure only calls async-signal-safe syscalls, as required between fork and
        // exec. It owns the child-side fds, so they stay alive until the exec happens.
        unsafe {
            cmd.pre_exec(install_extra_fds);
        }
    }

    let mut child = cmd.spawn()?;
    // Closing the parent's copies of the child-side ends is what lets the child see EOF.
    drop(cmd);

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

/// PHP `proc_terminate`. Sends `signal` to the process behind the resource; returns PHP's `false`
/// when the handle is already closed or the signal cannot be delivered.
pub fn proc_terminate(process: &PhpResource, signal: i64) -> bool {
    let PhpResource::Process(state) = process else {
        return false;
    };
    let state = state.borrow();
    let Some(child) = state.child.as_ref() else {
        return false;
    };
    send_signal(child.id() as i32, signal)
}

/// Shared body of `proc_terminate` and `posix_kill`. Signal 0 is PHP's existence probe and is
/// forwarded to `kill(2)` as such.
fn send_signal(pid: i32, signal: i64) -> bool {
    let signal = if signal == 0 {
        None
    } else {
        match nix::sys::signal::Signal::try_from(signal as i32) {
            Ok(signal) => Some(signal),
            Err(_) => return false,
        }
    };
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), signal).is_ok()
}

pub fn getmypid() -> i64 {
    std::process::id() as i64
}

pub fn cli_set_process_title(_title: &str) -> bool {
    // TODO(phase-c): PHP rewrites the argv area so the new title shows up in ps(1)'s full command
    // line. Rust hands out argv as owned copies, so the original block is not reachable; prctl's
    // PR_SET_NAME only replaces the 16-byte comm field and would report a different title.
    todo!()
}

pub fn setproctitle(_title: &str) {
    // TODO(phase-c): see cli_set_process_title; requires access to the process's own argv block.
    todo!()
}

// No-op until real signal handling is wired up; signal registration itself is
// deferred (see the TODO(plugin) notes in SignalRegistry::register).
pub fn pcntl_async_signals(_enable: bool) {}

pub fn pcntl_signal(_signal: i64, _handler: PhpMixed) -> bool {
    // TODO(phase-c): registering a signal handler requires the signal-handling subsystem to be
    // wired up (cf. SignalRegistry / the TODO(plugin) notes). sigaction(2) itself is reachable, but
    // the handler is a PHP callable whose dispatch depends on the runtime callable mechanism.
    todo!()
}

pub fn pcntl_signal_get_handler(_signal: i64) -> PhpMixed {
    // TODO(phase-c): see pcntl_signal; needs the signal-handling subsystem.
    todo!()
}

pub fn posix_getuid() -> i64 {
    nix::unistd::getuid().as_raw() as i64
}

pub fn posix_geteuid() -> i64 {
    nix::unistd::geteuid().as_raw() as i64
}

/// Looks up the passwd entry for `uid` and returns it in the shape of PHP's `posix_getpwuid`
/// associative array, or `false` when no entry matches.
pub fn posix_getpwuid(uid: i64) -> PhpMixed {
    let user = match nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(uid as u32)) {
        Ok(Some(user)) => user,
        _ => return PhpMixed::Bool(false),
    };
    let mut entry = indexmap::IndexMap::new();
    entry.insert("name".to_string(), PhpMixed::String(user.name));
    entry.insert(
        "passwd".to_string(),
        PhpMixed::String(user.passwd.to_string_lossy().into_owned()),
    );
    entry.insert("uid".to_string(), PhpMixed::Int(user.uid.as_raw() as i64));
    entry.insert("gid".to_string(), PhpMixed::Int(user.gid.as_raw() as i64));
    entry.insert(
        "gecos".to_string(),
        PhpMixed::String(user.gecos.to_string_lossy().into_owned()),
    );
    entry.insert(
        "dir".to_string(),
        PhpMixed::String(user.dir.to_string_lossy().into_owned()),
    );
    entry.insert(
        "shell".to_string(),
        PhpMixed::String(user.shell.to_string_lossy().into_owned()),
    );
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

pub fn posix_kill(pid: i64, signal: i64) -> bool {
    send_signal(pid as i32, signal)
}

/// PHP `get_current_user()`: the name of the owner of the running script file. The Shirabe
/// executable takes the place of the script; PHP returns an empty string when the lookup fails.
pub fn get_current_user() -> String {
    use std::os::unix::fs::MetadataExt as _;
    let Ok(executable) = std::env::current_exe() else {
        return String::new();
    };
    let Ok(metadata) = std::fs::metadata(&executable) else {
        return String::new();
    };
    match nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(metadata.uid())) {
        Ok(Some(user)) => user.name,
        _ => String::new(),
    }
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
    fn proc_open_wires_a_pipe_the_child_writes_to_beyond_stderr() {
        let mut pipes = IndexMap::new();
        let process = proc_open(
            "echo beyond >&3",
            &[
                Descriptor::Inherit,
                Descriptor::Inherit,
                Descriptor::Inherit,
                Descriptor::Pipe("w".to_string()),
            ],
            &mut pipes,
            None,
            None,
            None,
        )
        .unwrap();

        assert_eq!(
            stream_get_contents(pipes.get(&3).unwrap()).unwrap(),
            "beyond\n"
        );
        assert_eq!(proc_close(&process), 0);
    }

    #[test]
    fn proc_open_wires_a_pipe_the_child_reads_from_beyond_stderr() {
        let mut pipes = IndexMap::new();
        let process = proc_open(
            "cat <&4",
            &[
                Descriptor::Inherit,
                Descriptor::Pipe("w".to_string()),
                Descriptor::Inherit,
                Descriptor::Inherit,
                Descriptor::Pipe("r".to_string()),
            ],
            &mut pipes,
            None,
            None,
            None,
        )
        .unwrap();

        fwrite(pipes.get(&4).unwrap(), "fed\n", None);
        fclose(pipes.get(&4).unwrap());
        // Dropping the last handle closes the fd so `cat` sees end-of-input.
        pipes.shift_remove(&4);

        assert_eq!(
            stream_get_contents(pipes.get(&1).unwrap()).unwrap(),
            "fed\n"
        );
        assert_eq!(proc_close(&process), 0);
    }

    #[test]
    fn proc_terminate_signals_the_child() {
        let mut pipes = IndexMap::new();
        let process = proc_open(
            "sleep 30",
            &[
                Descriptor::Inherit,
                Descriptor::Inherit,
                Descriptor::Inherit,
            ],
            &mut pipes,
            None,
            None,
            None,
        )
        .unwrap();

        assert!(proc_terminate(&process, SIGTERM));
        // A child killed by a signal has no exit code, which proc_close reports as -1.
        assert_eq!(proc_close(&process), -1);
        let status = proc_get_status(&process);
        assert_eq!(status.get("pid").unwrap().as_int(), Some(-1));
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
