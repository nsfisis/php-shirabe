# Environment variables

Shirabe reads the environment variables Composer documents, with the same
meaning. This page lists the variables that are specific to Shirabe.

## `SHIRABE_TRACING`

Enables diagnostic output through [`tracing`](https://docs.rs/tracing). The
value is a list of filter directives in the syntax of
[`tracing_subscriber::EnvFilter`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html),
for example `info` or `shirabe::installer=info,shirabe::dependency_resolver=debug`.

Events are written to stderr, and each span reports its duration when it
closes, which makes this useful for finding where time is spent. Unset or
empty disables the output. Invalid directives print an error to stderr and
disable the output; Shirabe itself keeps running.

## `SHIRABE_COMPOSER_PHP_DIR`

Points at a Composer checkout that replaces the embedded Composer PHP runtime
during development (see [Composer runtime bundle](./dev/composer-runtime-bundle.md)).
