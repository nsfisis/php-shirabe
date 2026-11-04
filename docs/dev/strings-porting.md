# String porting

A PHP string is a sequence of bytes. PHP attaches no encoding to it, and only a few functions
interpret it as UTF-8. Shirabe therefore holds strings as bytes.

## The default is bytes

Every string is a `BString`/`&BStr` unless it has a reason to be UTF-8. The question to ask of a
value is why it must be UTF-8, not why it should be bytes. If none of the reasons below applies, it
is a `BString`, whatever the value happens to contain and wherever it comes from.

### Reasons a value may be `String`/`&str`

1. **A Rust API has no byte form.** The API takes or returns text and nothing else: compiling a
   pattern with `regex`, `str::parse()`, `chrono`, `reqwest::Url::parse()`, `jsonschema`, the method
   signatures of a `serde` visitor or serializer.
2. **The PHP function itself interprets its input as UTF-8.** `json_encode()`, `json_decode()`,
   `preg_*()` with the `u` modifier, `mb_*()`, `htmlspecialchars()`, Symfony's `UnicodeString`, ...
   Such a shim takes bytes, and fails on invalid UTF-8 the way the PHP function does
   (`JSON_ERROR_UTF8`, `PREG_BAD_UTF8_ERROR`, ...).
3. **The destination is `core::fmt`, and PHP cannot observe the text.** `Display` and `Debug`
   implementations, `panic!`, `tracing`, and an `anyhow` error raised by the port itself that holds
   no PHP string and corresponds to no PHP `throw`.
4. **The output is Rust source.** A build script generating code that `rustc` reads.

Under reasons 1 and 2 the text stays inside one function, between the API call and the conversion
to or from bytes, and does not appear in a signature or a field. A signature imposed by a trait, and
a private helper that only wraps such an API, may spell it.

### What is not a reason

* The value is always ASCII: a status name, a format name, a hash, digits, a fixed map key.
* The value is valid UTF-8 by construction, or was read from JSON or embedded at compile time.
* The value is produced by Rust code rather than read from outside.
* The value is a class, function or constant name. PHP allows bytes above `0x7f` in these.
* The value is a regular expression. A pattern is a PHP string up to the point where it is compiled.
* The value is a message for people. See [Building strings](#building-strings).
* Bytes that are not valid UTF-8 "cannot match" the data being searched. Search the data as bytes.
* The callee or a neighboring field currently takes `String`. Change it.
* The code is shorter that way.

Paths and binary data are not affected: they stay in the types below.

### Recording a reason

Clippy's `disallowed_types` and `disallowed_macros` lints flag every `String`, `str` and `format!`, so
a value that stays UTF-8 says why, where it does. The set of `reason`s can be reviewed with one grep:

```rust
#[expect(clippy::disallowed_types, reason = "regex takes a text pattern")]
fn compile(pattern: &str) -> Result<regex::Regex, regex::Error> { ... }
```

Literals (`let x = "abc"`), `panic!("{n}")` and types inferred from an API are not flagged.
`to_string()` and `std::fs::read_to_string()` are flagged by `disallowed_methods`. A value that gets
its type from `.into()`, `.collect()` or a `str` method that returns `String` is flagged only where
it reaches a signature or a field.

An impl for `&str` whose only purpose is to accept a Rust string literal where bytes are expected
(`PhpConcat for str`, `From<&str> for PhpMixed`, ...) records `reason = "a string literal"`.

rustc ignores a lint attribute on a macro statement such as `assert!(...);`, so `#[expect]` goes on
the enclosing `let`, expression statement or function.

## Types

* `bstr::BString` / `&bstr::BStr` for a string, and `Vec<u8>` / `&[u8]` for binary data such as file
  contents, hashes and archives.
* `PathBuf` / `&Path` for a path. Where Composer handles a path with string operations that `Path`
  cannot express (regex matching, `str_replace`, ...), operate on its bytes locally:
  `path_to_bytes()` (or `as_os_str().as_encoded_bytes()`) gives them, and `path_from_bytes()` turns
  bytes back into a path. The semantics of `Path` methods such as `join()` or `==` differ from
  PHP's string operations in corner cases (absolute operands, trailing `/`, `.` components); using
  them is accepted.
* `&'static BStr` for a string constant. `php_str()` builds one in a `const`:
  `const NAME: &BStr = php_str(b"value");`.
* A shim of a PHP function takes `impl AsRef<[u8]>` for a string parameter, so that `"literal"`,
  `b"literal"` and `BString` are all accepted, and returns `BString`. A trait method takes `&BStr`
  instead, to stay dyn-compatible.

## Decoding bytes as UTF-8

Do not call `from_utf8()`, `from_utf8_lossy()`, `to_str()`, `to_str_lossy()`, `to_string_lossy()` or
`into_string()`: a lossy conversion corrupts bytes that are not valid UTF-8, and a strict one fails
where PHP does not. Clippy's `disallowed_methods` lint enforces this; the methods are listed in
`clippy.toml`. `utf8_text()` is the one exemption, for reasons 1 and 2 above.

Boundaries with Rust APIs:

* Regular expressions: `regex::bytes`.
* JSON: `serde_json::from_slice()`. bstr's `serde` feature is disabled, so a `BString` field cannot
  be (de)serialized by accident: bstr would serialize it as an array of numbers. To serialize one,
  use the adapters in `shirabe_php_shim::json_bytes` with `#[serde(serialize_with = "...")]`; they
  fail on invalid UTF-8 the way `json_encode()` does.
* Process arguments and environment variables: `OsStr`/`OsString`, which hold bytes on Unix.
* HTTP header values: `HeaderValue::from_bytes()`.
* Files: `std::fs::read()`.

Text returned by a Rust API, such as the message of an `std::io::Error`, becomes a `BString` where it
is received, through `display_to_bytes()` when the API only offers `Display`.

## Replacing `str` methods

Methods of `str` and of `bstr::ByteSlice` that share a name with a PHP function often do not share
its behavior. Use the shim of the PHP function:

* `to_lowercase()`/`to_uppercase()` map Unicode; `strtolower()`/`strtoupper()` map ASCII only.
* `trim()` strips Unicode whitespace; PHP's `trim()` strips `" \t\n\r\0\x0B"`.
* `chars()` decodes UTF-8; PHP's string functions work on bytes.
* `lines()` also splits at `\r\n` and drops a trailing empty line; `explode("\n", ...)` does
  neither.

The `ByteSlice` methods among these are listed under `disallowed-methods` in `clippy.toml`. The shim of an `mb_*()` function,
which decodes UTF-8 as PHP does, records that reason where it calls one.

## Building strings

PHP's `.` operator and string interpolation map to `bconcat!`, and `sprintf()` to the shim's
`sprintf()`. Both keep the bytes of their operands. `bconcat!` also takes integers, so `bconcat!(n)`
is the string conversion of `n`. This applies to every string PHP code could observe, including messages read by people: errors, warnings, progress and exception messages. PHP
writes those bytes unchanged, and so does Shirabe.

`format!` produces a `String`, so formatting bytes into it replaces invalid UTF-8 with U+FFFD. Use it
only under the reasons above.

Exceptions hold their message as a `BString`. A `Throwable` also implements `Display`, as `anyhow`
requires, and that output is lossy, so read a message with `get_message()` and not with
`to_string()`.

The workspace patches bstr with a fork that removes `Display` from `BStr` and `BString`, so that
`format!("{}", bstr)` and `.to_string()` cannot convert lossily without a trace. Where the destination
is `core::fmt` (reason 3), format a string explicitly:

* `display_lossy()` for a `BStr`, as `Path::display()` does for a path.
* `{:?}` for an error about malformed data, where the exact bytes matter.

`PhpMixed` and the semver constraints do not implement `Display` either: convert them with
`php_to_string()` and `to_php_string()`, which keep the bytes.
