# PHP RPC

Composer can require a specific PHP version or loaded extensions, i.e., platform requirements,
and its plugin/script machinery executes real PHP code. To mimic this behavior needs a real PHP
runtime.

The `shirabe-php-rpc` crate spawns the system PHP as a child process and talks to it over a Unix
domain socket. There is exactly one child process per Shirabe process, shared by every caller.

## Locating and spawning PHP

The existing `PhpExecutableFinder` class resolves the PHP binary.

The child is started with the following arguments:

* `-d serialize_precision=-1` for stable float formatting of `serialize()`/`unserialize()`
* `-d phar.require_hash=0` (see [docs/dev/composer-runtime-bundle.md](./composer-runtime-bundle.md))
* `-d xdebug.mode=off`, together with `XDEBUG_MODE=off` in its environment, unless
  `COMPOSER_ALLOW_XDEBUG` is set (see [docs/dev/xdebug.md](./xdebug.md))

## Transport

- A `socketpair(2)` (no Windows support for now). The parent keeps one end and installs the
  other on descriptor 3 in the child, from a `pre_exec` hook, so the worker opens it as
  `php://fd/3` rather than connecting anywhere. A bound path would have to fit in `sun_path`
  (108 bytes), which a long `TMPDIR` overruns. The pair is connected from the start, so nothing
  has to wait for an `accept` either: a child that dies before reading surfaces as EOF on the
  first call.
- The PHP glue code (`php/worker.php`), the proxy stubs (`php/stubs/`), the runtime classes
  (`php/runtime/`) and the guards (`php/guards/`) are embedded in the Rust binary and written to a
  `0700` temp dir at spawn time, so both halves of the protocol are always the same commit.

### Frame layout

```
[u64 length LE]   -- number of bytes that follow (1 + 8 + payload length)
[u8  tag]         -- message tag, see below
[u64 corr_id LE]  -- correlation id; 0 for one-way notifications
[payload]         -- the remaining fields as one PHP-serialize()d list
```

`length` is validated against `MAX_FRAME_LEN` (256 MiB) before any allocation; an oversized
frame is a fatal channel error, not an allocation attempt.

### Message tags

| tag | name | direction | payload fields |
|---|---|---|---|
| `0x00` | `CallFunction` | Rust→PHP | `function_name`, `args`, `out_param_positions` |
| `0x01` | `CallStaticMethod` | Rust→PHP | `pclass`, `method_name`, `args`, `out_param_positions` |
| `0x02` | `NewObject` | Rust→PHP | `pclass`, `ctor_args` |
| `0x03` | `CallPhpMethod` | Rust→PHP | `phandle`, `method_name`, `args`, `out_param_positions` |
| `0x04` | `CallRustMethod` | PHP→Rust | `rhandle`, `method_name`, `args`, `out_param_positions` |
| `0x05` | `Return` | both | `value`, `out_params` |
| `0x06` | `Throw` | both | `exception_class`, `message`, `code`, `properties`, `file`, `line`, `parents` |
| `0x07` | `ReleaseRustHandle` | PHP→Rust | `rhandle` |
| `0x08` | `ReleasePhpHandle` | Rust→PHP | `phandle` |
| `0x09` | `EpochBump` | Rust→PHP | `rhandle`, `epoch` |

The Rust side allocates odd correlation ids, the PHP side even ones. `NewObject` instantiates
a class in the worker and returns a `__phandle` descriptor for the new P-table entity;
`CallPhpMethod` invokes a method on such an entity; `ReleasePhpHandle` drops it.

### Values: `PluginValue` and the codec

Payloads are encoded with a Rust reimplementation of the PHP `serialize()` grammar
(`src/value.rs`), byte-compatible with the PHP core implementation under
`serialize_precision=-1` (the float formatting itself is ported in `shirabe-php-src`). The value
model is `PluginValue`: PHP scalars, byte strings (`Vec<u8>` — non-UTF-8 round-trips
losslessly), lists, ordered maps, object records, and four descriptor kinds encoded as reserved
arrays:

- `{__rhandle, __class, __epoch[, __snapshot]}` — entity lives on the Rust side
- `{__phandle, __class, __implements}` — entity lives in the PHP child
- `{__pclass}` — a PHP class name
- `{__job}` — an asynchronous job, which is how a `React\Promise\PromiseInterface` crosses: a
  promise is never an entity in either table. The Rust side allocates odd job ids, the PHP side
  even ones. See [docs/dev/plugin-async.md](./plugin-async.md).

An immutable value has no entity to point at, so it crosses in neither table: it travels as the
object record `serialize()` writes for it (`PluginValue::PhpObject` — the class name and the
property table, property names carrying PHP's visibility mangling), and each side rebuilds its
own value from those fields. `unserialize()` revives the PHP one without running a constructor,
which is what makes the two directions equivalent: a value whose state a constructor cannot
express — an unset pretty string, a `Link` built without a pretty constraint — has no faithful
constructor call, and no field has to be read back out through reflection.
`Composer\Package\Link` travels this way, together with the `composer/semver` constraint it
holds, and so does the `\DateTimeInterface` release date, rebased on UTC because the Rust side
carries no timezone database. So does the `Composer\Util\Http\Response` that `HttpDownloader`
answers with. The two halves are
`crates/shirabe/src/plugin/php_plugin_value.rs` and `Shirabe\MaterializedValue`; the set of
classes that may cross is a closed list on both sides — the PHP one is the `allowed_classes`
list of every `unserialize()` — so a record can never name an arbitrary class.

A constraint crosses structurally: `shirabe_semver::AnyConstraint` maps to and from the real
`Composer\Semver\Constraint\*` objects, never through a constraint string, so the pretty string
and the conjunctive flag survive. A subclass of a listed class (other than of
`\DateTimeInterface`, which is rebased onto a plain `\DateTimeImmutable`) is not narrowed to its
listed parent; it crosses as a P-table entity.

`PluginValue::Object` is encode-only: the wire erases the array/object distinction for a
class-less object, so the decoder only produces `List` (contiguous 0-based int keys) or `Array`
for an `a:` record. A repeated object instance arrives as PHP's `r:` back-reference, which the
decoder resolves by copying the value it names (identity means nothing to a value on this side);
a cyclic object graph and a PHP reference (`R:`) are both rejected. The decoder is iterative
(input nesting never becomes call-stack depth) and additionally rejects payloads nested deeper
than 512 levels.

The codec is verified against the real PHP `serialize()`/`unserialize()` by oracle tests
(`tests/oracle.rs`), with floats, non-UTF-8 byte strings, object records and deep nesting as
focus areas.

## Concurrency and reentrancy

A logical call session is serialized by a thread-ID based reentrant session lock
(`src/session.rs`): the owning thread may nest calls freely (a `CallRustMethod` handler can
itself call back into PHP), while other OS threads block until the whole outer session
completes. This keeps the single-child-process invariant under `cargo test`'s parallel harness.
A `thread_local!` worker would not: each test thread would spawn a child of its own. The worker
mutex itself is only held per frame, not across a call.

While a Rust-initiated call waits for its `Return`, incoming `CallRustMethod` frames are
dispatched to the caller-supplied `RustMethodDispatcher` (the cooperative loop); with no
dispatcher active they are answered with an explicit `Throw`, never a silent null. Rust handle
0 is reserved for the runtime service endpoint (e.g. `__shirabe_find_file`, which the worker's
script-class autoloader uses to ask the Rust-side `ClassLoader` where a class file lives).

The PHP side mirrors this: its top level is a standing serve loop, and `callRust` drives the
same dispatch while waiting for its own `Return`.

## Failure model

- A worker that has ended takes Shirabe down with it: the worker stands in for the PHP process
  Composer runs in, so when a read or a write finds it gone, Shirabe exits with its exit code, or
  is killed by the signal that killed it, as Composer would be by a plugin's `exit()`, a fatal
  error or a signal.
- The outer `anyhow::Result` of `call_function`/`call_static_method` is the fatal lane: broken
  framing, oversized frames, and an I/O failure while the worker is still running (with its
  state attached as context).
- The inner `Result<PluginValue, PhpThrow>` is the recoverable lane: a PHP exception crossing
  the boundary as a `Throw` frame.
- A `Throw` names the class the exception was thrown as, `properties` carries the state that
  class declares beyond `message` and `code`, and `file` and `line` are where it was created.
  Rust to PHP, `\Shirabe\MaterializedThrowable` rebuilds the exception in the child —
  `new $class($message, $code)` for a class whose constructor has `\Exception`'s shape, an
  instance filled in field by field for one whose constructor has another, then the properties
  and the location by reflection — so a plugin's `catch (TransportException $e)` catches what it
  would catch under Composer and reads the status code off it. A class the child does not know
  keeps the `RuntimeException` shape.
- PHP to Rust, `parents` lists the superclasses of the class, nearest first, and
  `plugin/throwable_bridge.rs` rebuilds the ported exception of the class, or of the nearest
  superclass the port has a type for, answering to the class it was thrown as
  (`UnportedThrowable`). The state the PHP side describes is that of the classes whose state the
  Rust side ports, listed in `MaterializedThrowable::PROPERTIES`.
- A frame that decodes to something protocol-invalid is a bug in Shirabe itself (both halves
  ship in the same commit) and panics; the PHP side symmetrically dies so Rust observes EOF.
- The legacy scalar query API (`get_php_version` etc.) keeps its historical contract: every
  failure panics.
- There is no timeout. A plugin that never returns hangs Shirabe exactly as it hangs Composer,
  which runs it in its own process.

## Worker dispatch table

`CallFunction` first consults the worker's dispatch table (composite queries like `diagnose` and
`platform`, Shirabe-internal helpers prefixed `__shirabe_`), then falls back to calling the named
PHP function; an unknown name is an explicit error.

A composite query answers everything one consumer needs about the runtime in a single round trip,
because asking one constant and one extension at a time costs a round trip each. The Rust side
decodes the answer into a struct cached in a `OnceLock` (`Diagnostics` for `diagnose`,
`PlatformInfo` for `platform`) whose accessors panic on a name the worker does not report, so a
consumer and the worker cannot drift apart unnoticed.

Notable internal helpers:

- `__shirabe_eval` — runs a Rust-generated PHP snippet and returns its `return` value (used by
  the `scripts` Command-class execution path and the `_composer_tmp` class-rename path of
  `PluginManager::registerPackage`).
- `__shirabe_require` — `require_once` a file (e.g. an autoloader) into the worker, then
  re-prepends the stub autoloader so proxied FQCNs keep resolving to stubs even when the
  required file registered its own prepending autoloader (a Composer `vendor/autoload.php`
  does).
- `__shirabe_enable_script_autoloader` — registers the autoloader that resolves classes through
  the Rust-side `ClassLoader` via handle 0.
- `__shirabe_composer_require` — the body of `\Composer\Autoload\composerRequire`, sharing
  its `$GLOBALS['__composer_autoload_files']` guard (files-autoload entries of plugin
  packages).
- `__shirabe_installed_versions_reload` — mirrors the tail of `FilesystemRepository::write`
  (the unconditional `InstalledVersions::reload($versions)` plus the reflection-based
  `selfDir`/`installedIsLocalDir` restore) into the worker; skipped only when the class is not
  even autoloadable there, i.e. no Composer PHP runtime and therefore no observer code.
- `__shirabe_resolve_job` / `__shirabe_reject_job` — the settle pushes of a Rust-owned job: the
  worker resolves or rejects the `Deferred` behind the job's promise, which runs the callbacks
  plugin code chained on it before the call returns. The rejection is the exception the fields
  of a `Throw` frame describe.
- `__shirabe_resolved_promise` — wraps a value in `\React\Promise\resolve()`, for a Rust method
  whose PHP signature declares `PromiseInterface` and that has the value before any job exists.
  The worker holds the promise under an even job id and answers with its `{__job}` descriptor,
  which the Rust method passes on as its own answer.
- `__shirabe_rejected_promise` — the failure half of the same: `\React\Promise\reject()` over
  the exception the fields of a `Throw` frame describe, so a failed request reaches the caller
  as a rejection it handles rather than as a throw out of the call that started it.
- `__shirabe_sync_env` — replays the environment writes the Rust side made after the worker was
  spawned into the storage each one names (`putenv()`, `$_ENV`, `$_SERVER`); see
  [docs/dev/env-vars-porting.md](./env-vars-porting.md). The outermost `rpc_call` of a session
  issues it whenever the journal has moved on.
- `__shirabe_get_property` — for testing only: reads a public property of a P-table entity.
- `__shirabe_reflection_invoke` / `__shirabe_reflection_get_property` /
  `__shirabe_reflection_set_property` — for testing only: the `ReflectionMethod` /
  `ReflectionProperty` access PHPUnit tests use to reach the non-public members of a P-table
  entity.
- `__shirabe_oracle_roundtrip` — codec oracle support for tests.
- `__shirabe_console_application_boot` — builds the worker-side `Composer\Console\Application`
  (the `php/runtime/` definition) from the Rust handoff: the shared `$composer`/`$io` proxies,
  the initial working directory and disable-by-default flags, `\Shirabe\RustCommandStub` rows
  mirroring the built-in commands, and the live plugin-provided command entities.
- `__shirabe_run_console_application` — runs one stringified command line through a booted
  worker-side application; output goes to the inherited stdio, the exit code returns over the
  wire, and a failure propagates as a Throw (the booted application does not catch exceptions).
- `__shirabe_read_command_definition` — reads a command's input definition (plus help text and
  extra usages) as plain data, so the Rust side mirrors it for `help`/`list` rendering.

The runtime service endpoint (handle 0) answers `__shirabe_find_file` (autoload lookups),
`__shirabe_run_rust_command` — the reverse half of the two-world command split: a
`\Shirabe\RustCommandStub` forwards its stringified input here and the built-in command runs in
the Rust process, against the Rust-side application state — `__shirabeConstruct`, which
allocates the Rust entity behind a `new SomeProxiedClass(...)` written by plugin code and
answers with `[rhandle, epoch]`, `__shirabeCallStatic`, which takes `[class, method, args]`
and runs a static method no handle identifies a receiver for, and the job methods
`__shirabeResolveJob` / `__shirabeRejectJob` / `__shirabeCancelJob` / `__shirabeAbortJobs`
([docs/dev/plugin-async.md](./plugin-async.md)). Classes and static methods Rust cannot answer
are an explicit error naming them.

### By-ref parameters

`out_param_positions` names the by-ref parameters of the called method, as the caller declared
them; the answering `Return` carries `out_params`, a map from the same positions to the value
each parameter holds afterwards, and the caller assigns those back into its own variables. A
position the answer leaves out was never assigned to, which is what PHP does with an untouched
by-ref parameter — so an out-param is genuinely optional rather than defaulting to null.

A generated stub fills both halves mechanically from the real signature
(`docs/dev/plugin-stub-generation.md`). `Composer\Util\ProcessExecutor::execute` is the method
this exists for: it is the only by-ref parameter on Composer's public surface that belongs to a
proxied class.

## Proxy stubs, runtime classes and guards

`php/stubs/` holds the proxy stub classes for the FQCNs listed in
`scripts/plugin-stub-generator/targets.list` (the Composer graph, IO, config, the
package/repository graph, the event classes, the transaction and its operations, and the
download, installation, process and filesystem services). They are generated by `scripts/plugin-stub-generator/generate-stubs` and must not be edited by hand; see
`docs/dev/plugin-stub-generation.md`.

`php/runtime/` holds hand-written worker-side classes that are not mechanical proxies:

- `Composer\Console\Application` — a same-FQCN two-world implementation (never the real class
  file): plugin-provided commands run under it inside the worker, and its Composer-specific
  surface (`getIO()`/`getComposer()`/...) answers from the Rust handoff.
- `Shirabe\RustCommandStub` — the reverse stub for built-in commands registered into that
  application.
- `Shirabe\RustPluginStub` and `Shirabe\RustCapablePluginStub` — proxy stubs for a plugin
  implemented on the Rust side, which has no class of its own here. A plugin is normally PHP
  code running in the worker; these stand in where Rust hands such a plugin to PHP code, as
  `PluginManager::getPluginCapability` does when it passes the plugin to a capability
  constructor. The Capable flavour exists because `$plugin instanceof Capable` decides whether
  Composer asks a plugin for capabilities.
- `Composer\Util\Loop` — a same-FQCN port of the real class, running in the worker over the
  `HttpDownloader` and `ProcessExecutor` proxy stubs. The instance `Composer::getLoop()` hands
  out forwards `abortJobs()` to the Rust-side `Loop`, its sibling over the same job registry.
- `Shirabe\RustProcessStub` — the `Symfony\Component\Process\Process` subclass an
  `executeAsync()` promise resolves with: the child was spawned by the Rust side, so it answers
  from what the Rust-side `Process` reports about the terminated process instead of from a
  `proc_open()` resource.
- `Shirabe\MaterializedThrowable` — the PHP half of the `Throw` codec: rebuilds the exception a
  `Throw` frame (or a job rejection) describes as its own class, or as a `RuntimeException` when
  the child does not know the class, and describes an exception raised in the child as the fields
  of a `Throw`.
- `Shirabe\MaterializedValue` — the PHP half of the materialized-value codec: the closed list of
  classes `unserialize()` may revive, and the hook that hands such an instance to `serialize()`
  in place of a handle descriptor.
- `Composer\EventDispatcher\Event` — dual-mode: revived from a Rust handle (through
  `__shirabeBind`) it proxies like a generated stub, while a natively-constructed instance (real
  Composer code in the worker does `new PreCommandRunEvent(...)`, whose parent constructor lands
  here) is a faithful in-process port of the real base class and crosses the wire as a P-table
  entity (`__shirabeRustHandleDescriptor()` returns null in native mode).
- `Composer\Plugin\PreCommandRunEvent` — dual-mode in the same way as its parent: the event a
  built-in command dispatches on the Rust side arrives as a proxy, while the one
  `BaseCommand::initialize()` constructs here for a plugin command is the real class's port.

`php/guards/` holds the guard classes: one per Composer class whose entity lives on the Rust side
and that neither a stub nor a runtime class shadows. A guard keeps the FQCN, the hierarchy and the
constants of the real class, and raises an explicit error from its constructor and every method,
so code running here can never work on a second instance the Rust side never sees. They are
generated by the same tool as the stubs.

The three sets are written into two autoload directories at worker spawn (guards behind stubs) and
resolved with highest priority, so these FQCNs can never be shadowed by the real implementation;
`__shirabe_require` restores that priority after loading code that prepends its own autoloader.
Stubs are interned per rhandle (`WeakReference`-based registry) so identity (`===`) holds, and
their destructors send `ReleaseRustHandle`. Reviving a stub for an existing entity bypasses its
constructor (`newInstanceWithoutConstructor` plus `__shirabeBind`), because the constructor
carries the real class's own signature and belongs to plugin code building a *new* entity. `clone` on a stub calls `__shirabeClone` on the
entity and rebinds the copy to the handle that answers, so the two stubs never share (and never
double-release) one entity; entities with no clone semantics answer with an explicit error.

## Jobs

`ShirabeJobRegistry` is the worker's half of the job registry
([docs/dev/plugin-async.md](./plugin-async.md)). `ShirabeRpcRuntime::fromWire` turns a `{__job}`
descriptor with an odd id into the promise of a `React\Promise\Deferred` the registry interns for
that id (`ShirabeJobRegistry::promiseFor`), held until the Rust side pushes the outcome; the
promise's canceller asks the Rust side to drop the job. `ShirabeRpcRuntime::toWire` turns any
`React\Promise\PromiseInterface` into a `{__job}` descriptor with an even id
(`ShirabeJobRegistry::descriptor`) and attaches a recorder that reports the outcome to the Rust side when the promise settles. The
Rust half lives in `crates/shirabe/src/plugin/job_registry.rs`.

## The R and P tables

The R table (`R_TABLE` in `crates/shirabe/src/plugin/php_plugin_proxy.rs`) maps an rhandle to a
Rust entity (`RustEntity`, one variant per kind of entity that crosses). Rhandles increase
monotonically and are never reused, so a released handle can never name a later entity. An entry
is a strong reference held exactly while the child-side stub lives: the stub's `__destruct` sends
`ReleaseRustHandle`, and the entry is removed. The same entity always crosses as the same handle;
interning scans the table for the same variant and shared-pointer address, with no separate
reverse index. There is no per-plugin bookkeeping: whether an entry exists depends only on whether
any stub for it is still reachable in the worker, which is also how long Composer keeps an object
a plugin holds on to. A stub caught in a reference cycle keeps its entry until PHP's cycle
collector frees it, and everything goes away with the worker process.

`ShirabePhpObjectRegistry` holds PHP-owned entities (e.g. plugin instances) keyed by phandle,
strongly referenced until the Rust side sends `ReleasePhpHandle`. `ShirabeRpcRuntime::toWire`
turns any non-stub object into a `__phandle` descriptor through
`ShirabePhpObjectRegistry::descriptor` (interned by `spl_object_id`, so one entity keeps one
handle); `ShirabeRpcRuntime::fromWire` resolves descriptors back to the live entity through
`ShirabePhpObjectRegistry::get`. The Rust-side counterpart —
the R table holding `$composer`/`$io` entities reachable from plugin callbacks — lives in
`crates/shirabe/src/plugin/php_plugin_proxy.rs`.

## Two-world console application

Built-in commands run on the Rust side, against the Rust-side `Application`. Commands whose
implementation lives in the worker — those a plugin's `CommandProvider` supplies, and `scripts`
entries naming a Symfony `Command` subclass — run under the worker-side
`Composer\Console\Application`. Each command always runs in the world that holds its
implementation, against that world's `HelperSet`, and crossing over hands the whole `run()` to the
other side as a command line that the other side parses again:

- Rust to PHP, the proxy of a worker-side command sends its input stringified, and the worker runs
  it through `__shirabe_run_console_application`.
- PHP to Rust, a `\Shirabe\RustCommandStub` forwards `(string) $input` to
  `__shirabe_run_rust_command`, which parses it with `StringInput`. PHP omits the command name from
  that string for an input built without a `command` entry, so the name is prepended when the first
  token is neither the command's name nor one of its aliases.

The handoff (`__shirabe_console_application_boot`) carries what the parsed command line cannot: the
`$composer`/`$io` proxies, the initial working directory and the disable-by-default flags. The
verbosity, decoration and interaction flags travel in the command line itself, and the worker,
spawned after the Rust side has configured its IO, inherits the environment and the standard
streams, so both worlds see the same terminal.

Whether a `scripts` entry names a `Command` subclass is decided in the worker, which first needs
the Composer PHP runtime loaded (the class extends Symfony's `Command`). The question is only asked
for a class the Rust-side `ClassLoader`s resolve to a file, so plain shell-command scripts never
load the runtime.

## Trust model

The worker runs plugin code, which Shirabe trusts exactly as Composer does: there is no sandbox.
The limits on the wire — the closed list of classes `unserialize()` may revive, the decoder's
nesting limit and `MAX_FRAME_LEN` — guard against bugs in Shirabe's own glue code, which is the
only code that writes frames, not against an attacker. The socket has no name on the filesystem,
so only the two processes holding its descriptors can reach it.

## Out of scope (deferred)

By-ref parameters on Rust-initiated calls (`call_function`/`call_static_method`/`call_php_method`
always send an empty `out_param_positions` and ignore the answer's `out_params`), epoch-based
cache invalidation on the PHP side (the worker routes `EpochBump` to `__invalidateCache`, but no
stub defines it and the Rust side never sends the frame), the `previous` of an exception crossing
in either direction, and Windows support.
