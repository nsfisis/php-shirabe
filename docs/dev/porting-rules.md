# Porting rules

Shirabe ports Composer and its PHP dependencies to Rust file by file. Follow the rules below so that
every piece of Rust code can be traced back to, and compared side by side with, the PHP code it came
from.

## Crate layout

Create one crate per Composer package. Name the crate `shirabe-foo` for `composer/foo`, and
`shirabe-bar-baz` for `bar/baz`. Not every crate corresponds to a package, however.

For a crate based on a package, write the original package name in the `description` field of its
`Cargo.toml`, and bundle the original license file in this repository (see
[LICENSE.md](../../LICENSE.md)). Always check that the license is compatible before doing so.

## File and directory layout

Map PHP files and namespaces one-to-one onto Rust files and modules. For example:

* `composer/src/Composer/Json/JsonFile.php` maps to `crates/shirabe/src/json/json_file.rs`:
    1. Strip `composer/src/Composer/`.
    2. Convert each path component to lower snake case.
    3. Replace `.php` with `.rs`.
    4. Prepend `crates/shirabe/src/`.
* `composer/vendor/symfony/console/Input/InputInterface.php` maps to
  `crates/shirabe-symfony-console/src/input/input_interface.rs`:
    1. Strip `composer/vendor/symfony/console/`.
    2. Convert each path component to lower snake case.
    3. Replace `.php` with `.rs`.
    4. Prepend `crates/shirabe-symfony-console/src/`.

At the top of each file, write the path of the original PHP file, relative to the repository root:

```rust
//! ref: composer/src/Composer/Command/InstallCommand.php
```

Do not merge multiple PHP files into a single Rust file, except in rare cases. When you do, write a
`ref:` line for every PHP file.

## Identifiers and class structure

* Keep identifiers identical to the PHP ones. If an identifier is a Rust keyword, keep it with a raw
  identifier such as `r#type`.
    * Convert camel case to snake case.
    * Keep class names, interface names, and the like exactly as they are.
* Port an interface to a trait, as a rule. Keep the `Interface` suffix in the name.
* Use a trait object, as a rule, wherever an interface is returned or stored.
* Embed the superclass of a PHP class as a field named `inner`, unless that name collides with a PHP
  property.
* Map property visibility as follows: private to non-`pub`, public to `pub`, and protected to
  `pub(crate)`.
    * Always make the `inner` field for an embedded superclass non-`pub`.
* Rust has no default arguments. For a PHP function with default arguments, define one function per
  arity, numbered by argument count (e.g. `write`, `write2`, `write3`). If there are only a few
  default arguments and passing an extra `None` is enough, a single function may cover them all.
    * These functions may later be renamed to more descriptive names during refactoring.

## Types

Map PHP types to Rust types as follows:

* `bool`: `bool`
* `int`: `i64`
* `float`: `f64`
* `string`: `Vec<u8>`, `bstr::BString`, `PathBuf`, `OsString`, or their borrowed counterparts. See
  [strings-porting.md](strings-porting.md).
    * `PathBuf`, `OsString`: for the same purposes as in Rust.
    * `Vec<u8>`: for what is best described as binary data or a blob.
    * `BString`: for what is best described as a string.
    * `String`: do not use for a PHP string. Some code still does; it converts at its boundary with
      `legacy_lossy()`.
* `array`: `Vec` or `indexmap::IndexMap`, depending on whether the array is used as a list or as a
  map.
    * PHP arrays preserve insertion order, and `IndexMap` emulates this. Use `IndexMap` even where
      insertion order does not seem to matter; do not use `HashMap` or `BTreeMap`.
* `null`: `None`
* `?T`: `Option<T>`
* `mixed`: an appropriate type chosen from the context. If none can be determined, fall back to
  `shirabe_php_shim::PhpMixed`.

## Exceptions and error handling

* Port code that throws exceptions as follows:
    * Wrap the return value in `anyhow::Result`.
    * Where the PHP code handles errors with `try`-`catch`, use a nested result such as
      `anyhow::Result<Result<T, E>>`. The outer `anyhow::Result` carries fatal errors that Composer
      does not catch; the inner `Result` carries recoverable errors that Composer catches.
    * Handle `\LogicException` (and its subclasses) depending on whether it can be thrown at run
      time:
        * If it can only be thrown by a programming error, use `panic!` (or `assert!` for a
          conditional check). Do not wrap it in a `Result`.
        * If it can be thrown at run time even by a correct program (i.e. it is used as a convenient
          exception that always aborts), handle it with a `Result` like any other exception.
* Define exception classes as follows:
    * Define PHP's built-in exception classes (`\Exception`, `\RuntimeException`,
      `\UnexpectedValueException`, etc.) in `crates/shirabe-php-shim/src/lib.rs` as flat structs
      with `message: String` and `code: i64` fields. The PHP exception hierarchy is not reproduced.
    * Define Composer's exception classes (including empty classes that only extend another
      exception) as newtypes, e.g. `pub struct MaxFileSizeExceededException(pub TransportException);`.

## Late static binding

Porting PHP inheritance by composition through `inner` loses the late binding of `$this->x()` calls in
the parent's method bodies: once the child delegates to `self.inner.method(...)`, the parent's
implementation is fixed, and the child's overrides are never called. When a parent method calls,
through `$this->`, a method that a child overrides, port it with the following pattern:

* In the parent, extract a shared helper that takes the concrete type standing for PHP's `$this` as a
  trait object (e.g. `this: &dyn DownloaderInterface`), and name it `base_<method name>` (e.g.
  `FileDownloader::base_get_local_changes`, `base_update`).
* Call the helper with `self` as `this` from both the parent's own trait impl and the child's
  delegating method.
* If the method called through `$this->` is not part of the trait (e.g. a PHP protected method),
  compute its result at the call site and pass it as an argument (e.g. `install_operation_appendix`
  of `base_update`).

## PHP built-in functions

* When the exact equivalent of a PHP built-in function is needed, add a function with the same name to
  `crates/shirabe-php-shim/src/lib.rs` and use it.
    * If Rust's standard library or an inherent method has an obvious counterpart, use it instead
      (e.g. `.len()` for `count()`).
* **Place code written with reference to php-src (the C implementation of PHP) in
  `crates/shirabe-php-src`.** The php-src license (3-clause BSD, or zlib for
  `ext/standard/strnatcmp.c` only) differs from Shirabe's MIT license, and the crate boundary keeps
  them apart.
    * Do not write php-src-derived code in any other crate, including `shirabe-php-shim`. The
      dependency goes in one direction only: `shirabe-php-shim` → `shirabe-php-src`.
    * Mirror the php-src tree in the module layout of `shirabe-php-src` (e.g. code from
      `ext/standard/string.c` goes in `src/standard/string.rs`).
    * Record the origin of each function in a doc comment of the following form:

        ```rust
        /// php-src: ext/standard/strnatcmp.c `strnatcmp_ex` (PHP 8.5.2)
        ```

    * Place code that can be written without referring to php-src (written from the specification,
      or delegating to the standard library) in `shirabe-php-shim`. When in doubt, place it in
      `shirabe-php-src`.
    * Public wrappers kept in the shim (e.g. ones that only validate arguments) may remain under MIT.
      Re-export functions that are themselves derived from php-src with `pub use`, so that the API
      seen from outside does not change.

## Regular expressions

See [regex-porting.md](regex-porting.md).

## Strings

See [strings-porting.md](strings-porting.md).

## Environment variables

See [env-vars-porting.md](env-vars-porting.md).
