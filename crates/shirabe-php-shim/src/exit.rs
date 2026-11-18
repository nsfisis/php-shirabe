use std::io::Write as _;

/// How the PHP process ends when [`exit`] unwinds out of it.
#[derive(Debug)]
pub enum Exit {
    /// `exit($status)` with an integer status.
    Code(i32),
}

impl Exit {
    /// Ends the current running process.
    pub fn terminate(self) -> ! {
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();

        match self {
            Exit::Code(code) => std::process::exit(code),
        }
    }
}

impl From<i32> for Exit {
    fn from(code: i32) -> Self {
        Exit::Code(code)
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
