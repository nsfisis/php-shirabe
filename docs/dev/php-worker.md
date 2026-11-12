# PHP worker runtime

The RPC protocol ([docs/dev/php-rpc.md](./php-rpc.md)) carries calls, values and handles between
the Rust process and the PHP worker without knowing any class or function by name. This document
covers what Shirabe builds on top of it: the named entry points each side answers, the PHP classes
the worker loads, the classes that are sent as materialized values, and how the console application
is split between the two worlds. Which treatment each Composer class receives is decided by
[docs/dev/plugin-class-classification.md](./plugin-class-classification.md).

Everything Shirabe itself defines in the worker is internal: the classes in the `Shirabe\`
namespace, the global classes and interfaces whose names start with `Shirabe` (such as
`ShirabeRpcRuntime`), and the members whose names start with `__shirabe` or `__rhandle`. Plugin
code can see them, but they are not meant to be used from it, and they may change in any release
without notice.

TODO: Move the above notice somewhere else. It is for plugins authors, but this document is for Shirabe core developer.

## Entry points

A `CallFunction` name reaches the worker's dispatch table first (see "Entry points" in
[docs/dev/php-rpc.md](./php-rpc.md)). The table holds two composite queries,
`__shirabe_diagnose` and `__shirabe_platform`, the runtime queries `__shirabe_constant`,
`__shirabe_get_all_ini_files` and `__shirabe_xdebug_active`, and the Shirabe-internal helpers.
Every entry is named `__shirabe_<snake_case>`, so a name in the table never shadows a PHP
function.

A composite query answers everything one consumer needs about the runtime in a single round trip,
because asking one constant and one extension at a time costs a round trip each. The Rust side
sends the names to report on (the extensions, functions and ini settings for
`__shirabe_diagnose`; the constants, classes and extension info for `__shirabe_platform`) with the
query, so the lists exist only there, and decodes the answer into a cached struct (`Diagnostics`
for `__shirabe_diagnose`, `PlatformInfo` for `__shirabe_platform`).

### Records

A composite payload — the answer of a composite query, the console application handoff, a command
definition read back, the facts of a terminated process, the result of a `scripts` Command class —
is a PHP array whose keys are the field names of one Rust struct declared with
`shirabe_php_rpc::plugin_value_record!`, which derives its `ToPluginValue` / `FromPluginValue` from
those fields. The worker code reading or writing such an array uses the same snake_case names.

Notable internal helpers:

- `__shirabe_eval` — runs a Rust-generated PHP snippet and returns its `return` value (used by
  the `scripts` Command-class execution path and the `_composer_tmp` class-rename path of
  `PluginManager::registerPackage`).
- `__shirabe_require` — `require_once` a file (e.g. an autoloader) into the worker, then
  re-prepends the stub autoloader so proxied FQCNs keep resolving to stubs even when the
  required file registered its own prepending autoloader (a Composer `vendor/autoload.php`
  does).
- `__shirabe_enable_script_autoloader` — registers the autoloader that resolves classes through
  the Rust-side `ClassLoader`s on the autoload stack via handle 0.
- `__shirabe_composer_require` — the body of `\Composer\Autoload\composerRequire`, sharing
  its `$GLOBALS['__composer_autoload_files']` guard (files-autoload entries of plugin
  packages).
- `__shirabe_installed_versions_reload` — mirrors the tail of `FilesystemRepository::write`
  (the unconditional `InstalledVersions::reload($versions)` plus the reflection-based
  `selfDir`/`installedIsLocalDir` restore) into the worker.
- `__shirabe_resolve_job` / `__shirabe_reject_job` — the settle pushes of a Rust-owned job: the
  worker resolves or rejects the `Deferred` behind the job's promise, which runs the callbacks
  plugin code chained on it before the call returns.
- `__shirabe_resolved_promise` — wraps a value in `\React\Promise\resolve()`, for a Rust method
  whose PHP signature declares `PromiseInterface` and that has the value before any job exists.
  The worker holds the promise under an even job id and answers with its `Job` descriptor,
  which the Rust method passes on as its own answer.
- `__shirabe_rejected_promise` — the failure half of the same: `\React\Promise\reject()` over
  the exception such a description stands for, so a failed request reaches the caller
  as a rejection it handles rather than as a throw out of the call that started it.
- `__shirabe_sync_env` — replays the environment writes the Rust side made after the worker was
  spawned into the storage each one names (`putenv()`, `$_ENV`, `$_SERVER`); see
  [docs/dev/env-vars-porting.md](./env-vars-porting.md). The outermost `rpc_call` of a session
  issues it whenever the journal has moved on.
- `__shirabe_console_application_boot` — builds the worker-side `Composer\Console\Application`
  (the `php/runtime/` definition) from the Rust handoff: the shared `$composer`/`$io` proxies,
  the initial working directory, the `disablePluginsByDefault`/`disableScriptsByDefault` flags,
  `\Shirabe\RustCommandStub` rows mirroring the built-in commands, and the live plugin-provided
  command entities.
- `__shirabe_run_console_application` — runs one stringified command line through a booted
  worker-side application; output goes to the inherited stdio, the exit code returns over the
  wire, and a failure propagates as a Throw (the booted application does not catch exceptions).
- `__shirabe_read_command_definition` — reads a command's input definition (plus help text and
  extra usages) as plain data, so the Rust side mirrors it for `help`/`list` rendering.

The helpers that exist only for tests sit together at the end of the table, each commented
`For testing:`:

- `__shirabe_oracle_roundtrip` — codec oracle support.
- `__shirabe_get_property` — reads a public property of a P-table entity.
- `__shirabe_reflection_invoke` / `__shirabe_reflection_get_property` /
  `__shirabe_reflection_set_property` — the `ReflectionMethod` / `ReflectionProperty` access
  PHPUnit tests use to reach the non-public members of a P-table entity.

### Runtime service endpoint

The runtime service endpoint (handle 0) answers `__shirabe_find_file`, which the worker's
script-class autoloader uses to ask the Rust-side `ClassLoader`s on the autoload stack where a
class file lives,
`__shirabe_run_rust_command` — the reverse half of the two-world command split: a
`\Shirabe\RustCommandStub` forwards its stringified input here and the built-in command runs in
the Rust process, against the Rust-side application state — `__shirabe_construct`, which
allocates the Rust entity behind a `new SomeProxiedClass(...)` written by plugin code and
answers with its R-handle, `__shirabe_call_static`, which takes `[class, method, args]`
and runs a static method no handle identifies a receiver for, and the job methods
`__shirabe_resolve_job` / `__shirabe_reject_job` / `__shirabe_cancel_job` / `__shirabe_abort_jobs`
([docs/dev/plugin-async.md](./plugin-async.md)).

## Materialized values

The two halves of the materialized-value codec are the `ToPluginValue` / `FromPluginValue` impls of
`php_worker::codec` (and, for the PHP built-in `\DateTimeInterface`, the `ToPluginValue` / `FromPluginValue` impls of
`shirabe-php-rpc`) and `Shirabe\MaterializedValue`, whose `CLASSES` constant is the closed list of
classes that may be sent as object records.

`Composer\Package\Link` travels this way, together with the `composer/semver` constraint it holds
(a `Link` can be built without a pretty constraint, which no constructor call reproduces), and so
does the `\DateTimeInterface` release date, rebased on UTC because the Rust side carries no
timezone database. So does the `Composer\Util\Http\Response` that `HttpDownloader` answers with,
and the `Composer\ClassMapGenerator\ClassMap` that `AutoloadGenerator::dump()` returns: the Rust
side keeps neither after answering, so the child owns the revived instance outright.

A constraint is encoded structurally: `shirabe_semver::AnyConstraint` maps to and from the real
`Composer\Semver\Constraint\*` objects, never through a constraint string, so the pretty string
and the conjunctive flag survive.

## Proxy stubs, runtime classes and guards

`php/stubs/` holds the proxy stub classes for the FQCNs listed in
`scripts/plugin-stub-generator/targets.list`. They are generated by
`scripts/plugin-stub-generator/generate-stubs` and must not be edited by hand; see
[docs/dev/plugin-stub-generation.md](./plugin-stub-generation.md).

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
- `Shirabe\MaterializedThrowable` — the PHP half of the exception codec: rebuilds the exception
  a description (the payload of a `Throw` frame, a job rejection) stands for as its own class, and
  describes an exception raised in the child.
- `Shirabe\MaterializedValue` — the PHP half of the materialized-value codec: the closed list of
  classes `unserialize()` may revive beside the descriptor classes, and the hook that hands such
  an instance to `serialize()` in place of a handle descriptor.
- `Shirabe\Wire\RustHandle`, `Shirabe\Wire\PhpHandle` and `Shirabe\Wire\Job` — the descriptor
  records (see "Values" in [docs/dev/php-rpc.md](./php-rpc.md)).
- `Composer\EventDispatcher\Event` — dual-mode: revived from a Rust handle (through
  `__shirabeBind`) it proxies like a generated stub, while a natively-constructed instance (real
  Composer code in the worker does `new PreCommandRunEvent(...)`, whose parent constructor lands
  here) is a faithful in-process port of the real base class and is sent as a P-table
  entity.
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
Stubs are interned per R-handle so identity (`===`) holds, and
their destructors send `ReleaseRustHandle`. Reviving a stub for an existing entity bypasses its
constructor (`newInstanceWithoutConstructor` plus `__shirabeBind`), because the constructor
carries the real class's own signature and belongs to plugin code building a *new* entity.
`clone` on a stub calls `__shirabe_clone` on the entity and rebinds the copy to the handle that
answers, so the two stubs never share one entity.

## The R-table

`R_TABLE` holds the Rust-side entities. There is
no per-plugin bookkeeping: an entry lives as long as a stub for it is reachable in the worker,
which is also how long Composer keeps an object a plugin holds on to.

A kind of entity is a handle type (`Rc<RefCell<dyn LockerInterface>>`, `PackageInterfaceHandle`,
...) implementing `Entity`: the allocation that
is its identity, the PHP class it appears as in the worker, and optionally its clone and property semantics. An
entity is handed to the child only through `entity_value`, which registers it and builds the Rust
handle descriptor from the class `Entity::class` chose, and is read back as `EntityArg<H>`.

The table interns by address, so the same shared instance is always sent as the same R-handle
(`===` in the child). The class is fixed when the entity is registered, as the child fixes the class of the stub
it builds for an R-handle.

### Forwarding classes

Every proxy stub, runtime class and guard that forwards members to the Rust side has a module in
`php_worker::rpc::forwarding`, generated by `surface-manifest` (see
[docs/dev/plugin-stub-generation.md](./plugin-stub-generation.md)) and laid out by the class's namespace (`Composer\Package\Locker` is
`forwarding::package::locker`). The module holds:

- `PhpClass`, a marker type standing for the class;
- `Forwarding`, a trait with one method per member the class itself declares and forwards, whose
  default answers that the member is not available. The trait of a class whose parent forwards
  nothing names the handle type (`type Handle`) every descendant takes;
- `dispatch`, routing a method name to the `Forwarding` of the class declaring it, and `CROSSING`,
  the class and its `dispatch` as the value `Entity::class` returns;
- `construct` and `call_static` for a class forwarding its constructor or static methods, which
  the top-level `construct` and `call_static` route `__shirabe_construct` and
  `__shirabe_call_static` to.

What the Rust side answers is written by hand, as `impl forwarding::<path>::Forwarding for
forwarding::<path>::PhpClass` in the submodules of `php_worker::rpc::entities`. Only the members
the impl overrides are answered; the rest keep the generated default, and the surface manifest
records which is which. An impl reads its arguments with `FromPluginValue`. A value that is absent
where PHP would read null, such as an omitted argument or a missing property, decodes from
`PluginValue::Null`.

To serve a new class: add it to the stub generator's `targets.list`, regenerate the stubs and the
manifest, and write its forwarding impl. A new kind of entity also needs an `Entity` impl for its
handle type; a new class of an existing kind needs a branch in that impl's `class`. A missing impl
does not compile.

## Two-world console application

Built-in commands run on the Rust side, against the Rust-side `Application`. Commands whose
implementation lives in the worker — those a plugin's `CommandProvider` supplies, and `scripts`
entries naming a Symfony `Command` subclass — run under the worker-side
`Composer\Console\Application`. Each command always runs in the world that holds its
implementation, against that world's `HelperSet`, and a call into the other world hands the whole `run()` to the
other side as a command line that the other side parses again:

- Rust to PHP, the proxy of a worker-side command sends its input stringified, and the worker runs
  it through `__shirabe_run_console_application`.
- PHP to Rust, a `\Shirabe\RustCommandStub` forwards `(string) $input` to
  `__shirabe_run_rust_command`, which parses it with `StringInput`.

The handoff (`__shirabe_console_application_boot`) carries what the parsed command line cannot: the
`$composer`/`$io` proxies, the initial working directory and the
`disablePluginsByDefault`/`disableScriptsByDefault` flags. The verbosity, decoration and interaction
flags travel in the command line itself, and the worker, spawned after the Rust side has configured
its IO, inherits the environment and the standard streams, so both worlds see the same terminal.

Whether a `scripts` entry names a `Command` subclass is decided in the worker, which first needs
the Composer PHP runtime loaded (the class extends Symfony's `Command`). The question is only asked
for a class the Rust-side `ClassLoader`s resolve to a file, so plain shell-command scripts never
load the runtime.
