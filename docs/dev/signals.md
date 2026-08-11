# Signals

## In Composer core

Composer has three routes for handling signals:

* `Seld\Signal\SignalHandler`
* `Symfony\Component\Console\SignalRegistry\SignalRegistry` and `SignalableCommandInterface`
* PHP builtins (`pcntl_signal()`, etc.)

Of these, upstream Composer itself only ever uses `SignalHandler`, and Shirabe
behaves roughly the same way. The difference is when the interruption takes
effect, as described in [known incompatibilities](../known-incompatibilities.md).

> Composer runs its abort handler almost immediately after the signal arrives.
> Shirabe, however, runs it at the next checkpoint instead, so stopping `shirabe`
> command by `Ctrl+C` may take more time than Composer.

In PHP, the VM checks for a pending signal at the end of a loop and on a
function call, which gets the signal handler run almost immediately after the
signal arrives. Rust has no such mechanism, so we place the checkpoints by
hand; grep for `signals.is_triggered()` to find them. They are not as
fine-grained as a function call, so more work runs between receiving the signal
and starting the abort than Composer would let through, and it takes longer.

## In plugins and scripts

Shirabe treats signal handling in plugins and scripts as undefined behavior,
for two broad reasons.

The first is that Shirabe consists of a Rust core process and a PHP worker
process that runs the plugins and scripts, which makes faithful reproduction
difficult.

The second is that Composer itself does not fully account for plugins and
scripts subscribing to signals either. The `SignalHandler` that Composer uses
to handle signals overwrites an existing signal handler unconditionally. So
even when a plugin or script installs a handler through Symfony Console or a
PHP builtin, that handler is lost the moment execution reaches a place where
Composer uses `SignalHandler`.

For these two reasons, Shirabe today neither restricts plugins and scripts from
installing signal handlers nor does anything special about it. What happens
when they do is not guaranteed.

This stance may be withdrawn if a legitimate use case turns up.
