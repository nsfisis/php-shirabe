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
//! See docs/dev/regex-porting.md for the regex porting rules.

use indexmap::IndexMap;
use std::sync::{Arc, LazyLock, Mutex};

/// A single match's `$matches`: the `regex::Captures` the search produced, read by either the named
/// or the numbered form of a capture group. `'h` is the lifetime of the searched subject, which the
/// group values borrow from.
#[derive(Debug)]
pub struct PregMatches<'h> {
    caps: regex::Captures<'h>,
}

impl<'h> PregMatches<'h> {
    fn new(caps: regex::Captures<'h>) -> Self {
        Self { caps }
    }

    /// The value of the capture group at `index`, or `None` if that group did not participate in
    /// the match. A group the pattern does not have reads as `None` too, matching how PHP reports
    /// a `$matches` entry that is not there.
    pub fn get(&self, index: usize) -> Option<&'h str> {
        self.caps.get(index).map(|group| group.as_str())
    }

    /// The value of the capture group called `name`, under the same rules as `get`.
    pub fn name(&self, name: &str) -> Option<&'h str> {
        self.caps.name(name).map(|group| group.as_str())
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

pub fn preg_quote(str: &str, delimiter: Option<char>) -> String {
    // Regex pattern compatibility:
    // PHP's preg_quote escapes `<` and `>` (PCRE treats `\<`/`\>` as literals), but the `regex`
    // crate reads `\<`/`\>` as start-of-word / end-of-word boundary assertions. `<` and `>` are
    // already literal in the `regex` crate, so they are emitted unescaped to preserve the intended
    // literal match.
    const SPECIAL: &str = ".\\+*?[^]$(){}=!|:-#";
    let mut out = String::new();
    for c in str.chars() {
        if c == '\0' {
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

// Whether the pattern matches, for the call sites that ignore the capture groups.
pub fn preg_is_match(pattern: impl PregPattern, subject: &str) -> bool {
    let __resolved = pattern.resolve();
    let re = __resolved.regex();

    re.is_match(subject)
}

// Returns None if the pattern did not match; otherwise the match's capture groups.
pub fn preg_match<'h>(pattern: impl PregPattern, subject: &'h str) -> Option<PregMatches<'h>> {
    let __resolved = pattern.resolve();
    let re = __resolved.regex();
    let caps = re.captures(subject)?;

    Some(PregMatches::new(caps))
}

// Every occurrence of the pattern in `subject`, in match order. The search runs eagerly, as PHP's
// does: a match borrows `subject` alone, so the matches outlive the compiled pattern, which is only
// resolved for the duration of this call.
pub fn preg_match_all<'h>(
    pattern: impl PregPattern,
    subject: &'h str,
) -> impl Iterator<Item = PregMatches<'h>> {
    let __resolved = pattern.resolve();
    let matches: Vec<PregMatches<'h>> = __resolved
        .regex()
        .captures_iter(subject)
        .map(PregMatches::new)
        .collect();

    matches.into_iter()
}

pub fn preg_grep<T: AsRef<str>>(
    pattern: impl PregPattern,
    array: impl IntoIterator<Item = T>,
) -> impl Iterator<Item = T> {
    let __resolved = pattern.resolve();
    array.into_iter().filter(move |s| {
        let re = __resolved.regex();
        re.is_match(s.as_ref())
    })
}

pub fn preg_split(pattern: impl PregPattern, subject: &str) -> Vec<String> {
    preg_split_impl(pattern, subject, false)
}

pub fn preg_split_delim_capture(pattern: impl PregPattern, subject: &str) -> Vec<String> {
    preg_split_impl(pattern, subject, true)
}

fn preg_split_impl(pattern: impl PregPattern, subject: &str, delim_capture: bool) -> Vec<String> {
    let __resolved = pattern.resolve();
    let re = __resolved.regex();

    let mut result: Vec<String> = Vec::new();
    let mut last = 0usize;
    for caps in re.captures_iter(subject) {
        let m = caps.get(0).unwrap();
        result.push(subject[last..m.start()].to_string());
        if delim_capture {
            // `preg_split` accepts no PREG_UNMATCHED_AS_NULL, so the split list
            // holds strings only: trailing unmatched groups are dropped,
            // interior ones are emitted as "".
            if let Some(last_g) = (1..caps.len()).rev().find(|&g| caps.get(g).is_some()) {
                for g in 1..=last_g {
                    result.push(caps.get(g).map(|x| x.as_str()).unwrap_or("").to_string());
                }
            }
        }
        last = m.end();
    }
    result.push(subject[last..].to_string());

    result
}

pub fn preg_replace(pattern: impl PregPattern, replacement: &str, subject: &str) -> String {
    preg_replace2(pattern, replacement, subject, -1, None)
}

pub fn preg_replace2(
    pattern: impl PregPattern,
    replacement: &str,
    subject: &str,
    limit: i64,
    count: Option<&mut usize>,
) -> String {
    let __resolved = pattern.resolve();
    let re = __resolved.regex();
    let limit = if limit < 0 {
        usize::MAX
    } else {
        limit as usize
    };

    let mut out: Vec<u8> = Vec::new();
    let mut last = 0usize;
    let mut n = 0usize;
    for caps in re.captures_iter(subject) {
        if n >= limit {
            break;
        }
        let m = caps.get(0).unwrap();
        out.extend_from_slice(&subject.as_bytes()[last..m.start()]);
        php_replacement_expand(replacement, &caps, &mut out);
        last = m.end();
        n += 1;
    }
    out.extend_from_slice(&subject.as_bytes()[last..]);

    if let Some(count) = count {
        *count = n;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn preg_replace_callback<'h, F>(
    pattern: impl PregPattern,
    mut callback: F,
    subject: &'h str,
) -> anyhow::Result<String>
where
    F: FnMut(&PregMatches<'h>) -> anyhow::Result<String>,
{
    let __resolved = pattern.resolve();
    let re = __resolved.regex();

    let mut out: Vec<u8> = Vec::new();
    let mut last = 0usize;
    for caps in re.captures_iter(subject) {
        let m = caps.get(0).unwrap();
        out.extend_from_slice(&subject.as_bytes()[last..m.start()]);
        let matches = PregMatches::new(caps);
        out.extend_from_slice(callback(&matches)?.as_bytes());
        last = m.end();
    }
    out.extend_from_slice(&subject.as_bytes()[last..]);

    Ok(String::from_utf8_lossy(&out).into_owned())
}

// Translates a PHP PCRE pattern (delimiters + trailing modifiers) into a regex
// the `regex` crate can compile. Only delimiter stripping and the i/x/s/m
// modifiers are handled; PCRE-only constructs (possessive quantifiers,
// lookaround, backreferences) are not supported by `regex` and must be avoided
// in the caller's pattern.
// TODO(pcre): replace with a faithful PCRE engine to restore full semantics.
// PCRE treats `\<` and `\>` as escaped literal `<`/`>`, but the `regex` crate
// reads them as start/end-of-word boundary assertions. Rewrite those escapes to
// the literal characters so PCRE-sourced patterns (e.g. anything run through
// `preg_quote`, which escapes `<` and `>`) keep their original meaning. A `\\`
// escapes the following backslash, so `\\<` is left untouched.
fn translate_pcre_literals(inner: &str) -> String {
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
        } else {
            out.push(c);
        }
    }
    out
}

// PHP's PCRE engine keeps a per-process cache of compiled patterns (pcre.cache_size, default 4096),
// so repeated preg_* calls with the same pattern string are effectively free. The `regex` crate has
// no such cache, and callers like the classmap generator re-issue the same pattern string for every
// file (or even every scanned token), so compilation must be memoized here to match PHP's amortized
// cost.
// The cached value is `Arc`-wrapped so callers share a single `regex::Regex` instance:
// `regex::Regex::clone()` does not share the underlying meta engine's search-cache pool, so
// handing out fresh clones here would pay a ~10us per-clone cache warmup cost on every single
// `preg_*` call (measured), defeating the point of this cache. `Arc::clone()` is a refcount bump.
static PATTERN_CACHE: LazyLock<Mutex<IndexMap<String, Arc<regex::Regex>>>> =
    LazyLock::new(|| Mutex::new(IndexMap::new()));

fn compile_php_pattern(pattern: &str) -> anyhow::Result<Arc<regex::Regex>> {
    if let Some(cached) = PATTERN_CACHE.lock().unwrap().get(pattern) {
        return Ok(Arc::clone(cached));
    }

    let compiled = Arc::new(regex::Regex::new(&translate_php_pattern(pattern)?)?);
    PATTERN_CACHE
        .lock()
        .unwrap()
        .insert(pattern.to_string(), Arc::clone(&compiled));
    Ok(compiled)
}

// Strips PHP-style delimiters and modifiers from `pattern` and translates the body into
// `regex`-crate syntax, without compiling it.
fn translate_php_pattern(pattern: &str) -> anyhow::Result<String> {
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

    let inner = translate_pcre_literals(inner);
    Ok(if flags.is_empty() {
        inner
    } else {
        format!("(?{flags}){inner}")
    })
}

/// The result of resolving a `PregPattern`. Deliberately holds either a shared `Arc` (string
/// patterns, via `PATTERN_CACHE`) or a `'static` reference (the `php_regex!` macro's per-call-site
/// `LazyLock<Regex>`) rather than an owned `regex::Regex` — `regex::Regex::clone()` does not share
/// the underlying meta engine's search-cache pool, so producing a fresh owned clone here would pay
/// a ~10us per-call cache warmup cost regardless of which path produced it (measured).
#[derive(Debug)]
pub enum ResolvedPattern {
    Cached(Arc<regex::Regex>),
    Static(&'static regex::Regex),
}

impl ResolvedPattern {
    pub fn regex(&self) -> &regex::Regex {
        match self {
            Self::Cached(arc) => arc,
            Self::Static(re) => re,
        }
    }
}

/// Implemented by anything `preg_*` can accept as a pattern: a PHP-style pattern string (parsed
/// and cached in `PATTERN_CACHE`) or an already-compiled `&'static regex::Regex`, as produced by
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

impl PregPattern for &'static regex::Regex {
    fn resolve(self) -> ResolvedPattern {
        ResolvedPattern::Static(self)
    }
}

// Used by the `php_regex!` macro to obtain the `regex`-crate-syntax source for a PHP pattern.
pub fn php_regex_source(pattern: &str) -> String {
    translate_php_pattern(pattern).unwrap_or_else(|e| panic!("invalid regex: {e}"))
}

/// Wraps `regex_macro::regex!` so a PHP-style `preg_*` pattern literal (delimiters + modifiers)
/// compiles to a per-call-site cached `&'static regex::Regex`, instead of going through the
/// runtime `PATTERN_CACHE` lookup by string key. Expands to a `&'static regex::Regex`, ready to
/// pass straight into any `preg_*` function.
// TODO(pcre): `$php_pattern` is still translated from PHP delimiter/modifier syntax at runtime (on
// first use at each call site). Once call sites pass native `regex`-crate syntax directly, drop
// this wrapper and call `regex_macro::regex!` directly.
#[macro_export]
macro_rules! php_regex {
    ($php_pattern:expr $(,)?) => {
        &**$crate::regex!(&$crate::php_regex_source($php_pattern))
    };
}

// Expands a PHP preg replacement template against `caps`, appending bytes to
// `out`. Backreferences are written as `$1`, `${1}`, `\1` or `\\1`; a literal
// `$` or `\` not forming a reference is emitted verbatim. Out-of-range or
// non-participating groups expand to nothing.
fn php_replacement_expand(template: &str, caps: &regex::Captures, out: &mut Vec<u8>) {
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() => {
                let (group, consumed) = php_replacement_group(&bytes[i + 1..]);
                if let Some(m) = caps.get(group) {
                    out.extend_from_slice(m.as_str().as_bytes());
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
                        let group: usize =
                            std::str::from_utf8(&rest[..c]).unwrap().parse().unwrap();
                        if let Some(m) = caps.get(group) {
                            out.extend_from_slice(m.as_str().as_bytes());
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
                    out.extend_from_slice(m.as_str().as_bytes());
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
