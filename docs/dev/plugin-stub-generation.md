# Plugin proxy stub and guard generation

## Purpose

`scripts/plugin-stub-generator/generate-stubs` turns the classifier report
(`scripts/plugin-class-classifier/report.json`, see
[plugin-class-classification.md](./plugin-class-classification.md)) into everything the boundary
needs that can be derived mechanically:

* the **proxy stubs** under `crates/shirabe-php-rpc/php/stubs/`: one per class whose treatment is
  `stub`, a same-FQCN class whose public methods forward to the Rust-side entity over RPC (see
  "Proxy stubs, runtime classes and guards" in [php-worker.md](./php-worker.md));
* the **guards** under `crates/shirabe-php-rpc/php/guards/`: one per class whose treatment is
  `guard`, a same-FQCN class whose constructor and methods raise an explicit error. Without them
  the worker's autoloader would fall through to the real Composer implementation, and code running
  there would work on a second instance the Rust side never sees;
* `Shirabe\ByValueClasses` (`php/stubs/Shirabe/ByValueClasses.php`): the classes whose treatment
  is `by-value`, which `Shirabe\ByValueObject` reads as the vocabulary of the by-value codec;
* the Rust tables the hand-written Rust side has to satisfy:
    * `crates/shirabe/src/php_worker/rpc/forwarding.rs`: the forwarded members of every stub,
      runtime and guard class,
    * `crates/shirabe/src/php_worker/by_value.rs`: the codec functions of every by-value class,
    * `crates/shirabe/src/php_worker/pushed_statics.rs`: the function that pushes each shared
      static property whose state lives in the worker.
* the **PHP declarations** under `crates/shirabe/php-decls/`: the PHP surface of the Composer
  types the Rust bindings answer for, as JSON (see "Declarations").

None of these files is edited by hand. The crate's `build.rs` embeds whatever the PHP directories
hold.

## Running

```
cd scripts/plugin-stub-generator
composer install            # once; vendor/ is git-ignored
./generate-stubs            # rewrites every generated file
./generate-stubs --check    # verifies the committed files are fresh; exit 1 otherwise
```

Inputs:

* the classifier report (override with `--report=`). A report carrying classification violations
  is rejected. Regenerate it first with `scripts/plugin-class-classifier/classify` when the inputs
  changed.
* the Composer checkout (`composer/`, override with `--composer-root=`). The generator locates
  sources through the checkout's own PSR-4 autoload map, so interfaces from vendor packages
  (e.g. `Psr\Log\LoggerInterface`) resolve too.
* the worker runtime's own classes under `crates/shirabe-php-rpc/php/runtime/Shirabe/`, read only
  for what they forward (see "Forwarding modules").

The generator reads nothing else. In particular, it does not decide a treatment from what exists
in `php/runtime/` or on the Rust side: it checks that the implementation agrees with the report.

`just setup` installs both tools' dependencies.

## What a stub looks like

Stubs are emitted every base class first, following inheritance.

* **Root stubs** (stubs extending no class, or extending one that is not a stub; a stub extending
  another stub or a runtime class inherits the boilerplate from it) carry the proxy boilerplate:
  the `__entityId` property, the `__shirabeBind` binder the registry calls when reviving a stub
  for an existing entity, a destructor releasing the Rust entity id, and the Rust entity handle
  helper. The real `extends`/`implements` hierarchy is preserved and `\Shirabe\RustStub` is
  appended to the interface list.
* **Constructors** reproduce the real class's parameter list and forward to the Rust side (a
  `NewObject` sent through `\Shirabe\RpcRuntime::newRust`), which allocates the entity and answers
  with its handle. One is emitted for every root stub and for every subclass that declares a public
  constructor of its own, so `new SomeProxiedClass(...)` in plugin code never yields an unbound
  stub. Proxy revival does not run them (see `__shirabeBind` above).
* **Instance methods** forward via `\Shirabe\RpcRuntime::callRust`. For a root stub the emitted
  surface is the interface closure (each interface before the ones it extends, methods in
  declaration order; a concrete redeclaration in the class wins over the interface signature)
  followed by the class's own remaining public methods. A subclass stub declares only the methods
  whose *name* is new relative to the inherited stub surface — including methods of interfaces the
  subclass adds; an omitted override must match the inherited parameter list (names, arity,
  defaults, passing modes — type declarations may differ), otherwise generation fails.
* Methods returning `self`/`static` perform the RPC and then `return $this;` to preserve identity
  instead of round-tripping the handle.
* A method listed in `Generator::LOCAL_BODIES` gets the body given there instead of a forward.
  `PartialComposer::getLoop()` is the one entry: `Composer\Util\Loop` is a two-world class the
  Rust side never hands out as an entity, so the stub answers with the worker-side Loop built over
  the proxied services ([plugin-async.md](./plugin-async.md)). The member stays in the report's
  forwarded list; the Rust forwarding module leaves it out.
* **By-ref parameters** are declared `&$name` on the stub as well. The answer carries the value
  each of them holds afterwards, which the stub assigns back (see "By-ref parameters" in
  [php-rpc.md](./php-rpc.md)).
* **Arity** is reproduced when — and only when — the real method body calls `func_num_args()`, in
  which case the stub trims the argument list to what the caller actually passed instead of sending
  the declared defaults. Sending them regardless would make the Rust side answer a call the plugin
  never made: `ProcessExecutor::execute($cmd)` forwards the child's output, while
  `execute($cmd, $out)` captures it, and only the argument count separates them.
* **Class constants, static methods and public static properties** are materialized verbatim from
  the real source (they read no instance state and run locally in the worker), together with any
  non-public static helpers the methods call.
* A static method is materialized only when it can actually run in the worker. One that reads a
  **static property** — whose value the Rust side owns, as `ProcessExecutor::$timeout` does — or
  that reaches a class a **guard** shadows there — as `ProcessExecutor::escape()` reaches
  `Composer\Util\Platform` — forwards instead, as a `CallStaticMethod` sent through
  `\Shirabe\RpcRuntime::callRustStatic`, carrying a comment that names the reason.
* **Instance properties** are not declared on the stub, whatever their visibility: they are entity
  state. Every root stub instead carries `__get`/`__set`/`__isset`/`__unset` forwarders, so each
  access reaches the Rust side. They are emitted unconditionally because PHP's own answer for an
  undeclared property — null on a read, a dynamic property on a write, false on `isset()` — is
  silent.
* **`__toString`** is forwarded like any other method. **`__clone`** is part of the boilerplate on
  every stub, whether or not the real class declares one: PHP has already copied the stub by the
  time it runs, so the copy asks the Rust side for a clone of the entity and rebinds itself to the
  fresh Rust entity id (`__shirabe_clone`, answered with that id). The clone semantics of the real
  class live on the Rust side with the entity.

The generator checks that each stub forwards exactly the instance methods, constructor and static
methods the report lists for the class.

## What a guard looks like

A guard reproduces the real class's declaration — `abstract`/`final`, `extends`, `implements`, the
constants and public static properties verbatim — so that references satisfied by the declaration
alone keep working: `instanceof`, `catch`, `X::class`, class constants. Everything executable
raises `Shirabe\UnsupportedClass::fail()`, which names the class and the member:

* the constructor,
* every public and protected method the guard would otherwise inherit from real code: its own, its
  traits', and those of ancestors up to the first one that is itself guarded or shadowed by a stub
  or a runtime class.

Abstract methods of an abstract guard stay abstract. A method an ancestor declares `final` cannot
be redeclared, so it keeps running the real implementation; the guard's header names each one.

A static method the report forwards for the guard (`statics` in `forced.json`) forwards through
`\Shirabe\RpcRuntime::callRustStatic` instead, like a forwarded static of a stub, carrying the
reason the report gives; the guard's header names it. `Factory::getComposerFile()` is one: it reads
nothing but the `COMPOSER` environment variable.

## Runtime classes

The classes whose treatment is `runtime` are hand-written under
`crates/shirabe-php-rpc/php/runtime/Composer/`: two-world classes such as `Composer\Util\Loop` and
`Composer\Console\Application`, and dual-mode classes such as `Composer\EventDispatcher\Event`
that forward when revived from a handle and run natively when constructed in the worker. The
generator never emits them, but accepts them as base classes of stubs (computing the inherited
surface from the real Composer class the runtime file mirrors). It checks that the files under
`php/runtime/Composer/` are exactly the runtime classes of the report, and that each runtime class
defines every member the report says it forwards.

## Forwarding modules

`crates/shirabe/src/php_worker/rpc/forwarding.rs` holds one Rust module per worker-side class that
forwards members to the Rust side, laid out by the class's namespace (`Composer\Package\Locker` is
`package::locker`). Each module has:

* `PhpClass`, a marker type standing for the class;
* `Forwarding`, a trait with one method per member the class declares and the Rust side answers
  (`implemented` in the report), with no default. The hand-written
  `impl forwarding::<path>::Forwarding for forwarding::<path>::PhpClass` therefore has to answer
  each of them, and cannot answer a member the report marks `unimplemented` or `refused`: either
  mistake is a compile error. The trait of a class whose parent forwards nothing also names the
  handle type of the entities crossing as it or as a descendant (`type Handle`);
* `dispatch`, which routes a method name to the `Forwarding` of the class declaring it, the class
  itself or an ancestor, and answers an `unimplemented` member with the "not available over RPC
  yet" error and a `refused` one with that error and the reason from `forced.json`;
* `BINDINGS`, the class and its `dispatch` as one value;
* `construct` and `call_static` for the classes with a forwarded constructor or static methods. The
  top-level `construct` routes a `NewObject` to them; a class whose constructor is
  `unimplemented`, and a plugin's subclass of a stub, reach its last arm, the "cannot construct"
  error.

The members come from the report. The worker runtime's own classes under `php/runtime/Shirabe/`
(`Shirabe\RustPluginStub`, `Shirabe\RustCapablePluginStub`, `Shirabe\RustProcessStub`) stand for
Rust-side objects no Composer class describes, so the classifier knows nothing of them; their
modules are built from the `RpcRuntime::callRust` calls in their PHP, and the Rust side answers
every one of those members.

## Declarations

`crates/shirabe/php-decls/<namespace path>/<class>.json` (`Composer/Package/Locker.json` for
`Composer\Package\Locker`) describes one Composer class or interface: every interface of the
report and every class forwarding members, together with their Composer supertypes. A Composer
type is one whose source lives under `composer/src/`. PHP's builtin types and vendor types are not
described and not traversed: a member only they declare is not part of the surface, and a member
they declare along with a Composer type counts as the Composer type's.

Each file holds:

* `fqcn`, the forwarding module path (`rustModule`, `["package", "locker"]`), `kind` (`class` or
  `interface`), `abstract`, `parent`, the direct `interfaces`, and the report's `treatment`;
* `methods`: the public methods, static ones included, the type declares, inherits or takes from a
  trait, sorted by name (magic methods other than `__toString` are left out). Each has its PHP
  `name` and its Rust name (`rust`, the name the forwarding modules use), `declaredIn`, `static`,
  `params`, the native `returnType` and the `docReturnType`, and `funcNumArgs` (whether the body
  calls `func_num_args()`). `declaredIn` is the topmost Composer declaration: the interface
  declaring the method that no Composer interface it extends declares too, or the topmost class
  when no interface declares it. Unrelated interfaces declaring one method fail generation. The
  parameters and types are those of the nearest declaration, which is what the stub of the type
  reproduces;
* each parameter's `name`, `position`, `byRef`, `variadic`, `nullable` (null can arrive: the
  parameter is untyped, its type admits null, or it defaults to null), the native `type` with
  class names fully qualified, and the `docType` of its `@param` tag as written (a `@phpstan-` or
  `@psalm-` tag wins). Default values are not described: a stub sends every declared parameter,
  except that one of a method calling `func_num_args()` sends what the caller passed;
* for a class, `constructor` (`declaredIn` is null for PHP's implicit one) and `properties`, the
  public and protected properties, promoted ones included;
* for a class forwarding members, `forwarded` on each method and on the constructor: whether a
  call reaches the Rust side. It is false for a static method materialized in the worker and for
  a method answered there (`Generator::LOCAL_BODIES`). A forwarded member also has the `status`
  the report gives it (`implemented`, `unimplemented`, or `refused` with its `reason`).

The `shirabe-php-binding-macros` crate reads these files at compile time: `#[php_binding]`
generates the adapters of a Rust trait or inherent impl standing for a type, and `php_entity!` the
`Entity` and `Forwarding` impls of a handle type routing each implemented member to them.

## Codec and push tables

`crates/shirabe/src/php_worker/by_value.rs` names, for every by-value class, the functions
`encode_<path>` and `decode_<path>` of `php_worker::codec`, where `<path>` is the class's module
path joined by `_` (`encode_package_link` for `Composer\Package\Link`,
`decode_semver_constraint_multi_constraint` for `Composer\Semver\Constraint\MultiConstraint`).

`crates/shirabe/src/php_worker/pushed_statics.rs` names, for every shared static property whose
sync is `push`, the function `php_worker::<class>::push_<property>` (`installed_versions::push_installed`
for `InstalledVersions::$installed`).

Both are lists of `const _` items that only refer to the functions, so a function the report calls
for and the Rust side lacks is a compile error.

## Coverage assertions

Generation fails — instead of emitting something quietly wrong — on:

* a report carrying violations,
* a stub forwarding other members than the report lists,
* variadic parameters (in constructors too), by-ref parameters in a constructor or in a forwarded
  static method, `func_get_args()` in a forwarded body, static interface methods, magic methods
  other than `__toString`/`__clone`,
* an omitted override diverging from the inherited stub signature,
* a stub extending a class that is neither a stub nor a runtime class,
* a guard for something other than a class,
* a runtime class without its file, a file under `php/runtime/Composer/` whose class is not a
  runtime class, and a runtime class missing a member it forwards,
* a worker-side class forwarding no instance method, and two members of one class mapping to the
  same Rust name.

`generate-stubs` (in both modes) also fails when a `.php` file exists under the stubs or guards
directory that the report does not produce, since `build.rs` would embed it.

When a future Composer release adds a public member the emitter cannot handle, these assertions
surface it at generation time; extending the emitter (or deciding the porting policy) is then an
explicit step.
