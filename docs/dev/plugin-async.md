# Async at the plugin boundary

Composer has three asynchronous classes: `Composer\Util\HttpDownloader`, `Composer\Util\ProcessExecutor` and `Composer\Util\Loop`. All three are reachable from plugin code, and the first two hand out `React\Promise\PromiseInterface`. The RPC channel ([docs/dev/php-rpc.md](./php-rpc.md)) is a synchronous request/response protocol, so a promise that has not settled yet has no obvious representation on it.

This document decides what the boundary does about that. It is a design decision first; which parts exist is recorded in the staging section at the end.

For promise states, this document uses the terms settle (to become either resolved or rejected), resolve, reject and pending (not settled yet).

## How Composer's async actually works

The decision rests on one property of Composer's own model, and it is not the property the word "promise" suggests. **Composer has no event loop.** Between the call that starts a job and the call that waits for it, nothing runs on the PHP side. A promise settles, and its `then` callbacks run, only inside the waiting call. `Loop::wait()` is a `while (true)` over `HttpDownloader::countActiveJobs()` and `ProcessExecutor::countActiveJobs()`, and those two methods *are* the scheduler. `countActiveJobs()` starts as many jobs as the job budget allows at that moment, ticks the transport, resolves whatever has finished, and reports how many remain.

* `HttpDownloader::countActiveJobs()` calls `CurlDownloader::tick()`, which is `curl_multi_exec()` plus `curl_multi_select()` plus a `curl_multi_info_read()` drain that resolves the promise of each finished job. `curl_multi_select()` blocks until a socket moves or `$selectTimeout` (5 seconds) elapses. The cap exists so that `Loop::wait()` can get back to `ProcessExecutor::countActiveJobs()` and to the progress bar update.
* `ProcessExecutor::countActiveJobs()` polls `$job['process']->isRunning()` for every started job and resolves the promise of each one that has exited. It does not block. `ProcessExecutor::wait()` puts a `usleep(1000)` between ticks, but the `while (true)` in `Loop::wait()` puts nothing there, so a `Loop::wait()` with only process jobs left is a hot loop.

A job is started inside `add()` / `executeAsync()` when the job budget has room, and inside a later `countActiveJobs()` when it does not. What "started" means differs between the two services.

* Starting an `HttpDownloader` job only builds a curl handle and registers it with `curl_multi_add_handle()`. The transfer itself advances only inside a tick, which is what calls `curl_multi_exec()`.
    * This holds for the requests curl takes. A request it does not take (a `file://` URL, a request with `allow_self_signed`, a PHP without the curl extension) is handed to `RemoteFilesystem` from the resolver of the promise itself. The blocking download runs while `add()` constructs the promise, so the promise `add()` returns has already settled, and the job never counts against the job budget.
* Starting a `ProcessExecutor` job is `proc_open()`. The child process is running before `executeAsync()` returns, and from then on it advances on the OS regardless of ticks. All a tick does is detect that it has exited. When constructing the `Process` or calling `start()` throws, the job is rejected inside `executeAsync()`, so the promise it returns is already rejected.

React settles synchronously, so a promise resolved inside a tick runs its `then` callbacks before that tick returns. Everything a plugin observes therefore happens inside a `countActiveJobs()` call (or, for the two cases above that settle on the spot, inside the call that started the job), and a promise is *pending only between `add()` and the next wait*.

One more property matters at the boundary. **`Loop::wait($promises)` does not wait for `$promises`; it waits until the services have no active jobs.** `all($promises)` is used only to pick up a rejection. So `wait([$a])` runs every job to completion, `$b` included, and a promise a plugin got from `add()` and returned without waiting on settles the next time anyone calls `wait()`.

This is what makes the boundary tractable. Shirabe needs no promise that crosses the wire unresolved, no event loop in the worker, and no way for Rust to wake the worker up. It needs one primitive.

> **tick**: advance the outstanding jobs, settle the ones that finished, and report how many remain.

## Decision

**Rust owns the jobs. PHP owns the promises. Only job ids cross the boundary.**

The promise machinery (construction, `then` chains, `React\Promise\all()`, rejection propagation, cancellation) runs entirely inside the worker, against the real react/promise the Composer runtime bundle already ships. A promise object is never an entity, takes neither an R handle nor a P handle, and does not cross the wire in either direction. What crosses is an integer naming a job in a job registry that both sides hold an entry for.

The worker-side entry is a `React\Promise\Deferred`: the react/promise class that pairs a promise with the `resolve()` / `reject()` that settle it from outside. It plays the same role as the `$job['resolve']` / `$job['reject']` Composer stores in its job table by hand. The worker holds one `Deferred` per Rust-owned job, hands its `promise()` to the plugin, and calls `resolve()` / `reject()` when Rust reports that the job has settled.

Six rules follow.

1. **A job is a Rust future paired with an id.** A `rust-proxy` method that answers with `PromiseInterface` builds the future, registers it in the Rust job registry *without polling it*, and answers with the job's descriptor. Nothing has run by the time the call returns. A registered future must not hold a borrow of the entity. Everything it needs to run is moved or cloned into the future when it is built (`ProcessExecutor::execute_async_job` and `HttpDownloader::add_job` have this shape).
    * An `HttpDownloader` request curl does not take is not a job at all. It is run inside `add()` / `addCopy()`, and the call answers with `__shirabe_resolved_promise` / `__shirabe_rejected_promise`, because the promise Composer returns for it has settled too.
    * Process jobs are the other exception: the future is polled exactly once right after it is built, before it is registered. Composer's `executeAsync()` calls `proc_open()` on the spot when the job budget has room, and this poll is what takes the future through acquiring the budget and starting the child process. With no room in the budget the poll stops waiting for it, which is Composer's `STATUS_QUEUED`. `Filesystem::removeDirectoryAsync()` is polled once the same way, since what it does before the process starts (the edge cases that need no process) settles inside the call in Composer.
    * This poll does not push a settle. When the start fails, no job is registered and the call answers with `__shirabe_rejected_promise`, because the promise Composer returns is already rejected too. When the poll observes the child all the way to its exit, the job is registered holding its result, and the result is pushed on the next tick, because Composer resolves only inside `countActiveJobs()`.
    * Each job carries a *started* flag, the counterpart of Composer's `STATUS_STARTED`, which is what the canceller asks about. A process job sets it when its future gets through the job budget. An `HttpDownloader` job is flagged when it is registered if the budget has room at that moment, although nothing of the request has run yet, because Composer starts such a job inside `add()`.
2. **A job settles only inside a tick.** Only a tick polls the futures of the jobs in the registry. A started child process is run by the OS and advances outside ticks as well, but its exit is detected, and its promise settled, only inside a tick. This is Composer's own rule, not a constraint the port adds.
3. **A tick blocks until something moves or a time cap is reached.** The Rust `countActiveJobs()` polls the outstanding jobs counted against that entity one by one; if none settles, it waits until a waker fires or the cap is reached, polls them once more, and returns the remaining count. The cap matches Composer's wait granularity: the 5 seconds of `curl_multi_select()` for `HttpDownloader`, the 1 millisecond `wait()` sleeps for `ProcessExecutor`. The cap exists for the reason Composer's does: so that `Loop::wait()` gets back to the other service's tick and to the progress bar update. An entity with no outstanding jobs answers `0` without blocking.
4. **Settles are pushed to the worker from inside the tick.** For each finished job, the tick handler calls the worker's `__shirabe_resolve_job` / `__shirabe_reject_job` before it returns the remaining count. The worker resolves or rejects that job's `Deferred`, which runs the plugin's callbacks synchronously, in exactly the place Composer runs them.
5. **Futures on the Rust-internal path are recorded, not counted.** A future the Rust-side `InstallationManager` or `ComposerRepository` gets from `add()` is still owned by the caller and driven with `.await`. But `add()` registers a bookkeeping entry (a cancel handle) in the registry and the entry is removed when the future settles, so that `abortJobs()` can cancel it. The added cost is one insert and one remove per request, the same with or without plugins.
    * A tick neither polls nor counts these. In Composer the internal jobs and the plugin's jobs sit in one table, and the `countActiveJobs()` value a plugin reads includes both. Here, whenever plugin code runs, every Rust-internal future is suspended beneath it on the Rust call stack and cannot advance until the plugin returns. Counting them would make the `while` of every `wait()` a plugin calls from inside the execution spin forever.
6. **The reverse direction is symmetric.** A promise a plugin hands to Rust (what a custom `InstallerInterface` returns from `install()`, for instance) is registered by the worker as a *PHP-owned* job. The worker attaches a recorder to it, which pushes `__shirabeResolveJob` / `__shirabeRejectJob` back to Rust when it settles. That is right away for a promise that had settled before it crossed, so the report of such a job arrives before its descriptor does, and the Rust registry records an outcome for an id it has not seen yet. Awaiting a PHP-owned job on the Rust side means ticking the Rust registry until that id is recorded as settled: every poll of the await polls the whole job table, since the settle push of a Rust-owned job is what gets PHP code to run.

### Why not the alternatives

*Promises as entities.* Proxying `PromiseInterface` through an R handle or a P handle makes `then()` a round trip, puts the chain state on the wrong side of the boundary, and still leaves open who drives it. A chain a plugin builds (`$d->add($url)->then(...)`) produces a *derived* promise whose settling Rust can infer nothing about. So the entity model buys nothing: what Rust has to drive is still the job, not the promise. Composer itself never asks "has this promise settled?" either. It asks "are there active jobs?".

*Deferred settling without a tick.* The worker's top level is a standing serve loop, so a call from Rust arrives even while PHP is idle. The problem is not delivery but driving. Advancing a job outside a tick needs an executor on the Rust side that runs independently of the wait points, and the Composer graph is `Rc<RefCell<_>>`, hence `!Send`, so it cannot go on `tokio::spawn`. And if it could, nothing settles outside `wait()` in Composer, so a `then` that runs while the plugin is in `sleep()` after `add()` would move *away* from Composer, not toward it.

*Keeping eager settling.* This is a correctness bug, not just a performance problem. When every job settles before the call that started it returns, `countActiveJobs()` can only answer `0`, `abortJobs()` has nothing to cancel, the progress bar has nothing to advance, and a plugin that starts ten downloads gets them run one after another. The whole observable surface of the async API degenerates.

## Wire

The additions to [docs/dev/php-rpc.md](./php-rpc.md) are small, deliberately. There is no new frame tag and no new message direction.

### A fourth descriptor kind

Alongside `{__rhandle, …}`, `{__phandle, …}` and `{__pclass}`, a job travels as a reserved single-key array.

```
{__job: <int>}
```

Ownership is encoded in the parity of the id, as it already is for correlation ids. **Rust allocates odd job ids, the worker even ones.** An even id names either a PHP-owned job or a settled promise the worker built for `__shirabe_resolved_promise` / `__shirabe_rejected_promise` and is holding until the id comes back. The worker's `fromWire` creates and interns a `Deferred` for an id it has not seen, so one id always resolves to the same promise object. The intern table holds the `Deferred` strongly, and the entry is removed when the settle push arrives. A `Deferred` outlives the plugin dropping its promise, until the job settles, just as Composer's `$this->jobs[$id]['resolve']` keeps hold of the promise's internals. The worker's `toWire` maps a `React\Promise\PromiseInterface` to an even id and registers the settle recorder on it. Rust's `decode_handle` matches on the whole key set, as it does for the other three kinds.

### Worker functions (Rust → PHP)

| Name | Purpose |
|---|---|
| `__shirabe_resolve_job` | `(id, value)` — resolves the `Deferred` of a Rust-owned job. Runs the plugin's callbacks synchronously. |
| `__shirabe_reject_job` | `(id, class, message, code, properties)` — the failure half. `\Shirabe\MaterializedThrowable` builds the exception from the same four fields a `Throw` frame carries and rejects the `Deferred`. |
| `__shirabe_resolved_promise` | `(value)` — the fast path for a Rust method that must answer with `PromiseInterface` and has the value before building a future. There is no job to register. A promise is never an entity, so this cannot hand the promise itself back: the worker parks it under an even id and answers with `{__job}`, and the Rust method passes that descriptor on as its own answer, where `fromWire` exchanges it for the parked promise. |
| `__shirabe_rejected_promise` | `(class, message, code, properties)` — the failure half. |

### Runtime service methods (PHP → Rust, handle 0)

| Name | Purpose |
|---|---|
| `__shirabeResolveJob` | `(id, value)` — the worker reports that a PHP-owned job resolved. |
| `__shirabeRejectJob` | `(id, class, message, code, properties)` — the failure half. |
| `__shirabeCancelJob` | `(id)` — called from the canceller of the job's promise. Drops the future, and with it whatever the future owns: for a process job that is the `Process`, whose drop stops the child. If the job had started, it returns the exception the canceller should throw, as the same four fields a `Throw` carries. If it had not, it returns `null`. Composer's canceller throws only for `STATUS_STARTED` and silently turns a `STATUS_QUEUED` job into `STATUS_ABORTED`. |
| `__shirabeAbortJobs` | `()` — forwards `Loop::abortJobs()`, so that the requests the Rust-side execution is awaiting (the recorded entries of rule 5) are cancelled too. |

These four are served for every dispatcher: the worker reports on a job whenever the promise behind it settles, whichever call happens to be in flight then.

### Reentrancy is the load-bearing constraint

Rule 4 means the tick handler calls back into PHP in the middle of its own work, and the plugin callback that runs there may call straight back into Rust. Calling `add()` for another job is not a pathological case but the ordinary one. Composer also explicitly allows nested `wait()` (the `$currentPromises` comment says so). A plugin that calls `SyncHelper::await()` inside a settled callback calls a tick from inside a tick. The session lock already allows this (a `CallRustMethod` handler can nest calls into PHP), but the job registry has to be written for it.

* The registry is one per thread (a `thread_local!` `RefCell`, like the R table) and sits in its own cell, not inside the `RefCell<HttpDownloader>` / `RefCell<ProcessExecutor>` the dispatcher borrows. Each job records which entity it is counted against (which `countActiveJobs()` counts it). Otherwise a nested `add()` during a tick hits a double borrow and panics.
* **Job futures live in a slot table. A tick never builds a collection of futures on its stack.** A tick takes one future out of its slot, releases the registry borrow, polls it once with a shared waker, and then either puts it back or removes the slot and pushes the settle. Nothing is held across the settle push, so an inner tick called from inside it can advance every remaining slot. If the futures were held as a collection on the stack, the outer tick would push the settle while still owning them, the inner tick could not touch the outer one's jobs, and each would wait for the other to return: a deadlock.
* A slot whose future is taken out for a poll is empty. An inner tick skips an empty slot and does not count it. This only happens when the job's own future calls PHP and that PHP code calls a tick back (a `DownloadManager::download()` job that delegates to a plugin-provided `DownloaderInterface`, for instance). The job is on the call stack then, where Composer has the synchronous call that will build the promise, and not yet a job in the table. Counting it would keep the inner `wait()` looping on a job that cannot move until that very `wait()` returns.
* A pass over the table leaves its own waker on every job it polls. A pass that ran inside another one has therefore taken over the outer pass's wakers by the time it returns, so the outer pass does not go to sleep on them: a tick returns, and the Rust-side `Loop::wait()` polls again.
* `__shirabeCancelJob` rides the same structure. Dropping the future is removing the slot, which can be done at any time a tick is not holding it.

## Per-class decisions

| Class | Category | Decision |
|---|---|---|
| `Composer\Util\HttpDownloader` | `rust-proxy` (unchanged) | `add()`/`addCopy()` register a job and answer with a pending promise. `wait()`/`countActiveJobs()` tick. |
| `Composer\Util\ProcessExecutor` | `rust-proxy` (unchanged) | `executeAsync()` starts the child process on the spot when the job budget has room, and registers a job that resolves with a `Shirabe\RustProcessStub`. `wait()`/`countActiveJobs()` tick. |
| `Symfony\Component\Process\Process` | `php-native` (unchanged) | Stays real. The path where a plugin writes `new Process([...])->run()` keeps working as is. |
| `Shirabe\RustProcessStub` | **New: runtime class**, a subclass of `Process`, post-mortem surface only | The object the `executeAsync()` promise resolves with. |
| `Composer\Downloader\DownloadManager` | `rust-proxy` (unchanged) | `download()`/`prepare()`/`install()`/`update()`/`remove()`/`cleanup()` register their future as a job. The job is counted against the graph's `HttpDownloader`, the one the Rust implementation uses internally. |
| `Composer\Util\Filesystem` | `rust-proxy` (unchanged) | `removeDirectoryAsync()` registers a job counted against that entity's `ProcessExecutor` (the one it creates for itself when it was built without one), unless the removal settled inside the call. |
| `Composer\Util\Loop` | **`two-world`** (was `rust-proxy`) | The worker runs a port of the class on top of the proxied services. |
| `Composer\Util\SyncHelper` | **`two-world`** | Pure orchestration over `Loop::wait()`. |
| `React\Promise\*` | `php-native` (unchanged) | Never crosses the boundary. The worker's own copy is the only one. |

A job needs an entity to be counted against because Composer's `Loop::wait()` terminates on the sum of `HttpDownloader::countActiveJobs()` and `ProcessExecutor::countActiveJobs()`. A job counted by neither would leave `Loop::wait()` returning while it is still pending. In Composer the promise of `DownloadManager::download()` is a `then` chain over what `FileDownloader` got from `$this->httpDownloader->addCopy()`, and the root job sits in the table of the graph's `HttpDownloader`. On the Rust side the `DownloadManager` future awaits the `add_copy` future internally, so only the outer future is registered as a job. Counting it against the same service makes the number a plugin observes match.

### HttpDownloader

The shape does not change. What changes is when the future runs. `add()` and `addCopy()` build the future, register it, and answer with `{__job}`. The synchronous gates stay where PHP has them, *before* a promise exists: the empty-URL gate and the `enableAsync()` gate still throw out of the call, and only a failed request becomes a rejection. `wait(?int $index)` and `countActiveJobs(?int $index)` acquire their real meaning. Composer's `$index` is the job id, so the registry id serves directly as the index these two take.

The per-downloader job budget (`COMPOSER_MAX_PARALLEL_HTTP`, 12 by default) is a semaphore the Rust-side downloader already holds. It already limits concurrency on the Rust-internal path. Under the job model, requests a plugin starts queue against the same budget for the first time.

### ProcessExecutor and the Process object

`executeAsync()` was blocked by two things. Rule 1 solves the promise representation. What remains is that it resolves with a `Symfony\Component\Process\Process`. The state of that object is the `proc_open()` resource and the OS pipes of whichever process called `start()`. That process is the Rust process, which has no PHP object to hand back.

The decision is **`Shirabe\RustProcessStub`, a runtime class that extends `Symfony\Component\Process\Process` and is limited to a post-mortem surface**. Like a generated stub it is bound to an R handle through `newInstanceWithoutConstructor` and `__shirabeBind`, and it overrides every public method. A terminated process does not change any more, so the stub reads everything the Rust-side `Process` has to report in one round trip (`__shirabeFacts`) and answers from that; the incremental-output cursors and `clearOutput()` are kept in the stub. `Process` itself is not replaced with a same-FQCN stub, because a stub shadows the implementation: symfony/process is `php-native` with real code running, and a plugin doing its own `new Process([...])` and `run()` is an ordinary path. With a subclass, `instanceof Process` holds and the real class is untouched.

Limiting the surface to post-mortem use is justified by when the promise settles. Composer resolves it from `countActiveJobs()` only once `!$process->isRunning()`. So *the object a plugin receives is always a terminated process, whatever its exit code* (a non-zero exit resolves the promise; only a failed start rejects it). The surface that controls a running process is therefore not something the contract needs, and it does not have to work.

Start timing matches Composer (the exception in rule 1). When the job budget (`COMPOSER_MAX_PARALLEL_PROCESSES`, 10 by default) has room, `executeAsync()` starts the child process before the call returns. If a plugin does other work between `executeAsync()` and its wait, the child is running in the meantime. The synchronous gate stays where `HttpDownloader` has it: the `enableAsync()` gate is checked before the future is built and throws out of the call.

* **Provided**: `isSuccessful()`, `isStarted()`, `isTerminated()`, `isRunning()` (false), `getExitCode()`, `getExitCodeText()`, `getOutput()`, `getErrorOutput()`, `getIncrementalOutput()`, `getIncrementalErrorOutput()`, `clearOutput()`, `clearErrorOutput()`, `getCommandLine()`, `getWorkingDirectory()`, `getEnv()`, `getInput()` (null), `getStatus()`, `getPid()` (null, what Symfony returns for a process that is no longer running), `getStartTime()`, `getTimeout()`, `getIdleTimeout()` (null; the executor sets none), `checkTimeout()` (a no-op on a terminated process), `hasBeenSignaled()`, `getTermSignal()`, `hasBeenStopped()`, `getStopSignal()`, `isOutputDisabled()`, `isTty()`, `isPty()`, and `stop()`. On a terminated process `stop()` only reports the exit code.
* **Explicit error**: `start()`, `run()`, `mustRun()`, `restart()`, `wait()`, `waitUntil()`, `signal()`, `setInput()`, `getIterator()`, `addOutput()`, `addErrorOutput()`, `clone`, and the setters that only mean something before the start. `getLastOutputTime()` is an explicit error as well, because the Rust-side `Process` does not track it. A plugin that reaches these is asking to drive a child process from the worker. That is a different feature (a PHP-side sibling of `ProcessExecutor`), not one this decision opens.

`Composer\Util\ProcessExecutor` itself stays `rust-proxy` for the reasons already recorded in [plugin-class-classification.md](./plugin-class-classification.md). `$timeout` is process-wide state both worlds read and write. The child has to be spawned by the Rust process to keep a single source for standard I/O and for the executable path cache. And an executor a plugin `new`s has to be the same kind of entity as the one the graph holds.

Cancellation mirrors the branches of Composer's canceller. A started job is dropped, which stops its child, and the PHP-side canceller throws the `RuntimeException('Aborted process')` that `__shirabeCancelJob` returns. For an `HttpDownloader` job the same path returns an `IrrecoverableDownloadException`. A job that has not started is silently dropped and the canceller does not throw. A `DownloadManager` job stands for a whole promise chain whose root is not known at the boundary, so cancelling it throws an `IrrecoverableDownloadException` of its own instead of the exception of the request or the process at the root.

### Loop and SyncHelper become two-world

`Loop::wait()` is about thirty lines of orchestration over three public collaborators (`HttpDownloader::countActiveJobs()`, `ProcessExecutor::countActiveJobs()`, `ProgressBar`), plus `React\Promise\all()` and a `$currentPromises` table that exists only so that `abortJobs()` can cancel promises. Every part of it is promise state or console state, and by the decision above both belong to PHP. `SyncHelper` is thinner still: `await()` is `$loop->wait([$promise])`.

So the worker runs `Composer\Util\Loop` and `Composer\Util\SyncHelper` on top of the proxy stubs of that run's downloader and executor. `SyncHelper` is the real class. `Loop` is a runtime class (`php/runtime/Composer/Util/Loop.php`): a port of the real one whose only addition is the forward in `abortJobs()` described below. It keeps the FQCN, because a subclass would answer `get_class()` differently. Four things follow.

* **The progress bar works.** A `ProgressBar` is a symfony/console object each world implements separately, so there is nothing to hand across. Because `Loop::wait()` runs in the worker, the bar a plugin passes is advanced in the worker, by the real code, on the real object. The `two-world` seam moves to exactly where the symfony/console seam already is. The update granularity is set by the tick's time cap (rule 3), as in Composer.
* **`React\Promise\all()` and the rethrow of a rejection become real.** They are no longer a Rust reimplementation of what `all()` hands PHP.
* **`getLoop()` needs one piece of plumbing.** `Composer::getLoop()` is served by building a worker-side `Loop` over the two proxied services (`__shirabeLoopServices` on the Composer entity answers with both) and memoizing it per Composer entity, so `$composer->getLoop() === $composer->getLoop()` holds. The memo holds the Composer stub as well, which keeps the entity's handle from being released and re-issued under the memo. This instance does not cross the boundary. What crosses is the seam objects it is built from; that is what `two-world` means. The `PartialComposer` stub is generated, so this one method is an explicit exception in the stub generator ([plugin-stub-generation.md](./plugin-stub-generation.md)).
* **Rust-side surfaces that take a `Loop` need the reverse plumbing.** `ArchiveManager::__construct(DownloadManager, Loop)` and `Factory::createArchiveManager(Config, DownloadManager, Loop)` are Rust-owned, and a plugin can pass `new Loop($httpDownloader, $process)` the way `ArchiveCommand` does. The PHP-side `Loop` arrives as a P handle, so the Rust side asks it for `getHttpDownloader()` / `getProcessExecutor()` and builds its own `Loop` from the entities behind the R handles that come back. The P handle is released once it has been used for that. Neither surface has a proxy stub yet, so this plumbing has nothing to attach to and does not exist.

The Rust side keeps its own `Loop` (`crates/shirabe/src/util/loop.rs`) for the execution itself. The two are siblings over one job registry, and both see the same registry.

* The Rust-side `Loop::wait()` still drives the futures it is given with await. That parks on the reactor and is not a tick. In addition, if the registry still holds Rust-owned jobs counted against the loop's two services (ones a plugin started with `add()` and returned without waiting on), it polls those within the same wait and does not return while any is left. In Composer, a promise a plugin dropped settles, and its `then` runs, inside the next `InstallationManager` wait. This branch sits under a guard that finds nothing in a run where the worker was never started.
* The one behavioural deviation is in `abortJobs()`. An `abortJobs()` a plugin calls cancels the groups *the worker's* Loop is waiting on, and reaches the execution side only through the `__shirabeAbortJobs` forward, which only the Loop that serves `Composer::getLoop()` makes (a Loop a plugin built for itself shares no `$currentPromises` table with the execution in Composer either). The forward cancels the recorded requests (rule 5): each fails with the `IrrecoverableDownloadException` Composer's canceller throws. The Rust-side `Loop::abort_jobs()` cancels the same entries for its own downloader. Two things fall short of Composer's reach. Composer cancels both sides in one step where this takes two; the window in which that is observable, a plugin callback running inside the execution side's own `wait()` and calling `abortJobs()` there, is acceptably narrow. And only requests are recorded, so a process the execution side is awaiting is not cancelled.

## Failure model

A job rejection crosses the boundary as the four fields a `Throw` frame carries and is rebuilt in the worker by `\Shirabe\MaterializedThrowable`. So `catch (TransportException $e)` keeps working and the status code can still be read off it. Nothing changes here.

What is new is the **deadlock case**, which becomes an explicit error rather than a hang. A PHP-owned job settles only when PHP code runs, and PHP code runs only when Rust calls PHP. So when all that is left in the group of a Rust-side `Loop::wait()` is awaits of PHP-owned jobs, and the registry has no Rust-owned job left to tick for the loop's services, that promise is waiting on something outside the model (a plugin promise that no Composer job resolves). Instead of spinning, Rust fails those awaits, naming the job. The check is made over the whole group because as long as a sibling future (one that calls another package's installer, for instance) can still call PHP, that PHP code may settle the promise in question. It is a count: the group is stuck when at least as many awaits are parked on a PHP-owned job as it has futures left. A future that awaits a PHP-owned job *alongside* something else is therefore misjudged; the ported callers await one step at a time. The equivalent situation in Composer is an infinite `while (true)`, which is not behaviour worth reproducing. An await of a PHP-owned job outside any `Loop::wait()` has no group to check and parks forever in that situation.

A job whose promise a plugin dropped without ever waiting is driven and settled at the next wait point (a worker-side `wait()`, or the Rust-side `Loop::wait()`), as in Composer. With no wait point ever coming it stays in the registry, which keeps the service it is counted against alive, until the process exits.

## What this does not buy

Whether I/O actually overlaps depends on the job's future being non-blocking end to end. The requests and the processes a plugin starts directly are: a curl request and a child process only park on the reactor. So is a `DownloadManager` job for a dist package, whose download is awaited through `HttpDownloader::add_copy` all the way down. A job that wraps ported synchronous code is not: a `DownloadManager` job for a VCS source runs its commands through the synchronous `ProcessExecutor::execute`, and the metadata fetches of `ComposerRepository` go through the `sync_executor::block_on` bridge (see `util/sync_executor.rs`), so a single poll occupies such a future until that work completes and a tick drains such jobs one at a time. The boundary design neither depends on this being fixed nor fixes it. It only places the jobs so that overlap appears where the internals do not block. The surface a plugin can observe (pending promises, real `countActiveJobs()` values, a progress bar that advances, cancellation that works) is the same either way.

A `DownloadManager` job also borrows the manager for as long as its future lives, as the Rust-side callers of the same methods do. While such a job is pending between two ticks, a call that needs the manager mutably (`setPreferSource()`, `setPreferDist()`) panics on the cell.

Some things are out of scope here: an event loop in the worker, promises crossing the wire as entities, and the running-control side of `Process`. Windows is on hold for now, as project-wide policy has it, and Unix-specific spots such as sending `SIGINT` are written in the same branch shape as Composer.

## Staging

Stages 1 to 4 exist. Of stage 5, the bridges on the install path are gone; the ones listed in `util/sync_executor.rs` remain.

1. **Make `Loop` and `SyncHelper` `two-world`.** `Composer::getLoop()` serves a memoized worker-side sibling, `abortJobs()` is forwarded, and the progress bar is unblocked.
2. **The job registry.** The `{__job}` descriptor, `__shirabe_resolve_job` / `__shirabe_reject_job` / `__shirabeCancelJob` / `__shirabeAbortJobs`, a registry with the slot table, the counted-against entity and the bookkeeping entries, the four async methods of `HttpDownloader` on top of it, the six `DownloadManager` methods and `Filesystem::removeDirectoryAsync()` turned into jobs, and the guarded tick in the Rust-side `Loop::wait()`. The registry is `crates/shirabe/src/plugin/job_registry.rs`.
3. **`Shirabe\RustProcessStub`**, with `ProcessExecutor::executeAsync()`, `wait()` and `countActiveJobs()`.
4. **PHP-owned jobs.** `__shirabeResolveJob` / `__shirabeRejectJob` and the deadlock check in the Rust-side `Loop::wait()`. The promises a plugin-provided `InstallerInterface` returns are awaited this way.
5. Independently of all of the above: remove the internal `block_on` bridges. The jobs that wrap ported code overlap once the code they wrap stops blocking.

Not built, because nothing can reach it yet: the reverse plumbing for Rust-owned surfaces that take a `Loop`.

## Deviations to record

The ones a plugin author can run into are listed in [known-incompatibilities.md](../known-incompatibilities.md).

* A `Loop::abortJobs()` called from a plugin reaches the execution side through a forward, not through a shared `$currentPromises` table, and what it cancels there are the requests in flight, not the processes.
* The `Process` passed to an `executeAsync()` callback is an instance of `Shirabe\RustProcessStub`, and its running-control surface and `getLastOutputTime()` are an explicit error.
* Cancelling a started process job stops the child with the signals of `Process::stop()` (`SIGTERM`, then `SIGKILL`), where Composer sends `SIGINT` first.
* Cancelling the promise of a `DownloadManager` method rejects it with an `IrrecoverableDownloadException` that names the method, where Composer rejects it with the exception of the request or the process at the root of the chain.
* `countActiveJobs()` does not include the jobs Composer itself has in flight, and a `wait()` called from inside the execution returns once the jobs plugin code started are done, where Composer's drains its own as well.
* What `Filesystem::removeDirectoryAsync()` finds wrong before any job exists is thrown out of the call. Composer does the same for the edge cases, but turns a process that fails to start into a rejection.
* Awaiting a plugin promise that nothing can settle is an explicit error where Composer loops forever.
* `ProcessExecutor::countActiveJobs()` blocks until a child process exits or 1 millisecond elapses, where Composer returns without blocking. The number reported is the same.
* With `COMPOSER_DISABLE_NETWORK` set, Composer settles synchronously inside `add()` (when the job budget has room), so a `then` runs on the spot; here it does not run until the next tick.
* An `HttpDownloader::get()` a plugin calls ticks the other pending jobs too in Composer, through `wait($id)`; here it only completes its own request.
* A request curl does not take never touches the job budget here. In Composer it is never counted as running either, but `markJobDone()` still decrements the running count when it settles.
