use std::io::Write as _;
use std::os::unix::process::ExitStatusExt as _;

/// How the PHP process ends when [`exit`] unwinds out of it.
#[derive(Debug)]
pub enum Exit {
    /// `exit($status)` with an integer status.
    Code(i32),
    /// Killed by the signal.
    Signal(i32),
}

impl Exit {
    /// Ends the current running process.
    pub fn terminate(self) -> ! {
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();

        match self {
            Exit::Code(code) => std::process::exit(code),
            Exit::Signal(signal) => {
                if let Ok(signal) = nix::sys::signal::Signal::try_from(signal) {
                    let default = nix::sys::signal::SigAction::new(
                        nix::sys::signal::SigHandler::SigDfl,
                        nix::sys::signal::SaFlags::empty(),
                        nix::sys::signal::SigSet::empty(),
                    );
                    // SAFETY: SIG_DFL installs no handler code, and nothing runs after the raise
                    // below that could rely on the disposition this replaces.
                    let _ = unsafe { nix::sys::signal::sigaction(signal, &default) };
                    let _ = nix::sys::signal::raise(signal);
                }
                std::process::exit(128 + signal)
            }
        }
    }
}

impl From<i32> for Exit {
    fn from(code: i32) -> Self {
        Exit::Code(code)
    }
}

impl From<std::process::ExitStatus> for Exit {
    /// The end of a child process that ended with `status`, to end this process the same way.
    fn from(status: std::process::ExitStatus) -> Self {
        match status.code() {
            Some(code) => code.into(),
            None => Exit::Signal(
                status
                    .signal()
                    .expect("a process must end either with an exit code or by a signal"),
            ),
        }
    }
}

/// PHP's `exit` language construct. PHP implements `exit` as an unwind that no `catch` can catch
/// and that skips `finally` blocks, and runs destructors once the stack is gone. This unwinds the
/// stack with the [`Exit`] as the payload, so no `Result`-based port of a `catch` can intercept it,
/// ports of `finally` written as code after the call do not run, and `Drop` does. `main` catches
/// it and ends the process through [`Exit::terminate`].
pub fn exit(exit: impl Into<Exit>) -> ! {
    std::panic::resume_unwind(Box::new(exit.into()))
}
