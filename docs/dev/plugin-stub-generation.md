# Plugin proxy stub generation

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

## Running

```
cd scripts/plugin-stub-generator
composer install            # once; vendor/ is git-ignored
./generate-stubs            # rewrites crates/shirabe-php-rpc/php/stubs/
./generate-stubs --check    # verifies committed stubs are fresh; exit 1 otherwise
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
  report is git-ignored. Every target must be classified `rust-proxy` or
  `contract`, and a report with classification violations is rejected.

The freshness check runs in `cargo test` as
`crates/shirabe-php-rpc/tests/generated_stubs.rs`; it returns early when PHP,
the generator's vendor directory or the classifier report is unavailable.

## What the generator emits

* **Root stubs** (targets whose parent class is not itself a target) carry the
  proxy boilerplate: `__rhandle`/`__epoch` properties, a constructor that
  accepts `(rhandle, epoch)` from proxy instantiation and throws a diagnosable
  `RuntimeException` when plugin code tries to `new` the class directly, a
  destructor releasing the Rust handle, and the wire descriptor helper. The
  real `extends`/`implements` hierarchy is preserved and `\ShirabeRustStub` is
  appended to the interface list.
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
  methods call.
* **Public instance properties** are not declared on the stub; `__get`/`__set`
  forwarders carry every access (including dynamic-property writes) to the
  Rust side, where an unsupported name is an explicit error.
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

## Coverage assertions

Generation fails — instead of emitting something quietly wrong — on:

* a target missing from the classifier report, classified other than
  `rust-proxy`/`contract`, or a report carrying violations,
* by-ref or variadic parameters, static interface methods, magic methods
  other than `__toString`/`__clone`,
* an omitted override diverging from the inherited stub signature,
* a subclass target listed before its base class, or extending a class that is
  neither a target nor provided by `php/runtime/`,
* a target whose FQCN is also provided by `php/runtime/`,
* non-public class constants (materializing them is unsupported so far).

`generate-stubs` (in both modes) additionally fails when a `.php` file exists
under the stubs directory that no target produces, or when `STUB_FILES` /
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
