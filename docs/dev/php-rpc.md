# PHP RPC

Composer can require a specific PHP version or loaded extensions, i.e., platform requirements.
To mimic this behavior needs a real PHP runtime.

This document describes a first design of PHP runtime: a `shirabe-php-rpc` crate that spawns the
system PHP as a child process and asks it for runtime information over a Unix domain socket.

## Scope

The crate supports exactly one interaction pattern, and nothing else:

> Rust calls a named PHP function, passing a single string argument, and receives a single scalar
> back.

- Rust to PHP only. PHP never calls back into Rust.
- Exactly one argument, and it must be a string.
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
  - Request payload: the PHP function name as raw bytes, followed by a `\0` byte and the string
    argument (function names are static literals and never contain `\0`, so the first `\0`
    unambiguously separates name from argument).
  - Response payload: `serialize()` of the function's return value — any of `N;` (null), `b:0/1;`
    (bool), `i:<n>;` (int), `d:<f>;` (float), or `s:<len>:"<bytes>";` (string).

The PHP worker is a single read-eval-respond loop: read a framed function name and argument, call
the matching entry in a fixed dispatch table (`defined`, `constant`), send back
`serialize($result)`.

## Global state and public API

PHP runtime information (e.g., process handle) is held as process-global state
rather than threaded through call sites for now.
The crate exposes plain free functions. For example:

* get_php_version()
* has_constant()
* get_constant()

The connection is a process-global `static` (e.g. `OnceLock<Mutex<Worker>>`), lazily initialized on
the first call: the first call spawns the child, performs the handshake, and caches the connection.
Commands that never query PHP never start it. The child lives for the rest of the process and is
left to be reaped at exit (no explicit shutdown message).

A future revision threads this runtime information through arguments or embeds it in structs; for now
callers just reach for the global getter.

## Out of scope

Deferred things: multiple/non-string arguments, non-scalar return values, PHP to Rust callbacks
and re-entrancy, object handles / proxies / identity, stub generation, error
propagation, GC / lifecycle, and Windows support.
