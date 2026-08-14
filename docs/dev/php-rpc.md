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
* `-d xdebug.mode=off` (see [docs/dev/xdebug.md](./xdebug.md))

## Transport

- A `socketpair(2)` (no Windows support for now). The parent keeps one end and installs the
  other on descriptor 3 in the child, from a `pre_exec` hook, so the worker opens it as
  `php://fd/3` rather than connecting anywhere. A bound path would have to fit in `sun_path`
  (108 bytes), which a long `TMPDIR` overruns. The pair is connected from the start, so nothing
  has to wait for an `accept` either: a child that dies before reading surfaces as EOF on the
  first call, with its exit status attached.
- The PHP glue code (`php/worker.php`) and the proxy stub classes (`php/stubs/`) are embedded in
  the Rust binary and written to a `0700` temp dir at spawn time, so both halves of the protocol
  are always the same commit.

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
| `0x06` | `Throw` | both | `exception_class`, `message`, `code` |
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
losslessly), lists, ordered maps, object records, and three handle descriptor kinds encoded as
reserved arrays:

- `{__rhandle, __class, __epoch[, __snapshot]}` — entity lives on the Rust side
- `{__phandle, __class, __implements}` — entity lives in the PHP child
- `{__pclass}` — a PHP class name

An immutable value has no entity to point at, so it crosses in neither table: it travels as the
object record `serialize()` writes for it (`PluginValue::PhpObject` — the class name and the
property table, property names carrying PHP's visibility mangling), and each side rebuilds its
own value from those fields. `unserialize()` revives the PHP one without running a constructor,
which is what makes the two directions equivalent: a value whose state a constructor cannot
express — an unset pretty string, a `Link` built without a pretty constraint — has no faithful
constructor call, and no field has to be read back out through reflection.
`Composer\Package\Link` travels this way, together with the `composer/semver` constraint it
holds, and so does the `\DateTimeInterface` release date, rebased on UTC because the Rust side
carries no timezone database. The two halves are
`crates/shirabe/src/plugin/php_plugin_value.rs` and `Shirabe\MaterializedValue`; the set of
classes that may cross is a closed list on both sides — the PHP one is the `allowed_classes`
list of every `unserialize()` — so a record can never name an arbitrary class.

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
The worker mutex itself is only held per frame, not across a call.

While a Rust-initiated call waits for its `Return`, incoming `CallRustMethod` frames are
dispatched to the caller-supplied `RustMethodDispatcher` (the cooperative loop); with no
dispatcher active they are answered with an explicit `Throw`, never a silent null. Rust handle
0 is reserved for the runtime service endpoint (e.g. `__shirabe_find_file`, which the worker's
script-class autoloader uses to ask the Rust-side `ClassLoader` where a class file lives).

The PHP side mirrors this: its top level is a standing serve loop, and `callRust` drives the
same dispatch while waiting for its own `Return`.

## Failure model

- The outer `anyhow::Result` of `call_function`/`call_static_method` is the fatal lane: dead
  worker (EOF, with the child's exit status attached as context), broken framing, oversized
  frames.
- The inner `Result<PluginValue, PhpThrow>` is the recoverable lane: a PHP exception crossing
  the boundary as a `Throw` frame.
- A frame that decodes to something protocol-invalid is a bug in Shirabe itself (both halves
  ship in the same commit) and panics; the PHP side symmetrically dies so Rust observes EOF.
- The legacy scalar query API (`get_php_version` etc.) keeps its historical contract: every
  failure panics.

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
- `__shirabe_resolved_promise` — wraps a value in `\React\Promise\resolve()`, so a Rust method
  whose PHP signature declares `PromiseInterface` (the `DownloadManager` surface) can answer
  with the object type the caller expects. The Rust future has already run to completion by
  then; deferred resolution across the boundary does not exist yet.
- `__shirabe_settle_promise` — the inverse: drains a promise a plugin returned to Rust. React
  settles synchronously, so an already-settled promise yields its value here (a rejection is
  re-thrown as the Throw reply); one that is still pending is an explicit error.
- `__shirabe_get_property` — for testing only: reads a public property of a P-table entity.
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
the Rust process, against the Rust-side application state — and `__shirabeConstruct`, which
allocates the Rust entity behind a `new SomeProxiedClass(...)` written by plugin code and
answers with `[rhandle, epoch]`. Classes whose entity Rust cannot build are an explicit error
naming the class.

## Proxy stubs and runtime classes

`php/stubs/` holds the proxy stub classes (`Composer\Script\Event`, `Composer\PartialComposer`,
`Composer\Composer`, the `Composer\IO\{BaseIO,ConsoleIO,BufferIO,NullIO}` hierarchy, the
package/repository graph, and `Composer\EventDispatcher\EventDispatcher`). They are generated
by `scripts/plugin-stub-generator/generate-stubs` and must not be edited by hand; see
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
- `Shirabe\MaterializedValue` — the PHP half of the materialized-value codec: the closed list of
  classes `unserialize()` may revive, and the hook that hands such an instance to `serialize()`
  in place of a handle descriptor.
- `Composer\EventDispatcher\Event` — dual-mode: revived from a Rust handle (through
  `__shirabeBind`) it proxies like a generated stub, while a natively-constructed instance (real
  Composer code in the worker does `new PreCommandRunEvent(...)`, whose parent constructor lands
  here) is a faithful in-process port of the real base class and crosses the wire as a P-table
  entity (`__shirabeRustHandleDescriptor()` returns null in native mode).

Both sets are written into the same autoload directory at worker spawn and resolved with
highest priority, so these FQCNs can never be shadowed by the real implementation;
`__shirabe_require` restores that priority after loading code that prepends its own autoloader.
Stubs are interned per rhandle (`WeakReference`-based registry) so identity (`===`) holds, and
their destructors send `ReleaseRustHandle`. Reviving a stub for an existing entity bypasses its
constructor (`newInstanceWithoutConstructor` plus `__shirabeBind`), because the constructor
carries the real class's own signature and belongs to plugin code building a *new* entity. `clone` on a stub calls `__shirabeClone` on the
entity and rebinds the copy to the handle that answers, so the two stubs never share (and never
double-release) one entity; entities with no clone semantics answer with an explicit error.

## The P table

`ShirabePhpObjectRegistry` holds PHP-owned entities (e.g. plugin instances) keyed by phandle,
strongly referenced until the Rust side sends `ReleasePhpHandle`. `toWire` turns any non-stub
object into a `__phandle` descriptor (interned by `spl_object_id`, so one entity keeps one
handle); `fromWire` resolves descriptors back to the live entity. The Rust-side counterpart —
the R table holding `$composer`/`$io` entities reachable from plugin callbacks — lives in
`crates/shirabe/src/plugin/php_plugin_proxy.rs`.

## Out of scope (deferred)

Out-parameter write-back at the call sites, epoch-based cache invalidation on the PHP side,
R-table garbage collection on `ReleaseRustHandle`, error-class reconstruction across the
boundary, and Windows support.
