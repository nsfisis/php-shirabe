//! The Rust half of the job registry at the plugin boundary.
//!
//! This module has no Composer counterpart. A job is an asynchronous operation one side runs and
//! the other side holds a promise for; only its id crosses the RPC channel. See
//! `docs/dev/plugin-async.md`.

use crate::plugin::php_plugin_proxy::PluginRpcDispatcher;
use crate::plugin::throwable_bridge::throw_into_error;
use indexmap::IndexMap;
use shirabe_php_rpc::{PhpThrow, PluginValue, call_function_with_dispatcher};

/// The service a job is counted against: the one whose `countActiveJobs()` reports it and whose
/// tick drives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct JobOwner(usize);

impl JobOwner {
    pub(crate) fn http_downloader(
        downloader: &std::rc::Rc<std::cell::RefCell<crate::util::HttpDownloader>>,
    ) -> Self {
        Self(downloader.as_ptr() as usize)
    }

    /// The same key as [`JobOwner::http_downloader`], for code that only has the downloader the
    /// shared cell holds.
    pub(crate) fn of_http_downloader(downloader: &crate::util::HttpDownloader) -> Self {
        Self(downloader as *const crate::util::HttpDownloader as usize)
    }

    pub(crate) fn process_executor(
        process: &std::rc::Rc<std::cell::RefCell<crate::util::ProcessExecutor>>,
    ) -> Self {
        Self(process.as_ptr() as usize)
    }
}

/// A job the Rust side runs for a promise the worker holds.
struct RustJob {
    owner: JobOwner,
    /// `None` while a tick has taken the future out to poll it. Such a job is on the Rust call
    /// stack rather than waiting in the table, so nothing counts it and an inner tick skips it.
    future:
        Option<std::pin::Pin<Box<dyn std::future::Future<Output = Result<PluginValue, PhpThrow>>>>>,
    /// Whether the job got past the job budget, which is when Composer marks it started.
    started: std::rc::Rc<std::cell::Cell<bool>>,
    /// What the canceller of the job's promise throws when the job had started.
    canceled: PhpThrow,
    /// Keeps the owner's address from being reused for another service while the job is counted
    /// against it.
    _owner_entity: std::rc::Rc<dyn std::any::Any>,
}

/// A job the worker runs for a promise plugin code handed to the Rust side.
enum PhpJob {
    Pending(Option<std::task::Waker>),
    Settled(Result<PluginValue, PhpThrow>),
}

#[derive(Default)]
struct Registry {
    rust_jobs: IndexMap<u64, RustJob>,
    php_jobs: IndexMap<u64, PhpJob>,
    /// The PHP-owned jobs a Rust future is parked on, in the order they parked.
    php_waiting: Vec<u64>,
    /// Requests the Rust side started for itself. Their futures belong to whoever awaits them;
    /// the entry only lets `abortJobs()` reach them.
    internal_jobs: IndexMap<u64, (JobOwner, futures::future::AbortHandle)>,
    next_internal_id: u64,
    /// Bumped by every pass over the job table, so a pass can tell that another one ran inside
    /// it and re-registered the wakers it had left behind.
    passes: u64,
}

thread_local! {
    /// One per thread, like the R table whose entities the jobs are counted against.
    static REGISTRY: std::cell::RefCell<Registry> = std::cell::RefCell::new(Registry::default());
}

/// The Rust side allocates odd job ids, the worker even ones.
static NEXT_JOB_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Registers a job without polling it and answers with the descriptor the worker turns into the
/// job's promise.
pub(crate) fn register(
    owner: JobOwner,
    owner_entity: std::rc::Rc<dyn std::any::Any>,
    started: std::rc::Rc<std::cell::Cell<bool>>,
    canceled: PhpThrow,
    future: std::pin::Pin<Box<dyn std::future::Future<Output = Result<PluginValue, PhpThrow>>>>,
) -> PluginValue {
    let id = NEXT_JOB_ID.fetch_add(2, std::sync::atomic::Ordering::Relaxed);
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
    PluginValue::Job(id)
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
/// finished to the worker before polling the next. Returns whether any job settled.
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

/// Reports a finished job to the worker, which settles the job's `Deferred` and so runs the
/// callbacks plugin code chained on it before this returns.
fn push_settle(id: u64, outcome: Result<PluginValue, PhpThrow>) -> Result<(), PhpThrow> {
    let (function, args) = match outcome {
        Ok(value) => (
            "__shirabe_resolve_job",
            vec![PluginValue::Int(id as i64), value],
        ),
        Err(throw) => {
            let mut args = vec![PluginValue::Int(id as i64)];
            args.extend(throw_fields(throw));
            ("__shirabe_reject_job", args)
        }
    };
    match call_function_with_dispatcher(function, args, Some(&mut PluginRpcDispatcher::default())) {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(throw)) => Err(throw),
        Err(error) => Err(PhpThrow::runtime(format!(
            "settling job {id} in the plugin process failed: {error:#}"
        ))),
    }
}

/// The fields a `Throw` frame carries, as the argument list the worker rebuilds an exception
/// from: class, message, code, declared state, file and line.
pub(crate) fn throw_fields(throw: PhpThrow) -> Vec<PluginValue> {
    let properties = PluginValue::Array(
        throw
            .properties
            .iter()
            .map(|(name, value)| (name.clone().into_bytes(), value.clone()))
            .collect(),
    );
    vec![
        PluginValue::string(throw.exception_class),
        PluginValue::string(throw.message),
        PluginValue::Int(throw.code),
        properties,
        PluginValue::string(throw.file),
        PluginValue::Int(throw.line),
    ]
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
        crate::util::sync_executor::block_on(async {
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

/// Serves the canceller of a job's promise: drops the job, and answers with what the canceller
/// throws. Composer's cancellers throw only for a job that had started and abort a queued one
/// silently.
pub(crate) fn cancel(id: u64) -> Option<PhpThrow> {
    let job = REGISTRY.with(|registry| registry.borrow_mut().rust_jobs.shift_remove(&id))?;
    job.started.get().then_some(job.canceled)
}

/// Records a request the Rust side started for itself. The entry lives as long as the returned
/// guard, which the request's future holds until it settles.
pub(crate) fn record_internal_job(
    owner: JobOwner,
    abort_handle: futures::future::AbortHandle,
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
            .map(|(_, handle)| handle.clone())
            .collect()
    });
    for handle in handles {
        handle.abort();
    }
}

/// Records the outcome the worker reports for a PHP-owned job and wakes the future parked on it.
/// The report of a promise that had already settled when it crossed arrives before the Rust side
/// has decoded the job's descriptor, so an unknown id is recorded too.
pub(crate) fn settle_php_job(id: u64, outcome: Result<PluginValue, PhpThrow>) {
    let waker = REGISTRY.with(|registry| {
        match registry
            .borrow_mut()
            .php_jobs
            .insert(id, PhpJob::Settled(outcome))
        {
            Some(PhpJob::Pending(waker)) => waker,
            Some(PhpJob::Settled(_)) | None => None,
        }
    });
    if let Some(waker) = waker {
        waker.wake();
    }
}

/// Awaits a PHP-owned job. A promise plugin code built settles only while PHP code runs, and what
/// gets PHP code to run from here is the settle push of a Rust-owned job, so every poll ticks the
/// whole job table.
pub(crate) async fn await_php_job(id: u64) -> anyhow::Result<PluginValue> {
    let parked = ParkedOnPhpJob(id);
    let outcome = std::future::poll_fn(|cx| {
        if let Some(outcome) = take_php_job_outcome(id) {
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
            if let Some(outcome) = take_php_job_outcome(id) {
                return std::task::Poll::Ready(outcome);
            }
        }
        REGISTRY.with(|registry| {
            let mut registry = registry.borrow_mut();
            registry
                .php_jobs
                .insert(id, PhpJob::Pending(Some(cx.waker().clone())));
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

fn take_php_job_outcome(id: u64) -> Option<Result<PluginValue, PhpThrow>> {
    REGISTRY.with(|registry| {
        let mut registry = registry.borrow_mut();
        match registry.php_jobs.get(&id) {
            Some(PhpJob::Settled(_)) => match registry.php_jobs.shift_remove(&id) {
                Some(PhpJob::Settled(outcome)) => Some(outcome),
                _ => unreachable!("the entry was just seen settled"),
            },
            Some(PhpJob::Pending(_)) | None => None,
        }
    })
}

/// Takes a future off the parked list when it stops waiting, whether it finished or was dropped.
struct ParkedOnPhpJob(u64);

impl Drop for ParkedOnPhpJob {
    fn drop(&mut self) {
        REGISTRY.with(|registry| {
            let mut registry = registry.borrow_mut();
            registry.php_waiting.retain(|id| *id != self.0);
            if matches!(registry.php_jobs.get(&self.0), Some(PhpJob::Pending(_))) {
                registry.php_jobs.shift_remove(&self.0);
            }
        });
    }
}

/// The PHP-owned jobs a Rust future is parked on and the worker has not reported on yet.
pub(crate) fn parked_php_jobs() -> Vec<u64> {
    REGISTRY.with(|registry| {
        let registry = registry.borrow();
        registry
            .php_waiting
            .iter()
            .filter(|id| matches!(registry.php_jobs.get(*id), Some(PhpJob::Pending(_))))
            .copied()
            .collect()
    })
}

/// Fails the awaits parked on `ids`: nothing is left that could run the PHP code their promises
/// settle in.
pub(crate) fn fail_php_jobs(ids: &[u64]) {
    for id in ids {
        settle_php_job(
            *id,
            Err(PhpThrow::runtime(format!(
                "the promise of plugin job {id} can never settle: no job is left whose \
                 completion could run the code that resolves it"
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
