# String porting

A PHP string is a sequence of bytes. PHP attaches no encoding to it, and only a few functions
interpret it as UTF-8. Shirabe therefore holds PHP strings as bytes, and never decodes them as UTF-8
on its own.

## Types

* `bstr::BString` / `&bstr::BStr` for a string, and `Vec<u8>` / `&[u8]` for binary data such as file
  contents, hashes and archives.
* `PathBuf` / `&Path` for a path. Where Composer handles a path with string operations that `Path`
  cannot express (regex matching, `str_replace`, ...), operate on its bytes locally:
  `path_to_bytes()` (or `as_os_str().as_encoded_bytes()`) gives them, and `path_from_bytes()` turns
  bytes back into a path. The semantics of `Path` methods such as `join()` or `==` differ from
  PHP's string operations in corner cases (absolute operands, trailing `/`, `.` components); using
  them is accepted.
* A shim of a PHP function takes `impl AsRef<[u8]>` for a string parameter, so that `"literal"`,
  `b"literal"` and `BString` are all accepted. A trait method takes `&BStr` instead, to stay
  dyn-compatible.

## Where bytes are decoded as UTF-8

Only inside the shim of a PHP function that itself interprets its input as UTF-8 (`json_encode()`,
`json_decode()`, `preg_*()` with the `u` modifier, `mb_*()`, `htmlspecialchars()`, Symfony's
`UnicodeString`, ...). Such a shim takes bytes, and fails on invalid UTF-8 the way the PHP function
does (`JSON_ERROR_UTF8`, `PREG_BAD_UTF8_ERROR`, ...).

Everywhere else, do not call `from_utf8()`, `from_utf8_lossy()`, `to_str()`, `to_str_lossy()`,
`to_string_lossy()` or `into_string()`: a lossy conversion corrupts bytes that are not valid UTF-8,
and a strict one fails where PHP does not. The `no_utf8_gate` linter enforces this. The conversions
that predate the rule are recorded in `scripts/linters/baselines/no_utf8_gate.txt`; the counts may
only go down, and `scripts/linters/update-baseline` rewrites the file. Code that still holds a PHP
string as `String` converts at its boundary with `legacy_lossy()`.

Boundaries with Rust APIs that take text:

* Regular expressions: `regex::bytes`.
* JSON: `serde_json::from_slice()`. bstr's `serde` feature is disabled, so a `BString` field cannot
  be (de)serialized by accident: bstr would serialize it as an array of numbers. To serialize one,
  use the adapters in `shirabe_php_shim::json_bytes` with `#[serde(serialize_with = "...")]`; they
  fail on invalid UTF-8 the way `json_encode()` does.
* Process arguments and environment variables: `OsStr`/`OsString`, which hold bytes on Unix.
* HTTP header values: `HeaderValue::from_bytes()`.

## Building strings

PHP's `.` operator and string interpolation map to `bconcat!`, and `sprintf()` to the shim's
`sprintf()`. Both keep the bytes of their operands.

`format!` produces a `String`, so formatting bytes into it replaces invalid UTF-8 with U+FFFD. That
is acceptable for messages read by people (errors, warnings, progress), but not for text read by
machines: `composer.json` and `composer.lock`, the files generated under `vendor/composer/`
(`installed.json`, `installed.php`, `autoload_*.php`, ...), caches, subprocess arguments and input,
HTTP requests, values passed to plugins, and values printed to stdout for other programs
(`--format=json`, `config <key>`, paths). Build those with `bconcat!` or `sprintf()`.

The workspace patches bstr with a fork that removes `Display` from `BStr` and `BString`, so that
`format!("{}", bstr)` and `.to_string()` cannot convert lossily without a trace. Format a string
explicitly instead:

* `display_lossy()` for a message read by people, as `Path::display()` does for a path.
* `{:?}` for an error about malformed data, where the exact bytes matter.
* `legacy_lossy()` where the result goes into code that still holds the string as `String`.

`PhpMixed` and the semver constraints do not implement `Display` either: convert them with
`php_to_string()` and `to_php_string()`, which keep the bytes.
