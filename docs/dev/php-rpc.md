# PHP RPC

Composer checks platform requirements (a specific PHP version or loaded extensions), and its
plugin and script machinery executes real PHP code. Mimicking this behavior requires a real PHP
runtime.

The `shirabe-php-rpc` crate spawns the system PHP as a child process and talks to it over a Unix
domain socket. There is exactly one child process per Shirabe process, shared by every caller.

## Locating and spawning PHP

The ported `PhpExecutableFinder` class resolves the PHP binary.

The child is started with the following arguments:

* `-d serialize_precision=-1` for stable float formatting of `serialize()`/`unserialize()`
* `-d phar.require_hash=0` (see [docs/dev/composer-runtime-bundle.md](./composer-runtime-bundle.md))
* the `-d` settings and environment variables of the `WorkerConfig` the embedder registers
  with `set_worker_config_provider`, asked for at spawn time. The `shirabe` crate registers
  `-d xdebug.mode=off`, together with `XDEBUG_MODE=off` in its environment, unless
  `COMPOSER_ALLOW_XDEBUG` is set (see [docs/dev/xdebug.md](./xdebug.md))

## Transport

- A `socketpair(2)` (no Windows support for now). The parent keeps one end and installs the
  other on file descriptor 3 in the child, from a `pre_exec` hook, so the worker opens it as
  `php://fd/3` rather than connecting anywhere. A bound path would have to fit in `sun_path`
  (108 bytes), which a long `TMPDIR` overruns. The pair is connected from the start, so nothing
  has to wait for an `accept` either: a child that dies before reading surfaces as EOF on the
  first call.
- The PHP half of the protocol (`php/worker.php`, which bootstraps
  `php/runtime/Shirabe/RpcRuntime.php`) and the PHP files the worker loads on top of it (see
  [docs/dev/php-worker.md](./php-worker.md)) are embedded in the Rust binary and written to a
  `0700` temp dir at spawn time, so both halves of the protocol are always the same commit.

### Frame layout

```
[u64 length LE]   -- number of bytes that follow (1 + 8 + payload length)
[u8  tag]         -- frame tag, see below
[u64 corr_id LE]  -- correlation id; 0 for one-way notifications
[payload]         -- the remaining fields as one PHP-serialize()d list
```

### Frame tags

| tag | name | direction | payload fields |
|---|---|---|---|
| `0x00` | `CallFunction` | both | `function_name`, `args` |
| `0x01` | `CallMethod` | both | `entity_id`, `method_name`, `args` |
| `0x02` | `CallStaticMethod` | both | `pclass`, `method_name`, `args` |
| `0x03` | `NewObject` | both | `pclass`, `ctor_args` |
| `0x04` | `Return` | both | `value`, `out_params` |
| `0x05` | `Throw` | both | the exception's throwable record: `exception_class`, `message`, `code`, `properties`, `file`, `line`, `parents` |
| `0x06` | `ReleaseHandle` | both | `entity_id` |

The Rust side allocates odd correlation ids, the PHP side even ones.

`NewObject`, `CallMethod` and `ReleaseHandle` create, call and drop an entity in the receiver's
table. The `entity_id` of a `CallMethod` or a `ReleaseHandle` always names an entry of the
receiver's table: a Rust entity id when the PHP side sends it, a PHP entity id when the Rust side
does.

- `NewObject` from the Rust side instantiates a class in the worker and answers with a
  `PhpEntityHandle` for the new entry of the PHP entity table.
- `NewObject` from the PHP side is sent by a proxy stub's constructor: the Rust side builds a fresh
  entity of `pclass` (the stub's `static::class`) and answers with its Rust entity id as a plain
  integer rather than a handle, since the stub under construction binds itself to it.

`CallStaticMethod` from the PHP side is sent by the static methods a proxy stub or a guard forwards
instead of running them in the worker (see
[docs/dev/plugin-stub-generation.md](./plugin-stub-generation.md)).

### Values: `RpcValue` and the codec

Payloads are encoded with a Rust reimplementation of the PHP `serialize()`/`unserialize()`
grammar, byte-compatible with the PHP core implementation.

The value model is `RpcValue`. Besides the values PHP's `serialize()` itself writes — scalars,
lists, ordered maps and object records — it has three handle kinds, which stand for something that
is not sent by value.

#### Scalars

`Null`, `Bool`, `Int`, `Float` and `String`. A string is a byte string, as in PHP. The worker runs
with `serialize_precision=-1`, so a float is written with the shortest representation that reads
back as the same value, on both sides.

#### Lists and ordered maps

An `a:` record decodes as `List` when its keys are exactly `0..N` and as `Array` otherwise.

#### Handles

Each handle kind is the object record of a class reserved for it:

- `Shirabe\Rpc\RustEntityHandle {entityId, class}` — an entity that lives on the Rust side
- `Shirabe\Rpc\PhpEntityHandle {entityId, class, implements}` — an entity that lives on the PHP
  side
- `Shirabe\Rpc\PromiseHandle {id}` — a promise. The Rust side allocates odd promise ids, the PHP
  side even ones. See "Promises" below.

A handle is the whole record; the integer in it is an id (a Rust entity id, a PHP entity id or a
promise id).

#### Object records

Some immutable objects are sent as objects rather than as handles. Such an object is called a
*by-value object*. Like an object passed to PHP's `serialize()`/`unserialize()`, it is encoded
and decoded from its class name and the values of its properties. For the list of these classes,
see "By-value objects" in [docs/dev/php-worker.md](./php-worker.md).

### Converting Rust values

Rust code does not handle `RpcValue` directly. It works with ordinary Rust values, which the
`ToRpcValue` and `FromRpcValue` traits convert to and from `RpcValue`.

## The Rust entity table and the PHP entity table

Each side keeps a table of its own objects that a handle held by the other side points to:

- The Rust entity table is on the Rust side and maps a Rust entity id to a Rust object.
- The PHP entity table is on the PHP side and maps a PHP entity id to a PHP object.

The Rust entity table (`RUST_ENTITY_TABLE`) holds each entry strongly while the PHP-side object
standing for it lives: that object's destructor sends `ReleaseHandle`, and the entry is removed.
The same Rust object is always sent with the same Rust entity id.

The PHP entity table (`Shirabe\PhpEntityTable`) holds each entry strongly until the Rust side sends
`ReleaseHandle` for it. `Shirabe\RpcRuntime::toRpcValue` turns any object that is not a proxy stub,
a promise or a by-value object into a `PhpEntityHandle` through `Shirabe\PhpEntityTable::handle`,
which interns by `spl_object_id`, so the same PHP object is always sent with the same PHP entity id.
`Shirabe\RpcRuntime::fromRpcValue` resolves a handle back to the live object through
`Shirabe\PhpEntityTable::get`.

The Rust side holds a PHP entity handle as `PhpEntityHandle`, the only form a PHP entity id takes
outside the codec. All `PhpEntityHandle`s for one PHP entity id share a single reference-counted
entry: decoding a handle whose PHP entity id is already held yields another clone of it, however
many times and inside whichever value the entity arrives. The Rust side therefore holds at most one
reference to each entry of the PHP entity table, so the worker needs no reference count. When the
last clone drops, the PHP entity id is queued, and `ReleaseHandle` for every queued id is written
ahead of the next frame the Rust side sends.

## Calls

### Entry points

`CallFunction` from the Rust side first consults the worker's dispatch table of named entry
points, then falls back to calling the named PHP function.

`CallFunction` from the PHP side names one of the runtime services of the Rust side, which answer
the calls that have no receiver entity. No handle is reserved for them.

Every name Shirabe defines on either side — an entry point, a runtime service, and a method
an entity answers beyond its PHP class's own (such as `__shirabe_clone`) — is
`__shirabe_<snake_case>`.

### By-ref parameters

The `Return` answering a `CallMethod` from the PHP side carries `out_params`, a map from the
positions of the called method's by-ref parameters to the value each one holds afterwards, and the
calling stub assigns those back into its own variables. The worker's `Return` always carries an empty
`out_params`.

## Promises

A promise stands for a computation one side started and the other side may wait on. The side that
started it owns its id and pushes the outcome to the other side once it settles; cancelling a
Rust-owned promise from the worker asks the Rust side to drop it. The worker's half of the registry
is `Shirabe\PromiseRegistry`, and how promise handles carry `React\Promise\PromiseInterface` across
is described in
[docs/dev/plugin-async.md](./plugin-async.md).

## Concurrency and reentrancy

Either side can call the other, and a call can arrive while the receiver is itself waiting for an
answer. For example, Rust calls a method of a plugin in the worker, and while it runs, the plugin
calls a method of a Composer object, which is a call back into Rust.

So neither side simply blocks until its answer arrives. After sending a call, a side keeps reading
frames until the `Return` (or `Throw`) for that call comes. When a frame is a call from the other
side instead, it handles the call, sends the answer back, and goes on waiting. Handling such a call
may send further calls, so calls can nest to any depth.

The worker is a single child shared by every thread of the process. A reentrant session lock keyed
by thread ID (`SessionGuard`) keeps the calls of one thread together: the thread that made the
outermost call re-enters the session for the nested ones, and other threads block until the whole
outermost call completes.

## Error handling

A failure on the channel is one of four kinds: a fatal error, an exception, a bug in Shirabe, or a
hang.

### Fatal errors

On the Rust side, the result of a `CallFunction`, `CallStaticMethod`, `NewObject` or
`CallMethod` is a nested result. The outer `anyhow::Result` carries the fatal errors: no PHP executable,
broken framing, oversized frames, and an I/O failure while the worker is still running
(with its state attached as context).

A worker that has ended takes Shirabe down with it: the worker stands in for the PHP process
Composer runs in, so when a read or a write finds it gone, Shirabe exits with its exit code, or is
killed by the signal that killed it, as Composer would be by a plugin's `exit()`, a fatal error or
a signal.

### Exceptions

The inner `Result<RpcValue, PhpThrow>` carries the recoverable failures: a PHP exception sent
as a `Throw` frame.

A value of the wrong type is an exception, not a fatal error. When an argument or an answer does
not have the type the receiving side expects, it is not coerced, much as under
`declare(strict_types=1)`: the receiving side throws a `TypeError` instead. A value of the wrong
type that only Shirabe's own code writes is a bug in Shirabe, not an exception.

An exception is sent by value, not as a handle: the sending side writes a throwable record of it
(its class, message, code, properties, location and superclasses), and the receiving side rebuilds
an exception from that record. The throwable record is the payload of a `Throw` frame, and the same
record is used wherever else an exception is sent, such as a promise rejection.

- Rust to PHP: `\Shirabe\ThrowableRecord` rebuilds the exception in the child —
  `new $class($message, $code)` for a class whose constructor has `\Exception`'s shape, an
  instance filled in field by field for one whose constructor has another, then the properties
  and the location by reflection — so a `catch` in the child catches what it would catch if the
  exception had been thrown there, and reads the same state off it.
- PHP to Rust: `parents` lists the superclasses of the class, nearest first, and
  `throw_into_error` rebuilds the ported exception of the class, or of the nearest superclass the
  port has a type for, answering to the class it was thrown as (`UnportedThrowable`). The state
  the PHP side records is that of the classes whose state the Rust side ports, listed in
  `ThrowableRecord::PROPERTIES`.

### Timeout

There is no timeout. A plugin that never returns hangs Shirabe exactly as it hangs Composer, which
runs it in its own process.

## Trust model

The worker runs plugin code, which Shirabe trusts exactly as Composer does: there is no sandbox.
The limits on what a frame may carry, such as the closed list of classes `unserialize()` may
revive, guard against bugs in Shirabe's own glue code, which is the only code that writes frames, not against an
attacker. The socket has no name on the filesystem,
so only the two processes holding its file descriptors can reach it.
