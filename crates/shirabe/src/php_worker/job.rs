//! The Rust half of the promise registry at the plugin boundary, and the jobs behind the promises
//! the Rust side owns.
//!
//! This module has no Composer counterpart. A promise is held by the worker while one side runs
//! what settles it; only its id crosses the RPC channel. A Rust-owned promise is backed by a job,
//! a Rust future counted against a Composer service. See `docs/dev/plugin-async.md`.

pub(crate) mod rpc;

use crate::php_worker::call::PhpCall;
use crate::php_worker::throwable::throw_into_error;
use crate::util::sync_executor::block_on;
use crate::util::{HttpDownloader, ProcessExecutor};
use bstr::BStr;
use indexmap::IndexMap;
use shirabe_php_rpc::{PhpThrow, RpcValue};
use shirabe_php_shim::{bformat, error_chain_message};

/// The service a job is counted against: the one whose `countActiveJobs()` reports it and whose
/// tick drives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct JobOwner(usize);

impl JobOwner {
    pub(crate) fn http_downloader(
        downloader: &std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
    ) -> Self {
        Self(downloader.as_ptr() as usize)
    }

    /// The same key as [`JobOwner::http_downloader`], for code that only has the downloader the
    /// shared cell holds.
    pub(crate) fn of_http_downloader(downloader: &HttpDownloader) -> Self {
        Self(downloader as *const HttpDownloader as usize)
    }

    pub(crate) fn process_executor(
        process: &std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    ) -> Self {
        Self(process.as_ptr() as usize)
    }

    /// The same key as [`JobOwner::process_executor`], for code that only has the executor the
    /// shared cell holds.
    pub(crate) fn of_process_executor(process: &ProcessExecutor) -> Self {
        Self(process as *const ProcessExecutor as usize)
    }
}

/// The job behind a Rust-owned promise: the Rust side runs it, and the worker holds the promise.
struct RustJob {
    owner: JobOwner,
    /// `None` while a tick has taken the future out to poll it. Such a job is on the Rust call
    /// stack rather than waiting in the table, so nothing counts it and an inner tick skips it.
    future:
        Option<std::pin::Pin<Box<dyn std::future::Future<Output = Result<RpcValue, PhpThrow>>>>>,
    /// Whether the job got past the job budget, which is when Composer marks it started.
    started: std::rc::Rc<std::cell::Cell<bool>>,
    /// What the canceller of the job's promise throws when the job had started.
    canceled: PhpThrow,
    /// Keeps the owner's address from being reused for another service while the job is counted
    /// against it.
    _owner_entity: std::rc::Rc<dyn std::any::Any>,
}

/// A PHP-owned promise: one plugin code handed to the Rust side, settled by the worker.
enum PhpPromise {
    Pending(Option<std::task::Waker>),
    Settled(Result<RpcValue, PhpThrow>),
}

#[derive(Default)]
struct Registry {
    rust_jobs: IndexMap<u64, RustJob>,
    php_promises: IndexMap<u64, PhpPromise>,
    /// The PHP-owned promises a Rust future is parked on, in the order they parked.
    php_waiting: Vec<u64>,
    /// Requests and processes the Rust side started for itself. Their futures belong to whoever
    /// awaits them; the entry lets `Loop::wait()` count them and `abortJobs()` reach the ones that
    /// carry an abort handle.
    internal_jobs: IndexMap<u64, (JobOwner, Option<futures::future::AbortHandle>)>,
    next_internal_id: u64,
    /// Bumped by every pass over the job table, so a pass can tell that another one ran inside
    /// it and re-registered the wakers it had left behind.
    passes: u64,
}

thread_local! {
    /// One per thread, like the Rust entity table whose entities the jobs are counted against.
    static REGISTRY: std::cell::RefCell<Registry> = std::cell::RefCell::new(Registry::default());
}

/// The Rust side allocates odd promise ids, the worker even ones.
static NEXT_PROMISE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Registers a job without polling it and answers with the handle of the Rust-owned promise the
/// worker builds for it.
pub(crate) fn register(
    owner: JobOwner,
    owner_entity: std::rc::Rc<dyn std::any::Any>,
    started: std::rc::Rc<std::cell::Cell<bool>>,
    canceled: PhpThrow,
    future: std::pin::Pin<Box<dyn std::future::Future<Output = Result<RpcValue, PhpThrow>>>>,
) -> RpcValue {
    let id = NEXT_PROMISE_ID.fetch_add(2, std::sync::atomic::Ordering::Relaxed);
    REGISTRY.with(|registry| {
        registry.borrow_mut().rust_jobs.insert(
            id,
            RustJob {
                owner,
                future: Some(future),
                started,
                canceled,
                _owner_entity: owner_entity,
            },
        );
    });
    RpcValue::PromiseHandle(id)
}

/// How many jobs wait in the table for `owner`.
pub(crate) fn count(owner: JobOwner) -> i64 {
    REGISTRY.with(|registry| {
        registry
            .borrow()
            .rust_jobs
            .values()
            .filter(|job| job.owner == owner && job.future.is_some())
            .count() as i64
    })
}

/// How many jobs are active on `owner`, counting the ones the Rust side started for itself along
/// with the ones plugin code started: what Composer's `countActiveJobs()` reports to `Loop::wait()`.
pub(crate) fn count_active_jobs(owner: JobOwner) -> i64 {
    let internal = REGISTRY.with(|registry| {
        registry
            .borrow()
            .internal_jobs
            .values()
            .filter(|(job_owner, _)| *job_owner == owner)
            .count() as i64
    });
    count(owner) + internal
}

fn has_jobs(owners: &[JobOwner]) -> bool {
    REGISTRY.with(|registry| {
        registry
            .borrow()
            .rust_jobs
            .values()
            .any(|job| owners.contains(&job.owner) && job.future.is_some())
    })
}

fn begin_pass() -> u64 {
    REGISTRY.with(|registry| {
        let mut registry = registry.borrow_mut();
        registry.passes += 1;
        registry.passes
    })
}

fn another_pass_ran_since(pass: u64) -> bool {
    REGISTRY.with(|registry| registry.borrow().passes != pass)
}

/// Polls every job counted against `owners` once and pushes the outcome of each one that
/// finished to the worker before polling the next. Returns whether any promise settled.
fn poll_jobs(owners: &[JobOwner], cx: &mut std::task::Context<'_>) -> Result<bool, PhpThrow> {
    let ids: Vec<u64> = REGISTRY.with(|registry| {
        registry
            .borrow()
            .rust_jobs
            .iter()
            .filter(|(_, job)| owners.contains(&job.owner))
            .map(|(id, _)| *id)
            .collect()
    });
    let mut settled = false;
    for id in ids {
        let future = REGISTRY.with(|registry| {
            registry
                .borrow_mut()
                .rust_jobs
                .get_mut(&id)
                .and_then(|job| job.future.take())
        });
        let Some(mut future) = future else {
            continue;
        };
        match future.as_mut().poll(cx) {
            std::task::Poll::Pending => {
                // The slot is gone when the job's own future ran plugin code that cancelled it.
                REGISTRY.with(|registry| {
                    if let Some(job) = registry.borrow_mut().rust_jobs.get_mut(&id) {
                        job.future = Some(future);
                    }
                });
            }
            std::task::Poll::Ready(outcome) => {
                let known =
                    REGISTRY.with(|registry| registry.borrow_mut().rust_jobs.shift_remove(&id));
                if known.is_some() {
                    push_settle(id, outcome)?;
                    settled = true;
                }
            }
        }
    }
    Ok(settled)
}

/// Reports a finished job to the worker, which settles the `Deferred` of its promise and so runs the
/// callbacks plugin code chained on it before this returns.
fn push_settle(id: u64, outcome: Result<RpcValue, PhpThrow>) -> Result<(), PhpThrow> {
    let (function, args) = match outcome {
        Ok(value) => (
            "__shirabe_resolve_promise",
            vec![RpcValue::Int(id as i64), value],
        ),
        Err(throw) => (
            "__shirabe_reject_promise",
            vec![RpcValue::Int(id as i64), throw.to_rpc_value()],
        ),
    };
    match PhpCall::Function(BStr::new(function)).try_run::<RpcValue>(args) {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(throw)) => Err(throw),
        Err(error) => Err(PhpThrow::runtime(bformat!(
            "settling promise {id} in the plugin process failed: {}",
            error_chain_message(&error)
        ))),
    }
}

/// One tick of `owner`: advance its jobs, settle the ones that finished, and report how many
/// remain. With no job settling it blocks until one of them makes progress or `cap` elapses, looks
/// once more and returns, the way `curl_multi_select()` bounds a tick of Composer's. `index` asks
/// about that one job instead of the whole table, as the `$index` of `countActiveJobs()` does.
pub(crate) fn tick(
    owner: JobOwner,
    cap: std::time::Duration,
    index: Option<u64>,
) -> Result<i64, PhpThrow> {
    if count(owner) > 0 {
        let pass = begin_pass();
        let mut failure = None;
        let mut woken = false;
        block_on(async {
            let advanced = std::future::poll_fn(|cx| match poll_jobs(&[owner], cx) {
                Err(throw) => {
                    failure = Some(throw);
                    std::task::Poll::Ready(())
                }
                // A pass that ran inside this one left its own waker on the jobs it polled, so
                // waiting on them here could outlast their progress.
                Ok(settled)
                    if settled || woken || count(owner) == 0 || another_pass_ran_since(pass) =>
                {
                    std::task::Poll::Ready(())
                }
                Ok(_) => {
                    woken = true;
                    std::task::Poll::Pending
                }
            });
            let _ = tokio::time::timeout(cap, advanced).await;
        });
        if let Some(throw) = failure {
            return Err(throw);
        }
    }
    Ok(match index {
        Some(id) => REGISTRY.with(|registry| registry.borrow().rust_jobs.contains_key(&id)) as i64,
        None => count(owner),
    })
}

/// Serves the canceller of a Rust-owned promise: drops its job, and answers with what the canceller
/// throws. Composer's cancellers throw only for a job that had started and abort a queued one
/// silently.
pub(crate) fn cancel(id: u64) -> Option<PhpThrow> {
    let job = REGISTRY.with(|registry| registry.borrow_mut().rust_jobs.shift_remove(&id))?;
    job.started.get().then_some(job.canceled)
}

/// Records a request or a process the Rust side started for itself. The entry lives as long as
/// the returned guard, which the job's future holds until it settles. Only an entry with an abort
/// handle is cancelled by `abortJobs()`.
pub(crate) fn record_internal_job(
    owner: JobOwner,
    abort_handle: Option<futures::future::AbortHandle>,
) -> InternalJobGuard {
    REGISTRY.with(|registry| {
        let mut registry = registry.borrow_mut();
        let id = registry.next_internal_id;
        registry.next_internal_id += 1;
        registry.internal_jobs.insert(id, (owner, abort_handle));
        InternalJobGuard(id)
    })
}

#[derive(Debug)]
pub(crate) struct InternalJobGuard(u64);

impl Drop for InternalJobGuard {
    fn drop(&mut self) {
        REGISTRY.with(|registry| {
            registry.borrow_mut().internal_jobs.shift_remove(&self.0);
        });
    }
}

/// Cancels the requests the Rust side is awaiting on `owners`, or on every service when `owners`
/// is `None`.
pub(crate) fn abort_internal_jobs(owners: Option<&[JobOwner]>) {
    let handles: Vec<futures::future::AbortHandle> = REGISTRY.with(|registry| {
        registry
            .borrow()
            .internal_jobs
            .values()
            .filter(|(owner, _)| owners.is_none_or(|owners| owners.contains(owner)))
            .filter_map(|(_, handle)| handle.clone())
            .collect()
    });
    for handle in handles {
        handle.abort();
    }
}

/// Records the outcome the worker reports for a PHP-owned promise and wakes the future parked on it.
/// The report of a promise that had already settled when it crossed arrives before the Rust side
/// has decoded the promise's handle, so an unknown id is recorded too.
pub(crate) fn settle_php_promise(id: u64, outcome: Result<RpcValue, PhpThrow>) {
    let waker = REGISTRY.with(|registry| {
        match registry
            .borrow_mut()
            .php_promises
            .insert(id, PhpPromise::Settled(outcome))
        {
            Some(PhpPromise::Pending(waker)) => waker,
            Some(PhpPromise::Settled(_)) | None => None,
        }
    });
    if let Some(waker) = waker {
        waker.wake();
    }
}

/// Awaits a PHP-owned promise. A promise plugin code built settles only while PHP code runs, and
/// what gets PHP code to run from here is the settle push of a Rust-owned promise, so every poll
/// ticks the whole job table.
pub(crate) async fn await_php_promise(id: u64) -> anyhow::Result<RpcValue> {
    let parked = ParkedOnPhpPromise(id);
    let outcome = std::future::poll_fn(|cx| {
        if let Some(outcome) = take_php_promise_outcome(id) {
            return std::task::Poll::Ready(outcome);
        }
        let owners: Vec<JobOwner> = REGISTRY.with(|registry| {
            registry
                .borrow()
                .rust_jobs
                .values()
                .map(|job| job.owner)
                .collect()
        });
        if !owners.is_empty() {
            match poll_jobs(&owners, cx) {
                Err(throw) => return std::task::Poll::Ready(Err(throw)),
                // The callbacks a settle ran may have started the next job of the chain.
                Ok(true) => cx.waker().wake_by_ref(),
                Ok(false) => {}
            }
            if let Some(outcome) = take_php_promise_outcome(id) {
                return std::task::Poll::Ready(outcome);
            }
        }
        REGISTRY.with(|registry| {
            let mut registry = registry.borrow_mut();
            registry
                .php_promises
                .insert(id, PhpPromise::Pending(Some(cx.waker().clone())));
            if !registry.php_waiting.contains(&id) {
                registry.php_waiting.push(id);
            }
        });
        std::task::Poll::Pending
    })
    .await;
    drop(parked);
    outcome.map_err(throw_into_error)
}

fn take_php_promise_outcome(id: u64) -> Option<Result<RpcValue, PhpThrow>> {
    REGISTRY.with(|registry| {
        let mut registry = registry.borrow_mut();
        match registry.php_promises.get(&id) {
            Some(PhpPromise::Settled(_)) => match registry.php_promises.shift_remove(&id) {
                Some(PhpPromise::Settled(outcome)) => Some(outcome),
                _ => unreachable!("the entry was just seen settled"),
            },
            Some(PhpPromise::Pending(_)) | None => None,
        }
    })
}

/// Takes a future off the parked list when it stops waiting, whether it finished or was dropped.
struct ParkedOnPhpPromise(u64);

impl Drop for ParkedOnPhpPromise {
    fn drop(&mut self) {
        REGISTRY.with(|registry| {
            let mut registry = registry.borrow_mut();
            registry.php_waiting.retain(|id| *id != self.0);
            if matches!(
                registry.php_promises.get(&self.0),
                Some(PhpPromise::Pending(_))
            ) {
                registry.php_promises.shift_remove(&self.0);
            }
        });
    }
}

/// The PHP-owned promises a Rust future is parked on and the worker has not reported on yet.
pub(crate) fn parked_php_promises() -> Vec<u64> {
    REGISTRY.with(|registry| {
        let registry = registry.borrow();
        registry
            .php_waiting
            .iter()
            .filter(|id| matches!(registry.php_promises.get(*id), Some(PhpPromise::Pending(_))))
            .copied()
            .collect()
    })
}

/// Fails the awaits parked on `ids`: nothing is left that could run the PHP code their promises
/// settle in.
pub(crate) fn fail_php_promises(ids: &[u64]) {
    for id in ids {
        settle_php_promise(
            *id,
            Err(PhpThrow::runtime(bformat!(
                "plugin promise {id} can never settle: no job is left whose completion could run the code that resolves it"
            ))),
        );
    }
}

/// The part of `Loop::wait()` that concerns jobs plugin code started on the loop's services and
/// never waited on: polls them from inside the wait, as Composer's wait settles them. Returns
/// whether any are left; `Err` carries a failed settle push.
pub(crate) fn poll_jobs_in_wait(
    owners: &[JobOwner],
    cx: &mut std::task::Context<'_>,
) -> anyhow::Result<bool> {
    if !has_jobs(owners) {
        return Ok(false);
    }
    let pass = begin_pass();
    let settled = poll_jobs(owners, cx).map_err(throw_into_error)?;
    if settled || another_pass_ran_since(pass) {
        cx.waker().wake_by_ref();
    }
    Ok(has_jobs(owners))
}
