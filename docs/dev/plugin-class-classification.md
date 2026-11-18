# Plugin boundary class classification

## Purpose

Shirabe's plugin mechanism runs Composer plugins as real PHP code in a child process (the
worker), connected to the Rust core by a bidirectional RPC channel. For every class of
composer/composer and its vendor dependencies, something has to decide what the worker puts at
that FQCN — a generated proxy stub, a hand-written class, the real source, or a guard that raises
an explicit error — and what the Rust side has to answer for it.

The classifier (`scripts/plugin-class-classifier/`) makes that decision for every class, from two
sources of truth only:

* the Composer sources (`composer/src/Composer` and the vendor packages), and
* a few input files under `scripts/plugin-class-classifier/inputs/`.

It writes the outcome to `scripts/plugin-class-classifier/report.json`, and everything else is
generated from that report: the proxy stubs and guards the worker loads, the list of classes that
cross by value, and the Rust tables of forwarded members, codecs and pushed static state (see
[plugin-stub-generation.md](./plugin-stub-generation.md)). Neither the classifier nor the
generator reads the implementation to decide a treatment — not the files under `php/runtime/`,
not the Rust impls, not any hand-written list. Whether the implementation agrees with the report
is checked instead, and on the Rust side the check is the compiler: the generated code names
every hand-written function the report requires.

The procedure is deterministic, so a class a future Composer release adds or rewrites is
classified without a human re-deriving the design.

## Values and treatments

Each class gets two properties.

### Value

The value says where the instances of a class live, assuming Shirabe were 100% compatible with
Composer. Whether the Rust side implements the class yet, and whether the class is an explicit
error today, never changes its value.

| Value | Where an instance lives | Default treatment |
|---|---|---|
| `rust-handle` | the Rust side; the worker holds a handle | `stub` |
| `rust-value` | nowhere in particular: the value is copied to both sides | `by-value` |
| `worker` | the worker | `real` |
| `refused` | — | `guard` |

* Only classes and enums have a value; interfaces and traits have none (`null` in the report).
  An abstract class has a value like any other class.
* The value does not say whether the class works with the Rust side. A `worker` class may be held
  by the Rust side through a PHP entity handle (`RemoteFilesystem`), or have its static state
  pushed from the Rust side (`InstalledVersions`, `ErrorHandler`). The first is a matter of the
  handlers of the members involved (see "What the classification does not decide"); the second
  is covered by "Shared state".
* Vendor packages are handled as a whole: symfony/console is two-world, every other package is
  `worker`. The vendor and builtin classes that cross by value (the composer/semver constraints,
  `ClassMap`, `DateTime`, `DateTimeImmutable`) get `rust-value` from `overrides.json`, since the
  rules never look at single vendor classes.
* `refused` records a decision to give up compatibility for a class. Only `overrides.json` can
  give it, and no class has it today.

### Treatment

The treatment says what the worker puts at the FQCN.

| Treatment | What the worker defines |
|---|---|
| `stub` | a generated proxy stub |
| `runtime` | a hand-written class under `crates/shirabe-php-rpc/php/runtime/` |
| `by-value` | the real class, listed in `Shirabe\ByValueClasses` and carried as an object record |
| `real` | the real class |
| `guard` | a generated guard |

The treatment starts as the default of the value, `forced.json` overrides it, and the guard
propagation (below) applies last. Interfaces and traits are always `real`. A two-world class is
`real` or `runtime`; only some of its members run in the worker.

## The decision procedure

### Inputs

The parsed sources of `composer/src/Composer`, plus the vendor packages named in
composer/composer's `require`, applied at package granularity
(`src/VendorPackages.php`). `Composer\PHPStan\*` is excluded: dev-only tooling, never shipped at
runtime.

### Seed set

The boundary starts where Composer hands objects to plugin code:

* every type declared under `Composer\Plugin\` (the plugin API namespace),
* every subclass of `Composer\EventDispatcher\Event`,
* `Composer\EventDispatcher\EventSubscriberInterface`.

Nothing else is seeded by hand: `Composer` itself enters through
`PluginInterface::activate(Composer, IOInterface)`, `BaseCommand` through
`CommandProvider::getCommands()`, `InstallerInterface` through
`InstallationManager::addInstaller()` once `InstallationManager` is reached, and so on.

### Reachability closure

For every type `T` in the set, add:

* class-typed native parameter and return types of `T`'s public and protected methods (including
  inherited ones),
* element types from `@param` / `@return` / `@var` docblocks where the native type is `array`,
  `iterable`, `mixed`, `object`, or absent (Composer's PHPStan-checked docblocks make this
  reliable),
* types of `T`'s public and protected properties (a plugin subclassing `LibraryInstaller` sees
  `$this->downloadManager`),
* `@throws` types,
* `T`'s ancestors (parent classes and interfaces),
* when `T` is an interface or abstract class: every concrete subtype declared in the inputs (any of
  them can be the runtime instance behind the abstraction).

Iterate to a fixed point. Private members and method bodies do not extend the closure — the
boundary is the declared API surface, not the implementation. Two-world classes do not extend it
either: the worker carries their real implementation wholesale, so a `Command` subclass's protected
fields are local to the PHP side, not graph seams.

### Direction marking

During the closure, propagate direction: on a `provided` type, plugins call the methods —
parameter types become `consumed`, return/throws types become `provided`. The inverse expansion
(parameters `provided`, returns `consumed`) models Composer calling the plugin's implementation,
and therefore applies only to plugin-implementable types (interfaces and abstract classes) marked
`consumed`. A concrete class also gains a `consumed` mark when it appears in a parameter position
(a plugin can construct one and pass it in), but its method bodies are still Composer's own, so it
always expands provided-style. Without this restriction nearly every direction degenerates to
`both` through self-feedback.

### Values of reachable classes

Assigned in order:

1. Listed in `two-world.json` → `worker`.
2. Subclass of `\Throwable` → `worker`, with the `throwable` attribute. Exceptions are thrown and
   caught where they are raised, and cross as a throwable record.
3. Constants-only class (no methods, no properties, no hierarchy: `ScriptEvents`, `PluginEvents`,
   …) → `worker`; the definition is pure data.
4. Stateless pure class → `worker`: no instance property anywhere in the hierarchy, every
   public/protected instance method `pure` per the purity analysis, and the method bodies are
   locally satisfiable (they construct nothing that is a reachable class with shared state). With no
   state to share, the real code, run against the stubs it is given, answers identically on both
   sides. This catches `VersionParser`, `Auditor`, `Platform\Runtime`, `NoopInstaller`.
5. Value object → `rust-value`: the same purity and body-locality conditions, plus at least one
   instance property (something to copy), and no property — including inherited ones — and no
   constructor parameter typed as a blocking type (a reachable type that is not itself a candidate
   or a throwable, or a vendor type whose package holds living objects: react/promise,
   symfony/console). The candidate sets of steps 4 and 5 grow as one fixed point. This is a strict
   immutable-value detector: it finds `Link` and the advisory family, but not
   `Package`/`CompletePackage`, which carry setters and a `RepositoryInterface` back-reference.
6. Everything else → `rust-handle`.

Abstract classes go through the same steps; those with state end up `rust-handle` (`BaseIO`,
`BasePackage`, `SolverOperation`, `Rule`, `ArchiveDownloader`, `VcsDownloader`, `VcsDriver`).
Step 6 is a fall-through, so the reachable solver internals and VCS drivers are `rust-handle` too.
With full compatibility as the goal, plugins can touch instances of them, so that is the right
value; which of them gets a stub first is a priority the classification does not decide.

### Values of unreachable classes

A class the closure never reaches is `worker`. No instance the Rust side owns ever reaches plugin
code through the API, so whatever instance exists is one the worker made. If such a class's real
code constructs a `rust-handle` class whose stub constructor builds a Rust-side entity, the result
is the entity the Rust side would hold, not a second, unconnected instance. A class whose real code
cannot run in the worker because of what it reaches is guarded by the guard propagation, which
works from the treatments rather than the values.

### Overrides

`overrides.json` changes the value of a class for reasons the rules cannot see. It holds:

* `Composer\Autoload\ClassLoader` → `worker`: the worker bootstraps through the real
  `vendor/composer/ClassLoader.php` before any other definition could load.
* `Composer\Util\Http\Response` → `rust-value`: the value-object rule rejects it because
  `collect()` assigns to `$this`, but the object graph never retains a response, and freeing the
  copy each side holds is what `collect()` is for.
* the vendor and builtin classes that cross by value: the four composer/semver constraint classes,
  `ClassMap`, `DateTime` and `DateTimeImmutable`.

The report keeps the value the rules would give an overridden class as `ruleValue` (vendor and
builtin classes have none). An override that gives the value the rules already give is a
violation.

### Default treatment, forced treatments and the guard propagation

The treatment of a class is the default of its value, unless `forced.json` gives one (see "Input
files"). Then the guard propagation runs as a fixed point. A class whose value is `worker`, whose
treatment is the default `real`, and that is neither forced nor two-world, becomes `guard` when
its code has one of these hard references:

* `new X`, where `X` is a guard,
* a static reference to a class this rule guarded.

The real code of such a class would only get halfway before failing; failing at the entry is
better. The fifteen classes guarded this way today are the orchestrators (`Factory`, `Installer`,
the solver and its builders, `RepositoryFactory`) and the classes that construct them (`Git`,
`GitHub`, `AuthHelper`, …). The report names the reference that guarded each of them.

### Forwarded members

For every class whose treatment is `stub` or `runtime` and whose value is `rust-handle`, the
classifier lists the members its worker-side definition forwards to the Rust side, per declaring
class:

* the instance methods: for a class whose parent forwards nothing, the methods of its interfaces
  and its own public methods (`__toString` included, other magic methods excluded); for any other
  class, those of them its parent's surface lacks. An abstract class lists the methods it
  declares, even though only its concrete subclasses are ever instantiated.
* the constructor (`__construct`) of every concrete stub whose `new` reaches the Rust side: a stub
  whose parent forwards nothing declares one, and so does a stub declaring a public constructor of
  its own; any other stub inherits its parent's. A runtime class builds its own instance, and
  forwards no constructor.
* the public static methods of a stub that cannot run as copies of the real code in the worker:
  the ones that read or write a static property, or that name a guarded class, directly or through
  the non-public static helpers they call.

For a guard, the static methods `forced.json` lists under `statics`.

A guard's static carries the reason it is safe to serve. The stub generator checks that each stub
forwards exactly these members, and the Rust forwarding modules are generated from them. Whether
the Rust side answers a forwarded member is not an input of the classifier: the Rust side's entity
declarations record the members it does not answer (`unimplemented` and `refused` in
`php_entity!`).

## Input files

All inputs are JSON objects under `scripts/plugin-class-classifier/inputs/`. An entry that needs a
justification carries it in a `reason` field. Every file fails the run when an entry names a
class, member or property the Composer sources do not have, and when an entry gives what the rules
already give.

| File | Content | Kind |
|---|---|---|
| `two-world.json` | the namespaces and classes each side implements on its own | evidence |
| `static-state.json` | the disposition of each static property a class writes | evidence |
| `overrides.json` | values the rules cannot see | value |
| `forced.json` | where the current treatment departs from the value | current state |

### two-world.json

`Composer\Console\*`, `Composer\Command\*`, symfony/console, `Composer\Util\Loop` and
`Composer\Util\SyncHelper`, each keyed by namespace prefix or FQCN with a reason. Each side runs
its own implementation of them. The list cannot be inferred from the sources; it also stops the
reachability closure.

### static-state.json

`{"<FQCN>": {"<property>": "<disposition>"}}`, for every static property a class writes from its
method bodies. See "Shared state". A property written without an entry fails the run.

### overrides.json

`{"<FQCN>": {"value": "<value>", "reason": "..."}}`. See "Overrides".

### forced.json

Everything in which the worker, or the Rust side, currently departs from what the value calls
for. Each entry is keyed by FQCN:

```jsonc
{
  "Composer\\Json\\JsonFile": { "treatment": "guard" },
  "Composer\\Installer\\LibraryInstaller": { "treatment": "real", "reason": "..." },
  "Composer\\Console\\Application": {
    "treatment": "runtime",
    "reason": "...",
    "properties": { "composer": "handoff", "hasPluginCommands": "rust-local", "logo": "static-config" }
  },
  "Composer\\Factory": {
    "statics": { "getComposerFile": { "status": "forward", "reason": "..." } }
  },
  "Composer\\Downloader\\FileDownloader": {
    "staticProperties": { "downloadMetadata": "unsynced" }
  }
}
```

* `treatment` overrides the treatment.
    * `guard`: a `rust-handle` or `rust-value` class with no stub or codec yet. It takes no reason,
      since the reason is always that one.
    * `real`: a class whose value is not `worker` but whose real code stays in the worker, or a
      `worker` class kept out of the guard propagation. A reason is required.
    * `runtime`: a class with a hand-written definition under `php/runtime/`, because no generated
      one can do the job (the dual-mode `Event` and `PreCommandRunEvent`) or because the upstream
      body cannot run in the worker (`Application`, `Loop`). A reason is required.
    * A treatment equal to the one the class would have anyway fails the run.
* `statics` lists the static methods of a guard that forward to the Rust side instead of failing,
  each with the reason it is safe to serve.
* `properties` classifies, for a runtime class, every property the upstream class declares:
  `handoff` (copied into the worker-side instance), `rust-local` (Rust-side state the worker
  never needs) or `static-config` (static rendering data). A declared property without an entry,
  or an entry for a property upstream no longer declares, fails the run.
* `staticProperties` marks the shared static properties nothing keeps in sync (`unsynced`).

### forced.json has to become empty

An entry means the implementation lags behind, or the value of the class is not settled yet. That
is the only place either is recorded: there is no list of open questions elsewhere. A class whose
value is undecided keeps the value the rules give it, and `forced.json` records its current
treatment; revising the value later only means rewriting the entry so the treatment stays.

Changing the classifier never changes what the worker does — the treatment of a class, and
whether each member forwards or fails. When a rule or a value changes and the treatment would
follow, a `forced.json` entry cancels the difference. Changing what plugins can do is a separate
change that adds or removes `forced.json` entries.

## Shared state

Apart from where instances live, the two processes share state. Ideally all of it is synced both
ways.

### Static properties

The disposition of each static property a class writes says whether the state is shared:

* `memo-cache`: pure memoization; each side computes its own copy (`Git::$version`,
  `ProcessExecutor::$executables`, `Platform::$isDocker`, …).
* `seed-once`: copied from the Rust side once when the worker starts (nothing has it today).
* `needs-sync`: genuinely shared state (`ProcessExecutor::$timeout`,
  `InstalledVersions::$installed`, `ErrorHandler::$io`,
  `FileDownloader::$downloadMetadata` / `$responseHeaders`).
* `needs-review` is what the report shows for a property with no entry, and fails the run.

How a `needs-sync` property is kept in sync follows from the treatment of its class:

* `stub`: the stub forwards the static accessors that read and write it, so the worker holds no
  copy (`ProcessExecutor::$timeout`). PHP cannot intercept a public static property, so a public
  one has no mechanism.
* `real` or `runtime`: the state lives in the worker and the Rust side pushes it there (`push`):
  `InstalledVersions::$installed` after each install dump, `ErrorHandler::$io` through
  `register()`. The function that pushes each property is named by the generated
  `php_worker::pushed_statics`, so a missing one is a compile error; whether it is called at the
  right moments is not checked.
* `guard`: no mechanism. The guard copies public static properties from the real source, so a
  plugin reads the initial value.

A property with no mechanism has to be recorded as `unsynced` in `forced.json`:
`FileDownloader::$downloadMetadata` and `$responseHeaders`, which a plugin reads as empty arrays.

### Process environment

The environment is not class state, so it is recorded here rather than in the backlog.

* Rust → worker: `shirabe-php-rpc`'s `env.rs` replays the Rust side's writes before the outermost
  RPC call. When Rust code called from PHP writes the environment and calls PHP again in a nested
  call, the write is not replayed.
* Worker → Rust: nothing syncs it (see [env-vars-porting.md](./env-vars-porting.md)). A plugin's
  `putenv()` or `$_SERVER` write, and `Platform::putEnv` in the worker's real code (only
  `Svn::cleanEnv` today), do not reach the child processes the Rust side starts.

## Rust obligations

Every obligation follows from the report and is checked by compiling the generated code:

* `stub` and `runtime`: an answer for every forwarded member, generated by the entity declaration
  of the class (`php_entity!`), which routes each member to the Rust side or lists it as
  `unimplemented` or `refused`.
* `by-value`: an encode and a decode function in `php_worker::codec` (named by
  `php_worker::by_value`).
* a pushed static property: the function that pushes it (named by `php_worker::pushed_statics`).
* `real`: none as a rule. A worker class the Rust side holds through a PHP entity handle
  (`RemoteFilesystem`) is called through its public PHP methods, which the generated code does not
  cover.

## Backlog

The report's `backlog` lists the gaps between the current treatment and full compatibility; the
classifier prints the count of each item as a warning, without failing. It is computed from the
inputs alone.

The backlog judges references more strictly than the guard propagation: a static reference to any
guarded class counts as one the worker cannot follow, since a guard fails its static methods too
(except the ones it forwards).

1. Guarded `rust-handle` and `rust-value` classes: each needs a stub or a codec. `reachable`,
   `direction` and `plugin-constructible` are listed to help set priorities.
2. Hard references the upstream code in the worker cannot follow, per member: in every `real`
   class (two-world classes only when reachable, i.e. `BaseCommand`), a `new`, static reference or
   `extends` of a guarded class.
3. The `worker` classes the guard propagation guarded, with the references that did it.
4. Concrete `rust-handle` classes whose treatment is `real`: an instance the Rust side owns cannot
   cross.
5. `unsynced` static properties.

The forwarded members the Rust side does not answer are listed by the entity declarations on the
Rust side (`unimplemented` and `refused` in `php_entity!`), not here.

## What the classification does not decide

Much of what happens at the boundary is not a property of a class, and the classification does not
record it. It is the content of the handlers of the members involved:

* the API slot (a method and an argument or return position): `Locker::getJsonFile()` alone is an
  error; `AutoloadGenerator::createLoader()` alone builds its answer in the worker; an object
  passed to `InstallationManager::addInstaller(InstallerInterface)` becomes a reverse adapter.
* the concrete type on the Rust side: a handle is revived as the class the Rust side writes into
  it (`Shirabe\RustProcessStub`, `Shirabe\RustPluginStub`), and a stubbed class the Rust side does
  not bind (`ArrayRepository`) never crosses.
* where an instance came from: an `Event` or `PreCommandRunEvent` the Rust side hands over
  forwards, while one the worker constructs runs natively; the stub subclasses of `Event`
  (`Script\Event`, …) are not dual-mode.
* the runtime class: a plugin's subclass of a stub reaches the "cannot construct" error through
  `NewObject`, and a subclass of `Link` falls out of the exact-class check of the by-value codec and
  fails in the Rust decoder.
* how promises, callables, resources and exceptions cross ([php-rpc.md](./php-rpc.md),
  [plugin-async.md](./plugin-async.md)).

The worker runtime's own classes under `php/runtime/Shirabe/` (`RustPluginStub`,
`RustCapablePluginStub`, `RustProcessStub`, …) stand for Rust-side objects no Composer class
describes; they are outside the classification, and the stub generator reads what they forward
from their PHP.

### Notes on single classes

**ProcessExecutor and HttpDownloader.** No getter of `Composer` returns a `ProcessExecutor`, but
`Composer::getLoop()` → `Loop::getProcessExecutor()` / `Loop::getHttpDownloader()` make both
reachable. Both are stubs, and a plugin-`new`ed instance allocates a Rust-side entity of its own
rather than a second, unconnected executor or downloader. That keeps `ProcessExecutor::$timeout` —
process-wide state Composer seeds from `process-timeout` and `RunScriptCommand` rewrites mid-run —
one value for both sides, and lets the executor stay a constructor argument of the classes that
take one (`new Filesystem($process)`). Children are spawned in the Rust process either way, which
is also right for stdio: the worker inherits the Rust process's standard streams.
`executeAsync()` resolves its promise with a `Symfony\Component\Process\Process`, whose state is
the `proc_open()` resource of whichever process called `start()`; a Rust-side spawn has no such
object, so the promise resolves with a `Shirabe\RustProcessStub`, a subclass limited to the
surface that inspects a terminated process. `HttpDownloader::get()` and `copy()` answer with a
`Response` held as a value; its async surface registers each request as a job and answers with a
pending promise. `Loop` is two-world: the worker runs a port of the class over the two stubs, and
`Composer::getLoop()` answers with it (see [plugin-async.md](./plugin-async.md)).

**AutoloadGenerator.** `Composer::getAutoloadGenerator()` hands out the generator Composer's own
dumps run on, so a setter a plugin calls is seen by the next dump on the Rust side.
`buildPackageMap()` answers with package handles, `parseAutoloads()` with plain arrays, and
`dump()` with a `ClassMap` carried as a value. `createLoader()` answers with a real `ClassLoader`
built in the worker and filled with the state of the Rust-side loader, left unregistered as
upstream leaves it. The constructor is unimplemented: the Rust generator takes the concrete
`EventDispatcher`, and the entity table holds the dispatcher only through its interface.
`setPlatformRequirementFilter()` is refused: the factory's filters are guarded, and a
plugin-implemented filter has no reverse adapter.

**Locker.** `Composer::getLocker()` hands out the locker the running command reads and writes the
lock file through: the lock data a plugin reads is the Rust side's cache, and an in-memory
`setLockData(..., false)` is seen by the Rust side's next read. `getLockedRepository()` answers
with a `LockArrayRepository` stub over a fresh Rust-side repository, as upstream builds a fresh one
per call. `getJsonFile()` and `updateHash()` are refused, since `JsonFile` has no stub, and the
constructor is unimplemented: a plugin-built `Locker` would be a second view on the lock file.

**Response.** Two members do not work from the worker even as a value: `decodeJson()` reaches the
guarded `JsonFile`, and Composer answers a curl request with the `CurlResponse` subclass where this
port flattens the answer into a `Response` before it leaves `HttpDownloader`.

**Package and CompletePackage.** The whole package family is `rust-handle`. It is genuinely
mutable — 60+ setters, and Composer mutates packages in flight (`DownloadManager::download`,
`GitDownloader::doUpdate`, `PoolBuilder`, `LockTransaction`) — so a by-value treatment would need
write-back and invalidation on top of the copy.

**Bootstrap classes.** The worker `require`s the project's real `vendor/composer/ClassLoader.php`
(and `installed.php` / `InstalledVersions`) to autoload plugin code before any stub could load, so
`ClassLoader` is a `worker` override and `InstalledVersions` is `worker` by the rules. Plugin
classes are autoloaded from the Rust side: the loaders `PluginManager::registerPackage` builds
live in the Rust process, and the worker's autoloader asks them for a file through the
`__shirabe_find_file` runtime service. `ClassLoader::register` keeps a Rust-side autoload stack
that mirrors PHP's, so the loader `EventDispatcher` builds for scripts sits on the same stack after
the plugin loaders.

**ConsoleIO.** It returns real symfony/console objects (`getTable()`, `getProgressBar()`), and its
constructor takes `InputInterface`/`OutputInterface`/`HelperSet`, none of which can be carried as a
value. Those members and the constructor are unimplemented until a design exists.

### Undecided values

These are represented by keeping the value the rules give and recording the current treatment in
`forced.json`; none of them is settled by the classification.

* **Base classes plugins extend while the Rust side holds instances too** (`FileDownloader`,
  `ArchiveDownloader`, `LibraryInstaller`, `BaseIO`, `Package`, …). Their value is `rust-handle`.
  Full compatibility may call for a dual-mode definition like `Event`'s: an instance from the Rust
  side forwards, one the worker constructs runs the real code. Today their treatments split three
  ways, each broken on one side:
    * `guard` (`FileDownloader`, forced): extending `ArchiveDownloader` fails in the parent
      constructor.
    * `real` (`LibraryInstaller`, `ArchiveDownloader`, forced): an instance the Rust side owns
      cannot cross.
    * `stub` (`BaseIO`, `Package`, by default): a plugin's subclass fails at `NewObject`, which is
      decided by the runtime class and not recorded here.
* **Unreachable orchestrators** (`Installer`, `Factory`, the solver). Their value is `worker`
  (`rust-handle` for the reachable solver internals); forwarding their static entry points to the
  Rust side as `rust-handle` classes may be the better design. They are guarded today, by the guard
  propagation for `Factory` and `Installer` and by `forced.json` for the solver internals. Revising
  the value means adding an override and a `forced.json` `guard` entry, so the treatment stays.
* **The runtime classes**: whether full compatibility still needs a hand-written definition is
  open for as long as their `forced.json` entries remain.

## Attributes

The report also carries, for every class:

* `direction` — `provided` / `consumed` / `both`: whether instances flow Rust→PHP (return
  positions, event getters, callback arguments) or PHP→Rust (parameters of methods on shared
  objects, plugin-implemented contracts).
* `plugin-constructible` — a non-abstract class with a public constructor.
* `mutable-static` — the disposition of each static property the class writes.
* `throwable` — a subclass of `\Throwable`; it always needs a real definition in the worker so that
  `catch` and `instanceof` work.
* `public-static-properties` — PHP has no `__getStatic`, so a stub cannot intercept static property
  access.
* per method, `purity`: a method is a mutator iff it assigns to `$this->…` (directly, via compound
  assignment, `unset`, increment), passes a `$this`-rooted expression into a by-reference
  parameter position (builtin signatures resolved from jetbrains/phpstorm-stubs), or transitively
  calls a mutator on the same object; statically unresolvable calls are conservatively mutators.
* per method, `byRefParams` (the positions the protocol's out-params are keyed by) and
  `callableParams` (the positions the callback-handle machinery must cover).
* `hardRefs`, per member: `new X`, static calls `X::m` and static property writes `X::$p` from the
  method bodies, and the `extends` of the class.

## The classifier tool

### Layout

`scripts/plugin-class-classifier/` implements the procedure in PHP, with nikic/PHP-Parser for
parsing and jetbrains/phpstorm-stubs for builtin function signatures. `vendor/` is git-ignored;
`composer.lock`, the inputs and `report.json` are committed.

### Running

    scripts/plugin-class-classifier/classify
    scripts/plugin-class-classifier/classify --check

The first form reads `composer/src/Composer` and the inputs and writes `report.json` (stable
ordering), prints a summary to stdout and the backlog counts to stderr, and exits non-zero on a
violation (an input error, an unfiled static property, a reachable type that resolves to nothing
known); the report is still written so the violation can be reviewed. `--check` instead fails
unless the committed report equals the one the inputs give.

On a Composer upgrade, regenerate the report and review its diff: a class whose value changed
needs either the new value accepted (and its treatment with it, which changes what plugins can do)
or a `forced.json` entry that keeps the treatment.

### Report

Each row of `classes`:

```jsonc
{
  "fqcn": "...",
  "kind": "class|interface|trait|enum",
  "abstract": false,
  "reachable": true,
  "direction": "provided|consumed|both|null",
  "value": "rust-handle|rust-value|worker|refused|null",
  "ruleValue": "...",            // overridden classes only
  "treatment": "stub|runtime|by-value|real|guard",
  "treatmentSource": "default|forced|propagated",
  "reasons": ["..."],
  "hardRefs": [{ "member": "...", "kind": "new|static|extends", "target": "..." }],
  "attributes": { ... },
  "methods": [{ "name", "visibility", "static", "purity", "byRefParams", "callableParams" }],
  "forwarded": [{ "member": "...", "kind": "constructor|method|static", "status": "...", "reason": "..." }],
  "staticProperties": [{ "name": "...", "disposition": "...", "sync": "forward|push|unsynced|null" }]
}
```

`forwarded` is present for the stub and runtime classes that forward members and for the guards
with forwarded statics. Next to `classes`, the report lists the counts per value and treatment
(`categories`), the vendor packages and the vendor types the closure reached, the reachable
builtins, the `backlog` and the `violations`.

### Querying a single class

    scripts/plugin-class-classifier/query <target>...

answers "how is this class treated?" from `report.json`. A target may be a PHP source file, a Rust
source file, or a class name — fully qualified or just the short name, case-insensitive:

    query composer/src/Composer/Util/Git.php
    query crates/shirabe/src/io/io_interface.rs
    query Locker

### Analysis limits

All conservative:

* dynamic calls (`$this->$m()`, `call_user_func` and other callback-forwarding builtins with
  `$this`-rooted arguments) force `mutator`;
* a call to a method the class hierarchy does not declare (an interface-only method such as
  `$this->getVersion()` from `BasePackage::getUniqueName`) forces `mutator` even when every
  concrete implementation is a pure read; a call to a declared abstract method is treated as pure;
* `parent::m()` purity resolves against the own hierarchy rather than the declaring parent;
* vendor by-ref signatures are tabled only for composer/pcre;
* docblock type extraction tokenizes rather than fully parsing phpdoc (constants, `@template`
  names, and phpstan aliases are filtered out);
* the forwarded instance methods are computed from the interfaces the classifier parses, which
  excludes vendor interfaces; the stub generator, which reads every interface, checks that the two
  agree.
