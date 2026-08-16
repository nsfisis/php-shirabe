//! Subscription to the signals that abort a Shirabe run: SIGINT, SIGTERM and SIGHUP. Plugins and
//! scripts cannot subscribe; `docs/dev/signals.md` says why.
//!
//! Composer reacts to these signals from `Seld\Signal\SignalHandler`'s pcntl callbacks, which the
//! interpreter runs inside whatever the process happens to be doing. A Rust signal handler may only
//! touch atomics, so the two halves are split here: the installed handler records the signal number,
//! the current subscription depth and a counter and then returns, and `is_triggered` reads those
//! atomics at checkpoints, so that the abort code runs on the normal call stack where it can borrow
//! whatever it needs. Checkpoints sit at the head of each loop that advances the work and on the
//! paths that handle an error, which is where an interrupted read surfaces.
//!
//! Subscriptions nest, and a signal belongs to the innermost one that was live when it arrived, and
//! to that one only. The depth recorded by the handler is what selects it; the counter is what
//! keeps a later subscription at the same depth from inheriting an older signal. Aborting from a
//! nested subscription therefore skips the clean-up of the ones around it — a `Ctrl+C` during a
//! child process spawned by `require` does not restore `composer.json`. `SignalHandler` dispatches
//! the same way, from one global stack to the most recently created handler, and undoing more than
//! Composer does would be an incompatibility of its own.
//!
//! Termination goes through `exit_with_last_signal`, which restores the default disposition and
//! re-raises the signal so the parent shell sees a signalled child rather than an ordinary exit.

const HANDLED_SIGNALS: [nix::sys::signal::Signal; 3] = [
    nix::sys::signal::Signal::SIGINT,
    nix::sys::signal::Signal::SIGTERM,
    nix::sys::signal::Signal::SIGHUP,
];

/// Number of live `SignalSubscription`s.
static DEPTH: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// Incremented once per delivered signal, so that a subscription can tell a signal that arrived
/// during its own lifetime from one that arrived before it was created.
static SIGNAL_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// `DEPTH` as of the last delivered signal.
static SIGNALED_DEPTH: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static LAST_SIGNAL: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
static SAVED_SIGACTIONS: std::sync::Mutex<
    Vec<(nix::sys::signal::Signal, nix::sys::signal::SigAction)>,
> = std::sync::Mutex::new(Vec::new());

extern "C" fn handle_signal(signal: nix::libc::c_int) {
    LAST_SIGNAL.store(signal, std::sync::atomic::Ordering::SeqCst);
    SIGNALED_DEPTH.store(
        DEPTH.load(std::sync::atomic::Ordering::SeqCst),
        std::sync::atomic::Ordering::SeqCst,
    );
    SIGNAL_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
}

/// Subscribes to SIGINT, SIGTERM and SIGHUP for as long as it is alive. Outside every subscription
/// the signals keep their default disposition and terminate the process outright.
#[derive(Debug)]
pub struct SignalSubscription {
    depth: usize,
    seq: u64,
}

impl Default for SignalSubscription {
    fn default() -> Self {
        Self::new()
    }
}

impl SignalSubscription {
    // TODO(windows): Windows delivers console control events (CTRL_C_EVENT, CTRL_BREAK_EVENT)
    // rather than signals, and they are not subscribed to here.
    pub fn new() -> Self {
        let seq = SIGNAL_SEQ.load(std::sync::atomic::Ordering::SeqCst);
        let mut saved_sigactions = SAVED_SIGACTIONS.lock().unwrap();
        let depth = DEPTH.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        if depth == 1 {
            // Without SA_RESTART a signal interrupts a blocking read instead of resuming it, which
            // is what lets an interactive prompt inside a subscription reach the next checkpoint.
            let action = nix::sys::signal::SigAction::new(
                nix::sys::signal::SigHandler::Handler(handle_signal),
                nix::sys::signal::SaFlags::empty(),
                nix::sys::signal::SigSet::empty(),
            );
            for signal in HANDLED_SIGNALS {
                let previous = unsafe { nix::sys::signal::sigaction(signal, &action) }
                    .expect("failed to install a signal handler");
                saved_sigactions.push((signal, previous));
            }
        }
        Self { depth, seq }
    }

    /// Whether a signal arrived while this subscription was the innermost one.
    pub fn is_triggered(&self) -> bool {
        SIGNAL_SEQ.load(std::sync::atomic::Ordering::SeqCst) > self.seq
            && SIGNALED_DEPTH.load(std::sync::atomic::Ordering::SeqCst) == self.depth
    }

    pub fn last_signal(&self) -> nix::sys::signal::Signal {
        nix::sys::signal::Signal::try_from(LAST_SIGNAL.load(std::sync::atomic::Ordering::SeqCst))
            .expect("no signal has been delivered")
    }

    /// Terminates the process the way the signal would have, so that the parent shell sees a
    /// signalled child rather than an ordinary exit.
    pub fn exit_with_last_signal(&self) -> ! {
        let signal = self.last_signal();
        let default = nix::sys::signal::SigAction::new(
            nix::sys::signal::SigHandler::SigDfl,
            nix::sys::signal::SaFlags::empty(),
            nix::sys::signal::SigSet::empty(),
        );
        unsafe { nix::sys::signal::sigaction(signal, &default) }
            .expect("failed to restore a signal handler");
        let _ = nix::sys::signal::raise(signal);
        // usually the above raise() kills the process
        // not strictly correct but it's the best we can do here
        std::process::exit(128 + signal as i32);
    }
}

impl Drop for SignalSubscription {
    fn drop(&mut self) {
        let mut saved_sigactions = SAVED_SIGACTIONS.lock().unwrap();
        let depth = DEPTH.fetch_sub(1, std::sync::atomic::Ordering::SeqCst) - 1;
        if depth == 0 {
            for (signal, action) in saved_sigactions.drain(..) {
                unsafe { nix::sys::signal::sigaction(signal, &action) }
                    .expect("failed to restore a signal handler");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SignalSubscription;

    fn raise_sigint() {
        nix::sys::signal::raise(nix::sys::signal::Signal::SIGINT).unwrap();
    }

    /// Runs this test binary again with `SIGNAL_TEST_CHILD` set, so that the signal may kill the
    /// child instead of the test runner.
    fn rerun_self(test_name: &str) -> std::process::ExitStatus {
        std::process::Command::new(std::env::current_exe().unwrap())
            .arg(test_name)
            .arg("--exact")
            .env("SIGNAL_TEST_CHILD", "1")
            .status()
            .unwrap()
    }

    fn terminating_signal(status: &std::process::ExitStatus) -> Option<i32> {
        std::os::unix::process::ExitStatusExt::signal(status)
    }

    #[test]
    fn a_signal_belongs_to_the_innermost_subscription() {
        let outer = SignalSubscription::new();
        {
            let inner = SignalSubscription::new();
            assert!(!inner.is_triggered());
            raise_sigint();
            assert!(inner.is_triggered());
            assert_eq!(inner.last_signal(), nix::sys::signal::Signal::SIGINT);
            assert!(!outer.is_triggered());
        }
        {
            // The signal above belongs to a subscription that is gone, not to this one.
            assert!(!SignalSubscription::new().is_triggered());
        }
        assert!(!outer.is_triggered());
        raise_sigint();
        assert!(outer.is_triggered());
    }

    #[test]
    fn exit_with_last_signal_kills_by_the_signal() {
        if shirabe_php_shim::getenv("SIGNAL_TEST_CHILD").is_some() {
            let signals = SignalSubscription::new();
            raise_sigint();
            signals.exit_with_last_signal();
        }

        let status = rerun_self("signal::tests::exit_with_last_signal_kills_by_the_signal");
        assert_eq!(status.code(), None);
        assert_eq!(
            terminating_signal(&status),
            Some(nix::sys::signal::Signal::SIGINT as i32)
        );
    }

    #[test]
    fn the_default_disposition_returns_once_no_subscription_is_left() {
        if shirabe_php_shim::getenv("SIGNAL_TEST_CHILD").is_some() {
            drop(SignalSubscription::new());
            raise_sigint();
            unreachable!("SIGINT must terminate the process");
        }

        let status = rerun_self(
            "signal::tests::the_default_disposition_returns_once_no_subscription_is_left",
        );
        assert_eq!(
            terminating_signal(&status),
            Some(nix::sys::signal::Signal::SIGINT as i32)
        );
    }
}
