# PHP RPC

Composer can require a specific PHP version or loaded extensions, i.e., platform requirements.
To mimic this behavior needs a real PHP runtime.

This document describes a first design of PHP runtime: a `shirabe-php-rpc` crate that spawns the
system PHP as a child process and asks it for runtime information over a Unix domain socket.

## Scope

The crate supports exactly one interaction pattern, and nothing else:

> Rust calls a named, argument-less PHP function and receives a single fixed-type scalar back.

- Rust to PHP only. PHP never calls back into Rust.
- No arguments.
- Scalar return values only (string / int / float / bool / null).
- Every failure is ignored: a PHP exception, serialization/deserialization
  failure, a missing PHP function, a crashed child, etc. None are handled.
  If anything goes wrong, behavior is undefined.
- Windows is unsupported and `panic!`s for now.

## Locating PHP

Reuse the existing `PhpExecutableFinder` class to resolve the PHP binary.

## Transport

- A Unix domain socket. (No Windows support for now)
- The PHP glue code is a small script written to a temporary file.
- Message frame: `[usize length (little-endian)][payload]`.
  - Request payload: the bare PHP function name as raw bytes.
  - Response payload: `serialize()` of the function's return value.

The PHP worker is a single read-eval-respond loop: read a framed function name,
call the matching entry in a fixed dispatch table, send back `serialize($result)`.

## Global state and public API

PHP runtime information (e.g., process handle) is held as process-global state
rather than threaded through call sites for now.
The crate exposes plain free functions:

```rust
shirabe_php_rpc::get_php_version() -> String
```

The connection is a process-global `static` (e.g. `OnceLock<Mutex<Worker>>`), lazily initialized on
the first call: the first `get_php_version()` spawns the child, performs the handshake, and caches
the connection. Commands that never query PHP never start it. The child lives for the rest of the
process and is left to be reaped at exit (no explicit shutdown message).

A future revision threads this runtime information through arguments or embeds it in structs; for now
callers just reach for the global getter.

## Out of scope

Deferred things: arguments and non-scalar return values, PHP to Rust callbacks
and re-entrancy, object handles / proxies / identity, stub generation, error
propagation, GC / lifecycle, and Windows support.
