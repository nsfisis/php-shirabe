//! PHP's `preg_*` functions. Composer reaches PCRE through the `Composer\Pcre\Preg` wrapper; its
//! call sites are ported straight onto these functions. `Preg`'s `*StrictGroups()` variants have no
//! counterpart here because `Option` already tells a non-participating capture group from an empty
//! one, and a pattern that fails to compile panics instead of raising `PcreException`: Composer
//! never assembles a pattern that PCRE rejects, so such a failure is a programming error.
//!
//! This module's functions do not mirror the PHP signatures for two reasons.
//!
//! * Typing: the shape of the `$matches` out parameter of `preg_*()` functions depends on the
//!   `PREG_*` flags, which is hard to represent in a type-safe way.
//! * Performance: pattern matching is performed in Composer's hot loops such as dependency
//!   resolution. Allocating a PHP-compatible `$matches` array is expensive.
//!
//! Subjects are byte strings, as in PHP. A pattern without the `u` modifier matches bytes (`.` is
//! any byte, `\w` and case folding are ASCII-only), and a pattern with it matches the UTF-8
//! characters of a subject that must be valid UTF-8.
//!
//! See docs/dev/regex-porting.md for the regex porting rules.

use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use std::sync::{Arc, LazyLock, Mutex};

/// A single match's `$matches`: the `regex::bytes::Captures` the search produced, read by either
/// the named or the numbered form of a capture group. `'h` is the lifetime of the searched
/// subject, which the group values borrow from.
#[derive(Debug)]
pub struct PregMatches<'h> {
    caps: regex::bytes::Captures<'h>,
}

impl<'h> PregMatches<'h> {
    fn new(caps: regex::bytes::Captures<'h>) -> Self {
        Self { caps }
    }

    /// The value of the capture group at `index`, or `None` if that group did not participate in
    /// the match. A group the pattern does not have reads as `None` too, matching how PHP reports
    /// a `$matches` entry that is not there.
    pub fn get(&self, index: usize) -> Option<&'h BStr> {
        self.caps.get(index).map(|group| group.as_bytes().as_bstr())
    }

    /// The value of the capture group called `name`, under the same rules as `get`.
    pub fn name(&self, name: &str) -> Option<&'h BStr> {
        self.caps.name(name).map(|group| group.as_bytes().as_bstr())
    }

    /// The byte offset the capture group at `index` starts at, under the same rules as `get`.
    /// `PREG_OFFSET_CAPTURE` reports a non-participating group at offset `-1`.
    pub fn get_offset(&self, index: usize) -> Option<usize> {
        self.caps.get(index).map(|group| group.start())
    }

    /// The byte offset of the capture group called `name`, under the same rules as `get_offset`.
    pub fn name_offset(&self, name: &str) -> Option<usize> {
        self.caps.name(name).map(|group| group.start())
    }
}

/// Escapes `str` for a pattern. The result is a pattern fragment, which the `regex` crate takes as
/// a `String`: a byte that is not part of a valid UTF-8 sequence is written as a `\xNN` escape,
/// which matches that byte in a pattern without the `u` modifier.
pub fn preg_quote(str: impl AsRef<[u8]>, delimiter: Option<char>) -> String {
    // Regex pattern compatibility:
    // PHP's preg_quote escapes `<` and `>` (PCRE treats `\<`/`\>` as literals), but the `regex`
    // crate reads `\<`/`\>` as start-of-word / end-of-word boundary assertions. `<` and `>` are
    // already literal in the `regex` crate, so they are emitted unescaped to preserve the intended
    // literal match.
    const SPECIAL: &str = ".\\+*?[^]$(){}=!|:-#";
    let mut out = String::new();
    let bytes = str.as_ref();
    for (start, end, c) in bytes.char_indices() {
        if c == '\u{FFFD}' && &bytes[start..end] != "\u{FFFD}".as_bytes() {
            for &b in &bytes[start..end] {
                out.push_str(&format!("\\x{:02X}", b));
            }
        } else if c == '\0' {
            out.push_str("\\000");
        } else if SPECIAL.contains(c) || Some(c) == delimiter {
            out.push('\\');
            out.push(c);
        } else {
            out.push(c);
        }
    }
    out
}

/// The subject a compiled pattern can search, or `None` when the pattern has the `u` modifier and
/// `subject` is not valid UTF-8.
// TODO(error-model): PCRE fails such a search with `PREG_BAD_UTF8_ERROR`, which
// `Composer\Pcre\Preg` turns into a `PcreException`; here the search finds no match instead.
fn searchable<'h>(pattern: &CompiledPattern, subject: &'h [u8]) -> Option<&'h [u8]> {
    (!pattern.utf8 || subject.is_utf8()).then_some(subject)
}

// Whether the pattern matches, for the call sites that ignore the capture groups.
pub fn preg_is_match(pattern: impl PregPattern, subject: impl AsRef<[u8]>) -> bool {
    let resolved = pattern.resolve();
    let pattern = resolved.pattern();
    searchable(pattern, subject.as_ref()).is_some_and(|subject| pattern.regex.is_match(subject))
}

// Returns None if the pattern did not match; otherwise the match's capture groups.
pub fn preg_match<'h, S: AsRef<[u8]> + ?Sized>(
    pattern: impl PregPattern,
    subject: &'h S,
) -> Option<PregMatches<'h>> {
    let resolved = pattern.resolve();
    let pattern = resolved.pattern();
    let caps = pattern
        .regex
        .captures(searchable(pattern, subject.as_ref())?)?;

    Some(PregMatches::new(caps))
}

// Every occurrence of the pattern in `subject`, in match order. The search runs eagerly, as PHP's
// does.
pub fn preg_match_all<'h, S: AsRef<[u8]> + ?Sized>(
    pattern: impl PregPattern,
    subject: &'h S,
) -> impl Iterator<Item = PregMatches<'h>> {
    let resolved = pattern.resolve();
    let pattern = resolved.pattern();
    let matches: Vec<PregMatches<'h>> = match searchable(pattern, subject.as_ref()) {
        Some(subject) => pattern
            .regex
            .captures_iter(subject)
            .map(PregMatches::new)
            .collect(),
        None => Vec::new(),
    };

    matches.into_iter()
}

pub fn preg_grep<T: AsRef<[u8]>>(
    pattern: impl PregPattern,
    array: impl IntoIterator<Item = T>,
) -> impl Iterator<Item = T> {
    let resolved = pattern.resolve();
    array.into_iter().filter(move |s| {
        let pattern = resolved.pattern();
        searchable(pattern, s.as_ref()).is_some_and(|subject| pattern.regex.is_match(subject))
    })
}

pub fn preg_split(pattern: impl PregPattern, subject: impl AsRef<[u8]>) -> Vec<BString> {
    preg_split_impl(pattern, subject.as_ref(), false)
}

pub fn preg_split_delim_capture(
    pattern: impl PregPattern,
    subject: impl AsRef<[u8]>,
) -> Vec<BString> {
    preg_split_impl(pattern, subject.as_ref(), true)
}

fn preg_split_impl(pattern: impl PregPattern, subject: &[u8], delim_capture: bool) -> Vec<BString> {
    let resolved = pattern.resolve();
    let pattern = resolved.pattern();
    let Some(subject) = searchable(pattern, subject) else {
        return vec![BString::from(subject)];
    };

    let mut result: Vec<BString> = Vec::new();
    let mut last = 0usize;
    for caps in pattern.regex.captures_iter(subject) {
        let m = caps.get(0).unwrap();
        result.push(BString::from(&subject[last..m.start()]));
        if delim_capture {
            // `preg_split` accepts no PREG_UNMATCHED_AS_NULL, so the split list
            // holds strings only: trailing unmatched groups are dropped,
            // interior ones are emitted as "".
            if let Some(last_g) = (1..caps.len()).rev().find(|&g| caps.get(g).is_some()) {
                for g in 1..=last_g {
                    result.push(BString::from(
                        caps.get(g).map(|x| x.as_bytes()).unwrap_or_default(),
                    ));
                }
            }
        }
        last = m.end();
    }
    result.push(BString::from(&subject[last..]));

    result
}

pub fn preg_replace(
    pattern: impl PregPattern,
    replacement: impl AsRef<[u8]>,
    subject: impl AsRef<[u8]>,
) -> BString {
    preg_replace2(pattern, replacement, subject, -1, None)
}

pub fn preg_replace2(
    pattern: impl PregPattern,
    replacement: impl AsRef<[u8]>,
    subject: impl AsRef<[u8]>,
    limit: i64,
    count: Option<&mut usize>,
) -> BString {
    let resolved = pattern.resolve();
    let pattern = resolved.pattern();
    let subject = subject.as_ref();
    let limit = if limit < 0 {
        usize::MAX
    } else {
        limit as usize
    };

    let mut out: Vec<u8> = Vec::new();
    let mut last = 0usize;
    let mut n = 0usize;
    if let Some(subject) = searchable(pattern, subject) {
        for caps in pattern.regex.captures_iter(subject) {
            if n >= limit {
                break;
            }
            let m = caps.get(0).unwrap();
            out.extend_from_slice(&subject[last..m.start()]);
            php_replacement_expand(replacement.as_ref(), &caps, &mut out);
            last = m.end();
            n += 1;
        }
    }
    out.extend_from_slice(&subject[last..]);

    if let Some(count) = count {
        *count = n;
    }
    BString::from(out)
}

pub fn preg_replace_callback<'h, S, F, R>(
    pattern: impl PregPattern,
    mut callback: F,
    subject: &'h S,
) -> anyhow::Result<BString>
where
    S: AsRef<[u8]> + ?Sized,
    F: FnMut(&PregMatches<'h>) -> anyhow::Result<R>,
    R: AsRef<[u8]>,
{
    let resolved = pattern.resolve();
    let pattern = resolved.pattern();
    let subject = subject.as_ref();

    let mut out: Vec<u8> = Vec::new();
    let mut last = 0usize;
    if let Some(subject) = searchable(pattern, subject) {
        for caps in pattern.regex.captures_iter(subject) {
            let m = caps.get(0).unwrap();
            out.extend_from_slice(&subject[last..m.start()]);
            let matches = PregMatches::new(caps);
            out.extend_from_slice(callback(&matches)?.as_ref());
            last = m.end();
        }
    }
    out.extend_from_slice(&subject[last..]);

    Ok(BString::from(out))
}

// Translates a PHP PCRE pattern body into `regex`-crate syntax. PCRE treats `\<` and `\>` as
// escaped literal `<`/`>`, but the `regex` crate reads them as start/end-of-word boundary
// assertions. Rewrite those escapes to the literal characters so PCRE-sourced patterns (e.g.
// anything run through `preg_quote`, which escapes `<` and `>`) keep their original meaning. A `\\`
// escapes the following backslash, so `\\<` is left untouched. Without the `u` modifier PCRE reads
// the pattern as bytes, so a non-ASCII character is written as the `\xNN` escapes of its UTF-8
// bytes, which also keeps it valid inside a character class.
fn translate_pcre_literals(inner: &str, utf8: bool) -> String {
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some('<') | Some('>') => {
                    out.push(chars.next().unwrap());
                }
                Some('\\') => {
                    out.push('\\');
                    out.push(chars.next().unwrap());
                }
                _ => out.push('\\'),
            }
        } else if !utf8 && !c.is_ascii() {
            let mut buf = [0u8; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("\\x{:02X}", b));
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// A PHP pattern compiled for the `regex` crate.
#[derive(Debug)]
pub struct CompiledPattern {
    regex: regex::bytes::Regex,
    /// Whether the pattern has the `u` modifier.
    utf8: bool,
}

// PHP's PCRE engine keeps a per-process cache of compiled patterns (pcre.cache_size, default 4096),
// so repeated preg_* calls with the same pattern string are effectively free. The `regex` crate has
// no such cache, and callers like the classmap generator re-issue the same pattern string for every
// file (or even every scanned token), so compilation must be memoized here to match PHP's amortized
// cost.
// The cached value is `Arc`-wrapped so callers share a single `regex::bytes::Regex` instance:
// `Regex::clone()` does not share the underlying meta engine's search-cache pool, so handing out
// fresh clones here would pay a ~10us per-clone cache warmup cost on every single `preg_*` call
// (measured), defeating the point of this cache. `Arc::clone()` is a refcount bump.
static PATTERN_CACHE: LazyLock<Mutex<IndexMap<String, Arc<CompiledPattern>>>> =
    LazyLock::new(|| Mutex::new(IndexMap::new()));

fn compile_php_pattern(pattern: &str) -> anyhow::Result<Arc<CompiledPattern>> {
    if let Some(cached) = PATTERN_CACHE.lock().unwrap().get(pattern) {
        return Ok(Arc::clone(cached));
    }

    let compiled = Arc::new(compile_php_pattern_uncached(pattern)?);
    PATTERN_CACHE
        .lock()
        .unwrap()
        .insert(pattern.to_string(), Arc::clone(&compiled));
    Ok(compiled)
}

fn compile_php_pattern_uncached(pattern: &str) -> anyhow::Result<CompiledPattern> {
    let (source, utf8) = translate_php_pattern(pattern)?;
    let regex = regex::bytes::RegexBuilder::new(&source)
        .unicode(utf8)
        .octal(true)
        .build()?;
    Ok(CompiledPattern { regex, utf8 })
}

// Strips PHP-style delimiters and modifiers from `pattern` and translates the body into
// `regex`-crate syntax, without compiling it. Returns the translated source and whether the
// pattern has the `u` modifier.
fn translate_php_pattern(pattern: &str) -> anyhow::Result<(String, bool)> {
    let delimiter = pattern
        .chars()
        .next()
        .ok_or_else(|| anyhow::anyhow!("empty regex pattern"))?;
    // PCRE allows bracket-style delimiters whose closing character differs from
    // the opening one: `(...)`, `{...}`, `[...]`, `<...>`.
    let closing = match delimiter {
        '(' => ')',
        '{' => '}',
        '[' => ']',
        '<' => '>',
        c => c,
    };
    let end = pattern
        .rfind(closing)
        .filter(|&i| i >= delimiter.len_utf8())
        .ok_or_else(|| anyhow::anyhow!("unterminated regex pattern: {pattern}"))?;
    let inner = &pattern[delimiter.len_utf8()..end];
    let modifiers = &pattern[end + closing.len_utf8()..];

    let flags: String = modifiers
        .chars()
        .filter(|c| matches!(c, 'i' | 'x' | 's' | 'm'))
        .collect();

    // PCRE's `A` (PCRE_ANCHORED) modifier requires the match to start exactly at the search offset,
    // which the `regex` crate cannot express: it anchors a pattern only at the head of the haystack.
    // Anchor the pattern at the call site instead, by searching the sub-slice that begins at the
    // offset with a `^`-prefixed pattern.
    if modifiers.contains('A') {
        anyhow::bail!("anchored (A) regex pattern is not supported: {pattern}");
    }

    let utf8 = modifiers.contains('u');
    let inner = translate_pcre_literals(inner, utf8);
    let source = if flags.is_empty() {
        inner
    } else {
        format!("(?{flags}){inner}")
    };
    Ok((source, utf8))
}

/// The result of resolving a `PregPattern`. Deliberately holds either a shared `Arc` (string
/// patterns, via `PATTERN_CACHE`) or a `'static` reference (the `php_regex!` macro's per-call-site
/// `LazyLock`) rather than an owned regex — `Regex::clone()` does not share the underlying meta
/// engine's search-cache pool, so producing a fresh owned clone here would pay a ~10us per-call
/// cache warmup cost regardless of which path produced it (measured).
#[derive(Debug)]
pub enum ResolvedPattern {
    Cached(Arc<CompiledPattern>),
    Static(&'static CompiledPattern),
}

impl ResolvedPattern {
    fn pattern(&self) -> &CompiledPattern {
        match self {
            Self::Cached(arc) => arc,
            Self::Static(pattern) => pattern,
        }
    }
}

/// Implemented by anything `preg_*` can accept as a pattern: a PHP-style pattern string (parsed
/// and cached in `PATTERN_CACHE`) or an already-compiled `&'static CompiledPattern`, as produced by
/// the `php_regex!` macro.
pub trait PregPattern {
    fn resolve(self) -> ResolvedPattern;
}

impl PregPattern for &str {
    fn resolve(self) -> ResolvedPattern {
        ResolvedPattern::Cached(
            compile_php_pattern(self).unwrap_or_else(|e| panic!("invalid regex: {e}")),
        )
    }
}

impl PregPattern for &String {
    fn resolve(self) -> ResolvedPattern {
        self.as_str().resolve()
    }
}

impl PregPattern for String {
    fn resolve(self) -> ResolvedPattern {
        self.as_str().resolve()
    }
}

// A pattern built from PHP strings may hold bytes that are not valid UTF-8. They are written as
// `\xNN` escapes, which without the `u` modifier match the same byte, as a non-ASCII literal does.
// A backslash right before such a byte escapes it, so it is dropped in favour of the escape.
fn byte_pattern_source(pattern: &BStr) -> std::borrow::Cow<'_, str> {
    if let Some(text) = crate::utf8_text(pattern) {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(pattern.len() * 2);
    for chunk in pattern.utf8_chunks() {
        out.push_str(chunk.valid());
        for byte in chunk.invalid() {
            let trailing_backslashes = out.bytes().rev().take_while(|&b| b == b'\\').count();
            if trailing_backslashes % 2 == 1 {
                out.pop();
            }
            out.push_str(&format!("\\x{byte:02X}"));
        }
    }
    std::borrow::Cow::Owned(out)
}

impl PregPattern for &BStr {
    fn resolve(self) -> ResolvedPattern {
        byte_pattern_source(self).as_ref().resolve()
    }
}

impl PregPattern for &BString {
    fn resolve(self) -> ResolvedPattern {
        self.as_bstr().resolve()
    }
}

impl PregPattern for BString {
    fn resolve(self) -> ResolvedPattern {
        self.as_bstr().resolve()
    }
}

impl PregPattern for &'static CompiledPattern {
    fn resolve(self) -> ResolvedPattern {
        ResolvedPattern::Static(self)
    }
}

// Used by the `php_regex!` macro to compile a PHP pattern.
#[doc(hidden)]
pub fn __php_regex_compile(pattern: &str) -> CompiledPattern {
    compile_php_pattern_uncached(pattern).unwrap_or_else(|e| panic!("invalid regex: {e}"))
}

/// Compiles a PHP-style `preg_*` pattern literal (delimiters + modifiers) into a per-call-site
/// cached `&'static CompiledPattern`, instead of going through the runtime `PATTERN_CACHE` lookup
/// by string key. Expands to a `&'static CompiledPattern`, ready to pass straight into any `preg_*`
/// function.
#[macro_export]
macro_rules! php_regex {
    ($php_pattern:expr $(,)?) => {{
        static RE: ::std::sync::LazyLock<$crate::CompiledPattern> =
            ::std::sync::LazyLock::new(|| $crate::__php_regex_compile($php_pattern));
        &*RE
    }};
}

// Expands a PHP preg replacement template against `caps`, appending bytes to
// `out`. Backreferences are written as `$1`, `${1}`, `\1` or `\\1`; a literal
// `$` or `\` not forming a reference is emitted verbatim. Out-of-range or
// non-participating groups expand to nothing.
fn php_replacement_expand(template: &[u8], caps: &regex::bytes::Captures, out: &mut Vec<u8>) {
    let bytes = template;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() => {
                let (group, consumed) = php_replacement_group(&bytes[i + 1..]);
                if let Some(m) = caps.get(group) {
                    out.extend_from_slice(m.as_bytes());
                }
                i += 1 + consumed;
            }
            b'\\' if i + 1 < bytes.len() && bytes[i + 1] == b'\\' => {
                out.push(b'\\');
                i += 2;
            }
            // A backslash escapes a following `$`, yielding a literal dollar sign (so an escaped
            // `\$1` is not mistaken for the `$1` backreference).
            b'\\' if i + 1 < bytes.len() && bytes[i + 1] == b'$' => {
                out.push(b'$');
                i += 2;
            }
            b'$' if i + 1 < bytes.len() && bytes[i + 1] == b'{' => {
                let rest = &bytes[i + 2..];
                match rest.iter().position(|&b| b == b'}') {
                    Some(c) if c > 0 && rest[..c].iter().all(|b| b.is_ascii_digit()) => {
                        let group = rest[..c]
                            .iter()
                            .fold(0usize, |acc, &d| acc * 10 + usize::from(d - b'0'));
                        if let Some(m) = caps.get(group) {
                            out.extend_from_slice(m.as_bytes());
                        }
                        i += 2 + c + 1;
                    }
                    _ => {
                        out.push(b'$');
                        i += 1;
                    }
                }
            }
            b'$' if i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() => {
                let (group, consumed) = php_replacement_group(&bytes[i + 1..]);
                if let Some(m) = caps.get(group) {
                    out.extend_from_slice(m.as_bytes());
                }
                i += 1 + consumed;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
}

// Reads up to two leading ASCII digits as a PHP backreference group number.
fn php_replacement_group(bytes: &[u8]) -> (usize, usize) {
    let mut group = 0usize;
    let mut consumed = 0usize;
    while consumed < 2 && consumed < bytes.len() && bytes[consumed].is_ascii_digit() {
        group = group * 10 + (bytes[consumed] - b'0') as usize;
        consumed += 1;
    }
    (group, consumed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_pattern_matches_invalid_utf8_literally() {
        let pattern = BString::from(b"{^vendor/\xff\\\xfe$}".to_vec());
        assert!(preg_is_match(&pattern, b"vendor/\xff\xfe"));
        assert!(!preg_is_match(&pattern, b"vendor/\xff\\\xfe"));
        assert!(preg_is_match(
            BString::from(b"{^[\xfe\xff]+$}".to_vec()),
            b"\xff\xfe"
        ));
    }
}
