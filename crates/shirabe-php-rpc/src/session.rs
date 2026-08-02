//! Thread-ID based reentrant session lock.
//!
//! Guarantees at most one logical RPC "call session" is in flight against the shared worker at
//! any time, while allowing the owning thread to recurse into it freely (a handler may itself
//! call back into the other side). A different thread attempting to start a session blocks until
//! the entire outer session (including all of its nested calls) completes — it is never rejected
//! or panicked on, only serialized. This keeps the "exactly one child process" invariant intact
//! even when multiple OS threads use this crate concurrently, which happens today only under
//! `cargo test`'s parallel test harness but is not assumed to be forbidden in the future.

use std::sync::{Condvar, LazyLock, Mutex};

struct SessionLock {
    owner: Mutex<Option<(std::thread::ThreadId, u32)>>,
    cvar: Condvar,
}

impl SessionLock {
    fn acquire(&self) {
        let mut owner = self.owner.lock().unwrap();
        let me = std::thread::current().id();
        loop {
            match *owner {
                None => {
                    *owner = Some((me, 1));
                    return;
                }
                Some((tid, depth)) if tid == me => {
                    *owner = Some((me, depth + 1));
                    return;
                }
                Some(_) => {
                    owner = self.cvar.wait(owner).unwrap();
                }
            }
        }
    }

    fn release(&self) {
        let mut owner = self.owner.lock().unwrap();
        let me = std::thread::current().id();
        match *owner {
            Some((tid, depth)) if tid == me => {
                if depth == 1 {
                    *owner = None;
                    self.cvar.notify_all();
                } else {
                    *owner = Some((me, depth - 1));
                }
            }
            _ => unreachable!("SessionLock::release without a matching acquire on this thread"),
        }
    }
}

static SESSION: LazyLock<SessionLock> = LazyLock::new(|| SessionLock {
    owner: Mutex::new(None),
    cvar: Condvar::new(),
});

/// RAII guard; acquired once at the outermost `rpc_call`, re-entered (depth += 1, no blocking)
/// by nested calls from the same thread.
pub struct SessionGuard;

impl SessionGuard {
    pub fn enter() -> Self {
        SESSION.acquire();
        SessionGuard
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        SESSION.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_thread_reenters_without_blocking() {
        let _outer = SessionGuard::enter();
        let _inner = SessionGuard::enter();
    }

    #[test]
    fn other_threads_are_serialized() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicU32, Ordering};

        let concurrent = Arc::new(AtomicU32::new(0));
        let mut handles = Vec::new();
        for _ in 0..4 {
            let concurrent = Arc::clone(&concurrent);
            handles.push(std::thread::spawn(move || {
                for _ in 0..50 {
                    let _guard = SessionGuard::enter();
                    let now = concurrent.fetch_add(1, Ordering::SeqCst);
                    assert_eq!(now, 0, "two sessions were in flight at once");
                    concurrent.fetch_sub(1, Ordering::SeqCst);
                }
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
    }
}
