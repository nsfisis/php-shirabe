# Plugin boundary class classification

## Purpose

Shirabe's plugin mechanism runs Composer plugins as real PHP code in a child
process, connected to the Rust core by a bidirectional RPC channel. The
entity of each object is fixed to one side: the Composer object graph lives
in Rust and is proxied into the child process as thin same-FQCN stub
classes, while code that shares no state with the graph is loaded into the
child as the real PHP implementation. This document defines, for *every*
class in composer/composer and its vendor dependencies, which treatment it
receives — and defines the decision as a deterministic procedure, so that
new or rewritten classes in future Composer releases are classified the same
way without a human re-deriving the design.

The classification decides three practical things per class:

* what the stub generator must emit for the PHP child process (proxy stub,
  snapshot class, real source passthrough, or nothing),
* what the Rust side must reproduce with full fidelity (method set, argument
  order, class hierarchy, subclassing behavior) versus where internal
  refactoring is free,
* which classes need a reverse adapter so that plugin-provided objects can be
  called back from Rust.

A small, versioned exception list is permitted. Everything not on it must be
decided by the rules below, from the PHP sources alone.

## Classification output

### Categories

Every class or interface that can appear at the plugin boundary is assigned
exactly one category.

| Category | Entity lives | PHP child process sees | Rust obligation |
|---|---|---|---|
| `rust-proxy` | Rust | generated proxy stub (methods RPC to Rust) | full-fidelity reproduction; every public/protected method needs an RPC handler |
| `rust-snapshot` | Rust | generated snapshot class (`__rhandle` + eagerly copied fields, getters answer locally) | full-fidelity reproduction; snapshot serializer |
| `contract` | n/a (interface / abstract type) | generated declaration preserving the `extends`/`implements` hierarchy | depends on direction attributes |
| `two-world` | both, independent siblings | the real PHP implementation (same FQCN, re-defined or vendor-loaded) | independent Rust implementation; only the seam objects (composer / io / dispatcher) are shared |
| `php-native` | PHP | the real, unmodified PHP source | none — Rust may or may not have its own port for internal use, and that port is free to diverge in shape |
| `unsupported` | n/a | nothing; any reference raises an explicit error | none, until explicitly promoted |

#### rust-proxy

The living services: `Composer`, `Config`, `RepositoryManager`,
`InstallationManager`, `EventDispatcher`, `Locker`, `PluginManager`,
`DownloadManager`, `ArchiveManager`, `Loop`, `HttpDownloader`, the
`IOInterface` implementations, and the Event objects passed to listeners.
State sharing is their essence; every method call round-trips to the Rust
entity.

#### rust-snapshot

Immutable value objects, e.g. `Link` and the security-advisory family. The
child receives the field values together with an interned `__rhandle`, so
identity (`===`) is preserved while getters answer locally with zero
round-trips.

#### contract

Interfaces and abstract classes themselves; concrete classes get one of the
other categories. A contract's *direction* decides the artifacts: a
`provided` contract (instances flow Rust→PHP) needs the PHP declaration so
`instanceof` works; a `consumed` contract (plugins implement it and Composer
calls it back: `PluginInterface`, `EventSubscriberInterface`,
`InstallerInterface`, `Capability` and its descendants) additionally needs a
Rust-side reverse adapter that wraps a PHP object handle and implements the
corresponding Rust trait. Contracts can be both (`InstallerInterface` is
registered by plugins *and* returned by
`InstallationManager::getInstaller()`).

For abstract *classes* with concrete state and bodies (`BaseIO`,
`BasePackage`, `LibraryInstaller`) a declaration-only stub is not enough:
real plugins subclass them relying on the inherited behavior, so the stub
must carry the concrete members (proxy-dispatched or operating on snapshot
state) — the per-method rows in the report cover exactly these members.

#### two-world

An exception-list category (it cannot be inferred from the sources):
`Composer\Console\*`, `Composer\Command\*`, and the whole symfony/console
package. Each world runs its own full implementation; instances do not cross
the boundary.

#### php-native

Composer-plugin-api's pure type definitions, the stateless utility classes
(`TlsHelper`, `Platform\Version`, `ClassMapGenerator`, …), exception
classes, constants-only classes, the state-decoupled vendor packages
(composer/pcre, composer/semver, seld/jsonlint, justinrainbow/json-schema,
…) — and *reachable* classes that are stateless and pure (`VersionParser`,
`Auditor`, `Platform\Runtime`): with no instance state to share, the real
code answers identically in both worlds, and a snapshot would have nothing
to copy.

#### unsupported

Not a design failure; it is the no-silent-accuracy-tradeoff principle
applied to classes nobody has needed yet. Plugins touching them get an
explicit error, and the class can be promoted later.

### Attributes

Categories alone are not enough for the stub generator. The classifier also
emits per-class and per-method attributes.

#### direction

`provided` / `consumed` / both: whether instances flow Rust→PHP (return
positions, event getters, callback arguments) or PHP→Rust (parameter
positions of methods on shared objects, plugin-implemented contracts).
Drives which side needs stubs and which needs adapters.

#### plugin-constructible

A non-abstract class with a public constructor that is also reachable from
the graph (e.g. `JsonFile`, obtainable via `Locker::getJsonFile()` *and*
freely `new`ed by plugins). These need a constructor story on the stub (the
stub ctor must RPC a `NewObject` so the entity is allocated Rust-side); the
classifier surfaces them because they are individually design-sensitive.

#### mutable-static

The class writes to `static` properties. Sub-classified by the disposition
list (see exception lists): `memo-cache` (pure memoization, each world may
compute its own: `Git::$version`, `Platform::$isDocker`, …), `seed-once`
(copied from the Rust side once at child startup:
`ProcessExecutor::$timeout`, which Composer seeds from config),
`needs-sync` (genuinely shared process state: `Platform`'s env table), or
`needs-review` (default for newly appearing ones — the run fails loudly
until a human files it).

#### throwable

Subclasses of `\Throwable`. These always need a real PHP class definition in
the child (so `catch`/`instanceof` work) regardless of category, and a wire
mapping for the `Throw` message.

#### pure/mutator (per method)

Drives epoch invalidation of the child-side getter caches: after a mutator
runs Rust-side, the affected proxies' caches must be dropped. A method is a
mutator iff it assigns to `$this->…` (directly, via compound assignment,
`unset`, increment), passes a `$this`-rooted expression into a by-reference
parameter position (builtin signatures resolved from
jetbrains/phpstorm-stubs; `&` read syntactically for user-land signatures),
or transitively calls a mutator on the same object. Statically unresolvable
calls (dynamic method names, `call_user_func` and other
callback-forwarding builtins receiving `$this`) are conservatively
mutators.

#### by-ref parameters

Positions of `&$param` in public methods (the protocol's out-parameter
positions).

#### callable parameters

Positions whose native type is `callable` or `\Closure`
(`IOInterface::askAndValidate`, promise callbacks). These are the places
the callback-handle machinery must cover; the stub generator gets an
explicit signal instead of discovering them at runtime.

#### public static properties

On proxied/snapshotted classes (`FileDownloader::$downloadMetadata`,
`BasePackage::$stabilities`). PHP has no `__getStatic`, so a stub cannot
intercept static property access — each one needs an explicit decision
(materialize as a constant initializer when immutable, push-sync or
explicit-error when mutable).

## The decision procedure

The classifier runs the following pipeline. Every step is deterministic; the
only free inputs are the versioned exception lists.

### Inputs

The parsed sources of `composer/src/Composer`, plus the vendor packages
named in composer/composer's `require`. Vendor packages are first classified
as a whole (see the vendor rule under "Unreachable types"); only
symfony/console and react/promise need class-level treatment.
`Composer\PHPStan\*` is excluded entirely: dev-only tooling, never shipped
at runtime.

### Seed set

The boundary starts where Composer hands objects to plugin code:

* every type declared under `Composer\Plugin\` (the plugin API namespace),
* every subclass of `Composer\EventDispatcher\Event`,
* `Composer\EventDispatcher\EventSubscriberInterface`.

Nothing else is seeded by hand: `Composer` itself enters through
`PluginInterface::activate(Composer, IOInterface)`, `BaseCommand` through
`CommandProvider::getCommands()`, `InstallerInterface` through
`InstallationManager::addInstaller()` once `InstallationManager` is reached,
and so on.

### Reachability closure

For every type `T` in the set, add:

* class-typed native parameter and return types of `T`'s public and protected
  methods (including inherited ones),
* element types from `@param` / `@return` / `@var` docblocks where the native
  type is `array`, `iterable`, `mixed`, `object`, or absent (Composer's
  PHPStan-checked docblocks make this reliable),
* types of `T`'s public and protected properties (a plugin subclassing
  `LibraryInstaller` sees `$this->downloadManager`),
* `@throws` types,
* `T`'s ancestors (parent classes and interfaces),
* when `T` is an interface or abstract class: every concrete subtype declared
  in the inputs (any of them can be the runtime instance behind the
  abstraction).

Iterate to a fixed point. Private members and method bodies do not extend the
closure — the boundary is the declared API surface, not the implementation.
Two-world classes do not extend it either: the child process carries their
real implementation wholesale, so a `Command` subclass's protected fields are
world-2-local, not graph seams.

### Direction marking

During the closure, propagate direction: on a `provided` type, plugins call
the methods — parameter types become `consumed`, return/throws types become
`provided`. The inverse expansion (parameters `provided`, returns
`consumed`) models Composer calling the plugin's implementation, and
therefore applies **only to plugin-implementable types** (interfaces and
abstract classes) marked `consumed`. A concrete class also gains a
`consumed` mark when it appears in a parameter position (a plugin can
construct one and pass it in), but its method bodies are still Composer's
own, so it always expands provided-style. Without this restriction nearly
every direction degenerates to `both` through self-feedback.

### Categories for reachable types

Assigned in order:

1. On the two-world exception list → `two-world`.
2. Subclass of `\Throwable` → `php-native` with the `throwable` attribute
   (exception classes are flat data; the child gets real definitions, the
   wire carries them by value).
3. Interface or abstract class → `contract` (+ direction attributes). This
   outranks the constants-only rule below: a marker interface such as
   `Capability` still needs direction attributes.
4. Constants-only class (no methods, no properties, no hierarchy:
   `ScriptEvents`, `PluginEvents`, …) → `php-native`; the definition is
   pure data.
5. Belongs to a vendor package classified `php-native` as a whole → the
   type is `php-native` (its appearance in signatures does not force a proxy;
   instances are plain values or PHP-local objects). react/promise is the
   exception: `PromiseInterface` is a `contract` bridged to Rust promises.
6. Stateless pure class → `php-native`: no instance property anywhere in
   the hierarchy, every public/protected instance method `pure` per the
   purity analysis, and the method bodies are locally satisfiable (they
   construct nothing that will be a proxied service). With no state to
   share there is nothing to proxy or snapshot; the real code, run against
   injected proxies, behaves identically in both worlds. This catches
   `VersionParser`, `Auditor`, `Platform\Runtime`, `NoopInstaller`.
7. Value-object test → `rust-snapshot`: same purity and body-locality
   conditions, plus (a) at least one instance property (something to
   copy), and (b) no property — including inherited ones — and no
   constructor parameter typed as a blocking type (a reachable type that is
   not itself a candidate, a throwable, or a php-native vendor type). The
   candidate sets grow as one fixed point, and after the unreachable pass
   below, candidates whose bodies reference `unsupported` classes are
   demoted and everything reruns until stable. This is a strict *immutable
   value object* detector: it finds `Link` and the advisory family — but
   not `Package`/`CompletePackage`, which carry setters and a
   `RepositoryInterface` back-reference (see "Known deviations").
8. Everything else → `rust-proxy`.

### Unreachable types

Classes never touched by the closure are not part of the shared graph, but
plugins may still reference them (`new Composer\Util\Filesystem()`,
`JsonFile::parseJson()`, any vendor helper). The rule is a leaf-first fixed
point over *hard* body references — `new X`, `X::method()`, writes to
`X::$prop` — while `instanceof`, `catch`, and `X::class` are satisfied by a
mere declaration and never demote.

What a hard reference may legally target from real PHP running in the child
process:

* `X::method()` works when X has any executable presence there: a php-native
  class, real vendor code, real console code (two-world), or a generated
  stub — proxy stubs carry static methods as RPC forwarders, which is what
  makes the ubiquitous `Platform::getEnv()` call sites loadable. Only
  `unsupported` peers and unknown types demote.
* `new X` additionally requires local constructibility: php-native,
  two-world, vendor, and builtin classes are real code; `rust-snapshot`
  values may be built locally (they are values — they become Rust-backed
  when they cross the boundary). Constructing a `rust-proxy` service is the
  unresolved dual-instantiation case and demotes, explicitly and visibly.
* a `needs-sync` static disposition (and an unfiled one) also demotes.

A class every hard reference of which passes is `php-native`; otherwise it
is `unsupported` — the class embeds orchestration over shared state (e.g.
`Composer\Installer`, `Factory`, the solver), and silently running the real
PHP implementation against proxies would fork the state the Rust side
believes it owns. Explicit error until a human decides. Demotions cascade,
and every demotion records its concrete reason in the report. Classes with
an `overrides.list` entry take their category from the override and do not
participate in the fixed point.

Vendor packages are classified wholesale by the same criterion applied
package-level: a package is `php-native` if no class in it references
composer/composer types or shared static state (true for composer/pcre,
composer/semver, seld/jsonlint, justinrainbow/json-schema,
composer/ca-bundle, composer/spdx-licenses, composer/metadata-minifier,
composer/class-map-generator, composer/xdebug-handler, seld/signal-handler,
psr/log, symfony/filesystem, symfony/finder, symfony/process,
seld/phar-utils, the polyfills); symfony/console is `two-world`;
react/promise ships as real code while `PromiseInterface` is bridged.

### Exception lists

Three, all versioned next to the tool, all expected to stay short:

* `two-world.list` — `Composer\Console\*`, `Composer\Command\*`,
  symfony/console.
* `static-state.list` — disposition per mutable-static class
  (`memo-cache` / `seed-once` / `needs-sync`); anything not listed fails
  the run.
* `overrides.list` — per-class category corrections. Every entry must carry
  a reason. Initial content: `Platform` → `rust-proxy` (its env table is
  shared state; the child's Platform stub RPCs env access so both worlds
  see the same environment), the bootstrap classes (see "Known
  deviations"), and two phantom-reachability corrections (`HhvmDetector`,
  `VersionGuesser`).

### Failure mode

When the rules cannot decide (a new mutable-static class, a docblock the
type extractor cannot parse in a position that matters, a class whose
category changed between Composer versions), the classifier fails the run
and names the class. It never silently defaults — the
default-to-`unsupported` rule for unreachable classes is itself an explicit,
reviewable outcome in the report, not a silent guess.

## Known deviations and open questions

The mechanical rules surfaced several points where earlier design prose was
incomplete or a decision is still owed. Each needs an explicit user
decision; the tool keeps them visible instead of resolving them silently.

### ProcessExecutor and HttpDownloader are reachable

Earlier design analysis assumed no public getter returns a
`ProcessExecutor`; it had checked `Composer.php`'s getters only. In fact
`Composer::getLoop()` → `Loop::getProcessExecutor(): ?ProcessExecutor` /
`Loop::getHttpDownloader(): HttpDownloader` make both reachable. The
graph-owned `ProcessExecutor` instance must therefore be proxied (its job
queue is driven by the Rust loop), while the design intent — plugins using
it as a stateless utility — survives only for plugin-`new`ed instances.
This is exactly the dual-instantiation situation the
`plugin-constructible` attribute exists to surface.

### Dual instantiation

`Locker::getJsonFile(): JsonFile` makes `JsonFile` reachable, so it is
`rust-proxy` + `plugin-constructible` — and plugins `new JsonFile(...)`
constantly. The question is not cosmetic: `ProcessExecutor`, `JsonFile`,
and `Util\Filesystem` being `rust-proxy` is what demotes the VCS/auth
utility belt (`Git`, `GitHub`, `GitLab`, `Bitbucket`, `Svn`, `AuthHelper`,
`RemoteFilesystem`) to `unsupported` — each of them constructs one of those
three internally. `ArrayLoader` is a fourth member: plugins `new
ArrayLoader` constantly, and it drives constructor-plus-setters on the
package classes, so its fate follows theirs. These utilities can become
php-native the moment plugin-`new`ed instances of the trio may live
PHP-locally (or the stub `NewObject` constructor story lands); until the
user decides, the tool reports them as `unsupported` with the constructing
site named.

### Process: dual instantiation split by caller

`ProcessExecutor::executeAsync()` resolves its promise with a
`Symfony\Component\Process\Process` instance, so it may cross the language
boundary despite being a wholesale-`php-native` vendor class. A `Process`
can't be reconstructed PHP-side from Rust-generated data because its state
is stored in a `resource` created by `proc_open()`.

Resolution: split `ProcessExecutor`'s Rust implementation by caller.
Rust-ported Composer code (`VersionGuesser`, `Git`, …) calls `execute_async()`
directly and spawns in Rust. A plugin holding a `ProcessExecutor` handle
(`Loop::getProcessExecutor()`) instead hits the `rust-proxy` stub's RPC entry,
which forwards the spawn to the PHP child so the real `Process::start()` runs
there. The plugin gets the genuine object, never a fake one.

### Package and CompletePackage

They classify as `rust-proxy` mechanically: they carry setters
(`setRepository`, `setInstallationSource`, …), so the strict immutability
test rightly rejects them. The plugin architecture plans a
snapshot-with-writeback treatment for packages ("essentially immutable" as
a pragmatic call); enacting it is an `overrides.list` entry awaiting
explicit confirmation, including for the `RootPackage`/`AliasPackage`
variants.

### Bootstrap classes cannot be stub-shadowed

The child process necessarily `require`s the project's real
`vendor/composer/ClassLoader.php` (and `installed.php` /
`InstalledVersions`) to autoload plugin code, before any stub could load. A
same-FQCN stub cannot coexist; both are overridden to `php-native`.
`InstalledVersions::$installed` is nonetheless genuinely shared state —
Rust rewrites `installed.php` on every dump — so the Rust side must push a
reload (`InstalledVersions::reload()`) after installs, or post-install
event handlers read stale data.

### ConsoleIO leaks world-2 objects

`ConsoleIO` (rust-proxy) returns real symfony/console instances through the
seam: `getTable(): Table` / `getProgressBar(): ProgressBar`, and its
constructor takes `InputInterface`/`OutputInterface`/`HelperSet` — none of
which can cross the wire as values. The stub needs a bespoke story (e.g. a
local Table bound to a proxying `OutputInterface`), or these members become
explicit errors. Undecided.

### Proxy clone semantics

`clone $package` is a common plugin idiom (and `NoopInstaller::install`
does `$repo->addPackage(clone $package)`), but PHP `clone` on a proxy stub
copies the handle, not the Rust entity. The stub generator needs a
`__clone` that RPCs a clone of the entity. Undecided.

## The classifier tool

### Dependencies and layout

`scripts/plugin-class-classifier/` implements the pipeline in PHP. Two
Composer dependencies: nikic/PHP-Parser (parsing) and
jetbrains/phpstorm-stubs (builtin function signatures — by-ref parameter
positions are read from the stubs instead of a hand-maintained table).
`vendor/` and `report.json` are git-ignored; `composer.lock` is committed.
The exception lists live in `lists/`.

### Running

    scripts/plugin-class-classifier/classify

reads `composer/src/Composer` and writes `report.json` (machine-readable,
stable ordering) plus a human-readable Markdown summary to stdout. Per-class
rows carry category, direction, attributes (including public static
properties on stub categories), demotion reasons, and — for stub-relevant
categories — the per-method purity verdicts, by-ref parameter positions,
and callable parameter positions the stub generator needs. The wholesale
vendor package table and the vendor types actually reached by the closure
are listed separately. The run exits non-zero when a rule cannot decide (an
unfiled mutable-static class, a reachable type that resolves to nothing
known); the report is still written so the violation can be reviewed and
filed.

`report.json` is generated output and not tracked in git. The intended
workflow on a Composer upgrade: run the classifier before updating the
`composer/` checkout, keep that report aside, re-run after the update, and
diff the two files — review only the changed rows. A brand-new class lands
in a category (or in the violation list) without any human re-derivation.

### Querying a single class

    scripts/plugin-class-classifier/query <target>...

answers "how is this class treated?" for one or more targets, from
`report.json` (generated on the fly when missing; staleness is not
checked). A target may be a PHP source file, a Rust source file, or a class
name — fully qualified or just the short name, case-insensitive:

    query composer/src/Composer/Util/Git.php
    query crates/shirabe/src/io/io_interface.rs
    query Locker

Rust paths are resolved by normalized segment matching against the report's
FQCNs rather than textual case conversion, because the snake_case mapping
is not reversible for acronyms (`io_interface.rs` → `IOInterface`). Vendor
class names resolve at package granularity. The output shows category
(including an overridden category's computed value), direction,
reachability, reasons, attributes, and the mutator methods.

### Analysis limits

All conservative:

* dynamic calls (`$this->$m()`, `call_user_func` and other
  callback-forwarding builtins with `$this`-rooted arguments) force
  `mutator`;
* a call to a method that is abstract at the analyzed level
  (`$this->getVersion()` from `BasePackage::getUniqueName`) forces
  `mutator` even when every concrete implementation is a pure read — this
  costs spurious epoch invalidations, not correctness, and can be refined
  by resolving abstract callees over all concrete subtypes;
* `parent::m()` purity resolves against the own hierarchy rather than the
  declaring parent;
* vendor by-ref signatures are tabled only for composer/pcre
  (jetbrains/phpstorm-stubs covers PHP itself, not Composer's vendor
  packages; other vendor APIs composer calls expose no by-ref parameters);
* docblock type extraction tokenizes rather than fully parsing phpdoc
  (constants, `@template` names, and phpstan aliases are filtered out).
