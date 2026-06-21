use crate::{PhpMixed, PhpResource};
use indexmap::IndexMap;

pub const SIGINT: i64 = 2;
pub const SIGTERM: i64 = 15;
pub const SIGUSR1: i64 = 10;
pub const SIGUSR2: i64 = 12;

pub fn exec(
    _command: &str,
    _output: Option<&mut Vec<String>>,
    _exit_code: Option<&mut i64>,
) -> Option<String> {
    todo!()
}

pub fn shell_exec(_command: &str) -> Option<String> {
    todo!()
}

pub fn system(_command: &str, _result_code: Option<&mut i64>) -> Option<String> {
    todo!()
}

pub fn escapeshellcmd(_command: &str) -> String {
    todo!()
}

pub fn escapeshellarg(_arg: &str) -> String {
    todo!()
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
    todo!()
}

pub fn proc_terminate(_process: &PhpMixed, _signal: i64) -> bool {
    todo!()
}

pub fn getmypid() -> i64 {
    std::process::id() as i64
}

pub fn cli_set_process_title(_title: &str) -> bool {
    todo!()
}

pub fn setproctitle(_title: &str) {
    todo!()
}

// No-op until real signal handling is wired up; signal registration itself is
// deferred (see the TODO(plugin) notes in SignalRegistry::register).
pub fn pcntl_async_signals(_enable: bool) {}

pub fn pcntl_signal(_signal: i64, _handler: PhpMixed) -> bool {
    todo!()
}

pub fn pcntl_signal_get_handler(_signal: i64) -> PhpMixed {
    todo!()
}

pub fn posix_getuid() -> i64 {
    todo!()
}

pub fn posix_geteuid() -> i64 {
    todo!()
}

pub fn posix_getpwuid(_uid: i64) -> PhpMixed {
    todo!()
}

pub fn posix_isatty(_stream: PhpResource) -> bool {
    todo!()
}

pub fn posix_kill(_pid: i64, _signal: i64) -> bool {
    todo!()
}

pub fn get_current_user() -> String {
    todo!()
}
