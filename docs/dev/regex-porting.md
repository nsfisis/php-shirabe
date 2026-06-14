# Regex porting

Composer uses PHP's `preg_*` functions (backed by PCRE). Shirabe ports these to the Rust
[`regex`](https://docs.rs/regex) crate. Because PCRE and the `regex` crate have different feature
sets, follow the rules below when porting.

## Do not introduce a PCRE crate

The `regex` crate is not based on PCRE library and does not support some features that PCRE does.

Even when such a feature is needed, do not introduce a PCRE-family crate such as `pcre2`. Either
rewrite the pattern into a form the `regex` crate can express, or decompose it into hand-written
logic that does not use a regular expression.

## Panic on pattern compile failure

When a pattern fails to compile, `panic!`. Do not wrap it in a `Result` and propagate it to the
caller.

This reflects a design assumption that Composer-derived patterns always compile successfully at
runtime: a compile failure is not a recoverable error but a programming mistake (or a porting bug).

## Do not port performance-only possessive quantifiers

When a PCRE possessive quantifier (`++`, `*+`, `?+`, etc.) is used solely to suppress backtracking
for performance, replace it with the plain quantifier (`+`, `*`, `?`).

The `regex` crate never backtracks, so it has no possessive quantifiers and has no need for them.

## Port other unsupported features ad hoc

For any other PCRE feature the `regex` crate does not support (conditional subpatterns,
backreferences, etc.), port it case by case. When you do, make the transformation explicit with a
comment in the following form:

```
// Regex pattern compatibility:
// <description>
```
