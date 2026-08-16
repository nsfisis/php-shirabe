# Plugin proxy stub and guard generation

## Purpose

The plugin PHP worker resolves proxied Composer FQCNs to thin stub classes whose
public methods forward to the Rust-side entity over RPC (see
`docs/dev/php-rpc.md`, "Proxy stubs"). These stub files, committed under
`crates/shirabe-php-rpc/php/stubs/` and embedded into the binary by the
`STUB_FILES` list in `crates/shirabe-php-rpc/src/lib.rs`, are *generated* by
`scripts/plugin-stub-generator` — never edited by hand. Generating them from the
Composer sources keeps every stub signature mechanically faithful to the real
class and turns "the stub is missing something the real class has" into a
generation failure instead of silent breakage.

The same tool emits the **guard classes** under
`crates/shirabe-php-rpc/php/guards/`, one for every Rust-owned class no stub and
no `php/runtime/` class shadows. Without them the worker's autoloader falls
through to the real Composer implementation, and code running there works on a
second instance the Rust side never sees — silent breakage in place of the
explicit error an unimplemented plugin API is supposed to raise. `build.rs`
embeds the guards, building the list from the directory itself.

## Running

```
cd scripts/plugin-stub-generator
composer install            # once; vendor/ is git-ignored
./generate-stubs            # rewrites the stubs/ and guards/ directories
./generate-stubs --check    # verifies committed files are fresh; exit 1 otherwise
```

Inputs:

* `targets.list` — the FQCNs to emit, stub base classes before their subclasses.
  Growing the stub set means adding a line here and regenerating.
* the hand-written classes under `crates/shirabe-php-rpc/php/runtime/` (their
  FQCNs derive from the file paths). These are two-world implementations with
  behavior of their own — not mechanical proxies — so the generator never emits
  them, but it accepts them as base classes of generated stubs (computing the
  inherited surface from the real Composer class the runtime file mirrors) and
  fails if a `targets.list` entry would shadow one.
* the Composer checkout (`composer/`, override with `--composer-root=`); the
  generator locates sources through the checkout's own PSR-4 autoload map, so
  interfaces from vendor packages (e.g. `Psr\Log\LoggerInterface`) resolve too.
* the classifier report (`scripts/plugin-class-classifier/report.json`, override
  with `--report=`). Run `scripts/plugin-class-classifier/classify` first; the
  report is git-ignored. Every stub target must be classified `rust-proxy` or
  `contract`, and a report with classification violations is rejected. The guard
  set is read from the report alone: every `rust-proxy`, `rust-snapshot` and
  `unsupported` class that is not a stub target, a `php/runtime/` class or an
  exemption.
* `guard-exemptions.list` — the Rust-owned FQCNs that stay resolvable to the real
  Composer class, each with the worker-side mechanism that makes a native
  instance correct (the materialized-value codec for `Composer\Package\Link`,
  the dual-mode `Composer\EventDispatcher\Event` for
  `Composer\Plugin\PreCommandRunEvent`). An entry that needs no guard anyway
  fails the run.

The freshness check for both sets runs in `cargo test` as
`crates/shirabe-php-rpc/tests/generated_stubs.rs`; it returns early when PHP,
the generator's vendor directory or the classifier report is unavailable.

## What the generator emits

* **Root stubs** (targets whose parent class is not itself a target) carry the
  proxy boilerplate: `__rhandle`/`__epoch` properties, the `__shirabeBind`
  binder the registry calls when reviving a stub for an existing entity, a
  destructor releasing the Rust handle, and the wire descriptor helper. The
  real `extends`/`implements` hierarchy is preserved and `\ShirabeRustStub` is
  appended to the interface list.
* **Constructors** reproduce the real class's parameter list and forward to the
  Rust side (`__shirabeConstruct` on handle 0), which allocates the entity and
  answers with its handle; a class Rust cannot build answers with an explicit
  error naming it. One is emitted for every root stub and for every subclass
  that declares a public constructor of its own, so `new SomeProxiedClass(...)`
  in plugin code never yields an unbound stub. Proxy revival does not run them
  (see `__shirabeBind` above).
* **Instance methods** forward via `\ShirabeRpcRuntime::callRust`. For a root
  stub the emitted surface is the interface closure (each interface before the
  ones it extends, methods in declaration order; a concrete redeclaration in
  the class wins over the interface signature) followed by the class's own
  remaining public methods. A subclass stub declares only the methods whose
  *name* is new relative to the inherited stub surface — including methods of
  interfaces the subclass adds; an omitted override must match the inherited
  parameter list (names, arity, defaults, passing modes — type declarations
  may differ), otherwise generation fails. PHP builtin interfaces
  (`Countable`, `Stringable`, ...) contribute no closure entries of their own;
  the class's own public methods already cover their surface.
* Methods returning `self`/`static` perform the RPC and then `return $this;`
  to preserve identity instead of round-tripping the handle.
* **Class constants, static methods and public static properties** are
  materialized verbatim from the real source (they read no instance state and
  run locally in the worker), together with any non-public static helpers the
  methods call. Constants keep their declared visibility, so a non-public one
  stays unreadable from outside the stub as it is in the real class.
* **Instance properties** are not declared on the stub, whatever their
  visibility: they are entity state. Every root stub instead carries
  `__get`/`__set`/`__isset`/`__unset` forwarders, so each access reaches the
  Rust side, where an unsupported name is an explicit error. They are emitted
  unconditionally because PHP's own answer for an undeclared property — null on
  a read, a dynamic property on a write, false on `isset()` — is silent.
* **`__toString`** is forwarded like any other method. **`__clone`** is part of
  the boilerplate on every stub, whether or not the real class declares one:
  PHP has already copied the stub by the time it runs, so the copy asks the
  Rust side for a clone of the entity and rebinds itself to the fresh handle
  (`__shirabeClone`, answered with `[rhandle, epoch]`). The clone semantics of
  the real class live on the Rust side with the entity; entities that model no
  clone answer with an explicit error.
* **Imports**: the original file's `use` statements are kept in their original
  order, restricted to names the emitted stub references; signatures declared
  elsewhere (interface files) are re-spelled through that import table.

## What a guard looks like

A guard reproduces the real class's declaration — `abstract`/`final`, `extends`,
`implements`, the constants and public static properties verbatim — so that
references satisfied by the declaration alone keep working: `instanceof`,
`catch`, `X::class`, `Link::TYPE_REQUIRE`. Everything executable raises
`ShirabeUnsupportedClass::fail()`, which names the class and the member:

* the constructor (the class's own, or the one it would inherit; a class whose
  hierarchy declares none gets a no-argument one, so the implicit constructor
  cannot be reached either),
* every public and protected method the guard would otherwise inherit from real
  code: its own, its traits', and those of ancestors up to the first one that is
  itself guarded or shadowed by a stub. Private methods are reachable only from
  the code the guard replaces, and a destructor that throws would fire during
  unwinding, so neither is emitted.

Abstract methods of an abstract guard stay abstract. A method an ancestor
declares `final` cannot be redeclared, so it keeps running the real
implementation; the guard's header names each one.

## Coverage assertions

Generation fails — instead of emitting something quietly wrong — on:

* a target missing from the classifier report, classified other than
  `rust-proxy`/`contract`, or a report carrying violations,
* by-ref or variadic parameters (in constructors too), static interface
  methods, magic methods other than `__toString`/`__clone`,
* an omitted override diverging from the inherited stub signature,
* a subclass target listed before its base class, or extending a class that is
  neither a target nor provided by `php/runtime/`,
* a target whose FQCN is also provided by `php/runtime/`.

`generate-stubs` (in both modes) additionally fails when a `.php` file exists
under the stubs or guards directory that no target produces, or when `STUB_FILES` /
`RUNTIME_FILES` in `crates/shirabe-php-rpc/src/lib.rs` does not embed every
generated stub / runtime file. It also cross-checks the handoff property table
for `Composer\Console\Application` (declared in `generate-stubs` itself) against
the real class: a property upstream adds without a handoff classification — or a
table row the class no longer declares — fails generation, so the worker-side
runtime application can never silently drop plugin-visible state after a
Composer version bump.

When a future Composer release adds a public member the emitter cannot handle,
these assertions surface it at generation time; extending the emitter (or
deciding the porting policy) is then an explicit step, mirroring the
completeness stance of `docs/dev/plugin-class-classification.md`.
