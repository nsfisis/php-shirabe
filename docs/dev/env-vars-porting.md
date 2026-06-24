# Environment variable porting

PHP exposes three distinct ways to read process environment variables: the `$_ENV` and `$_SERVER`
superglobals, and `getenv()`/`putenv()`. They look interchangeable but are not, so Shirabe ports
each to a separate construct in `crates/shirabe-php-shim/src/env.rs`. Choose the porting target by
matching the exact PHP construct used in Composer; do not substitute one for another.

## The three are not interchangeable

The three differ in what they observe and when:

* `getenv()`/`putenv()` read and write the real environment variables. `putenv()` mutates the
  live process environment, so a value set with `putenv()` is inherited by child processes spawned
  afterwards.
* `$_ENV` and `$_SERVER` are snapshots taken at startup. They are populated once when the
  process starts and are not kept in sync afterwards. A later `putenv()` does *not* appear in
  `$_ENV`/`$_SERVER`, and assigning to `$_ENV`/`$_SERVER` does *not* change the real environment.
* `$_ENV` and `$_SERVER` are not shared with child processes. Launching an external program via `system()`
  and friends passes along the real environment (as mutated by `putenv()`), not the
  `$_ENV`/`$_SERVER` snapshot.
* Also, `$_ENV` and `$_SERVER` have their own storage; they are not shared.

Because of these differences, porting must preserve which of the three Composer actually used at
each call site.

## Mapping

| PHP | Shirabe |
| --- | --- |
| `getenv()` | `getenv()`/`getenv_all()` |
| `putenv("K=V")` | `putenv()` |
| `putenv("K")` (unset) | `putenv_clear()` |
| `$_ENV` | `PHP_ENV` |
| `$_SERVER` | `PHP_SERVER` |

## TODOs

The current implementation only models the Rust side. Two things remain unimplemented:

* Propagating `$_ENV`/`$_SERVER` into the real PHP runtime. When Shirabe hands control to PHP
  (for the plugin API), the PHP side needs to see the same `$_ENV`/`$_SERVER` snapshot Shirabe
  holds. This is marked `TODO(php-runtime)` in `env.rs` and is not yet wired up.
* Reflecting PHP-side mutations of `$_ENV`/`$_SERVER` back into Shirabe. How to handle the case
  where PHP code rewrites `$_ENV` or `$_SERVER` is still TBD.
