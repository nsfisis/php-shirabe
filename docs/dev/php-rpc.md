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
- The PHP half of the protocol (`php/worker.php`) and the PHP files the worker loads on top of it
  (see [docs/dev/php-worker.md](./php-worker.md)) are embedded in the Rust binary and written to a
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
- `{__job}` — an asynchronous job, an entity in neither table. The Rust side allocates odd job
  ids, the PHP side even ones. See "Jobs" below.

An immutable value has no entity to point at, so it crosses in neither table: it travels as the
object record `serialize()` writes for it (`PluginValue::PhpObject` — the class name and the
property table, property names carrying PHP's visibility mangling), and each side rebuilds its
own value from those fields. `unserialize()` revives the PHP one without running a constructor,
which is what makes the two directions equivalent: a value whose state a constructor cannot
express has no faithful constructor call, and no field has to be read back out through
reflection. The classes that may cross this way are a closed list on both sides — the PHP one is
the `allowed_classes` list of every `unserialize()` — so a record can never name an arbitrary
class. Which classes are on the list, and how each one maps to a Rust value, is not part of the
protocol; see "Materialized values" in [docs/dev/php-worker.md](./php-worker.md).

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
0 is reserved for the runtime service endpoint, which answers calls that have no receiver entity.

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
  and the location by reflection — so a `catch` in the child catches what it would catch if the
  exception had been thrown there, and reads the same state off it. A class the child does not
  know keeps the `RuntimeException` shape.
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

## Entry points

`CallFunction` first consults the worker's dispatch table of named entry points, then falls back
to calling the named PHP function; an unknown name is an explicit error. Shirabe-internal entry
points are prefixed `__shirabe_`. Likewise, a `CallRustMethod` on handle 0 names a method of the
runtime service endpoint; one it does not answer is an explicit error naming it.

The entry points themselves are not part of the protocol: each is a consumer of it, and the
catalogue lives in [docs/dev/php-worker.md](./php-worker.md).

### By-ref parameters

`out_param_positions` names the by-ref parameters of the called method, as the caller declared
them; the answering `Return` carries `out_params`, a map from the same positions to the value
each parameter holds afterwards, and the caller assigns those back into its own variables. A
position the answer leaves out was never assigned to, which is what PHP does with an untouched
by-ref parameter — so an out-param is genuinely optional rather than defaulting to null.

## Jobs

A job is a computation one side started and the other side may wait on. The side that started it
owns its id and pushes the outcome to the other side once it settles; cancelling a Rust-owned job
from the worker asks the Rust side to drop it. The worker's half of the registry is
`ShirabeJobRegistry`, and how jobs carry `React\Promise\PromiseInterface` across is described in
[docs/dev/plugin-async.md](./plugin-async.md).

## The R and P tables

The R table maps an rhandle to a Rust-side entity. Rhandles increase monotonically and are never
reused, so a released handle can never name a later entity. An entry is a strong reference held
exactly while the PHP-side object standing for it lives: that object's destructor sends
`ReleaseRustHandle`, and the entry is removed. The same entity always crosses as the same handle.
Whether an entry exists depends only on whether anything standing for it is still reachable in the
worker; one caught in a reference cycle keeps its entry until PHP's cycle collector frees it, and
everything goes away with the worker process.

`ShirabePhpObjectRegistry` holds PHP-owned entities keyed by phandle, strongly referenced until the
Rust side sends `ReleasePhpHandle`. `ShirabeRpcRuntime::toWire` turns any object that is not a
handle descriptor, a job or a materialized value of its own into a `__phandle` descriptor through
`ShirabePhpObjectRegistry::descriptor` (interned by `spl_object_id`, so one entity keeps one
handle); `ShirabeRpcRuntime::fromWire` resolves descriptors back to the live entity through
`ShirabePhpObjectRegistry::get`.

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
