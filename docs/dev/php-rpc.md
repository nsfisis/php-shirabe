# PHP RPC

Composer can require a specific PHP version or loaded extensions, i.e., platform requirements,
and its plugin/script machinery executes real PHP code. To mimic this behavior needs a real PHP
runtime.

The `shirabe-php-rpc` crate spawns the system PHP as a child process and talks to it over a Unix
domain socket. There is exactly one child process per Shirabe process, shared by every caller;
it hosts both the simple runtime queries (`get_php_version`, `has_constant`, ...) and the plugin
protocol.

## Locating PHP

The existing `PhpExecutableFinder` class resolves the PHP binary. The child is started with
`-d serialize_precision=-1` so the wire codec's float formatting is pinned to the default PHP
behavior.

## Transport

- A Unix domain socket (no Windows support for now), bound in a `0700` temp dir with the socket
  file itself chmodded to `0600`.
- The PHP glue code (`php/worker.php`) and the proxy stub classes (`php/stubs/`) are embedded in
  the Rust binary and written to the temp dir at spawn time, so both halves of the protocol are
  always the same commit.

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
losslessly), lists, ordered maps, and three handle descriptor kinds encoded as reserved arrays:

- `{__rhandle, __class, __epoch[, __snapshot]}` — entity lives on the Rust side
- `{__phandle, __class, __implements}` — entity lives in the PHP child
- `{__pclass}` — a PHP class name

`PluginValue::Object` is encode-only: the wire erases the array/object distinction and object
revival is banned (`unserialize(..., ['allowed_classes' => false])` is enforced on the PHP
side), so the decoder only produces `List` (contiguous 0-based int keys) or `Array`. The
decoder is iterative (input nesting never becomes call-stack depth) and additionally rejects
payloads nested deeper than 512 levels.

The codec is verified against the real PHP `serialize()`/`unserialize()` by oracle tests
(`tests/oracle.rs`), with floats, non-UTF-8 byte strings and deep nesting as focus areas.

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

`CallFunction` first consults the worker's dispatch table (composite queries like `diagnose`,
Shirabe-internal helpers prefixed `__shirabe_`), then falls back to calling the named PHP
function; an unknown name is an explicit error. Notable internal helpers:

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
- `__shirabe_installed_versions_reload` — mirrors `FilesystemRepository::write`'s in-process
  `InstalledVersions::reload($versions)` into the worker; guarded by
  `class_exists(..., false)` so an unloaded class keeps its upstream lazy-load behavior.
- `__shirabe_get_property` — for testing only: reads a public property of a P-table entity.
- `__shirabe_oracle_roundtrip` — codec oracle support for tests.

## Proxy stubs

`php/stubs/` holds hand-written proxy stub classes (`Composer\EventDispatcher\Event`,
`Composer\Script\Event`, `Composer\PartialComposer`, `Composer\Composer`, and the
`Composer\IO\{BaseIO,ConsoleIO,BufferIO,NullIO}` hierarchy), written in the shape the future
stub generator will output. They are autoloaded with highest priority so a proxied FQCN can
never be shadowed by the real implementation; `__shirabe_require` restores that priority after
loading code that prepends its own autoloader. Stubs are interned per rhandle
(`WeakReference`-based registry) so identity (`===`) holds, and their destructors send
`ReleaseRustHandle`.

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
