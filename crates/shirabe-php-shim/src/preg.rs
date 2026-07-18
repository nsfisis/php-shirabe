use indexmap::IndexMap;
use std::sync::{Arc, LazyLock, Mutex};

pub const PREG_PATTERN_ORDER: i64 = 1;
pub const PREG_SET_ORDER: i64 = 2;
pub const PREG_OFFSET_CAPTURE: i64 = 256;
pub const PREG_UNMATCHED_AS_NULL: i64 = 512;
pub const PREG_SPLIT_NO_EMPTY: i64 = 1;
pub const PREG_SPLIT_DELIM_CAPTURE: i64 = 2;
pub const PREG_SPLIT_OFFSET_CAPTURE: i64 = 4;
pub const PREG_GREP_INVERT: i64 = 1;

#[derive(Debug, Clone, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub enum CaptureKey {
    ByIndex(usize),
    ByName(String),
}

#[derive(Debug, Default)]
pub struct PregOffsetCaptureMatches {
    groups: Vec<Vec<(String, usize)>>,
}

impl PregOffsetCaptureMatches {
    pub fn group(&self, i: usize) -> &[(String, usize)] {
        &self.groups[i]
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

// Returns whether the pattern matched; populates matches[0]=full match, matches[1..]=captures.
// Optional groups that did not participate in the match are stored as None.
pub fn preg_match(
    pattern: impl PregPattern,
    subject: &str,
    matches: &mut Vec<Option<String>>,
) -> bool {
    let __resolved = pattern.resolve();
    let (re, _anchored) = __resolved.parts();
    matches.clear();
    match re.captures(subject) {
        Some(caps) => {
            for g in 0..caps.len() {
                matches.push(caps.get(g).map(|m| m.as_str().to_string()));
            }
            true
        }
        None => false,
    }
}

pub fn preg_replace(pattern: impl PregPattern, replacement: &str, subject: &str) -> String {
    preg_replace2(pattern, replacement, subject, -1, None)
}

pub fn preg_split(pattern: impl PregPattern, subject: &str) -> Vec<String> {
    preg_split2(pattern, subject, -1, 0)
}

// PREG_PATTERN_ORDER: the outer vec is indexed by capture group, the inner by
// match occurrence. Non-participating groups are reported as "".
pub fn preg_match_all(pattern: impl PregPattern, subject: &str) -> Vec<Vec<String>> {
    let __resolved = pattern.resolve();
    let (re, _anchored) = __resolved.parts();
    let group_count = re.captures_len();
    let mut groups: Vec<Vec<String>> = vec![Vec::new(); group_count];
    for caps in re.captures_iter(subject) {
        for (g, group) in groups.iter_mut().enumerate() {
            group.push(
                caps.get(g)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default(),
            );
        }
    }
    groups
}

// PREG_SET_ORDER: the outer vec is indexed by match occurrence, the inner by
// capture group (a classic `$matches` row).
pub fn preg_match_all_set_order(
    pattern: impl PregPattern,
    subject: &str,
    matches: &mut Vec<Vec<String>>,
) -> usize {
    let __resolved = pattern.resolve();
    let (re, _anchored) = __resolved.parts();
    let mut rows: Vec<Vec<String>> = Vec::new();
    for caps in re.captures_iter(subject) {
        rows.push(php_match_row(&caps));
    }
    let count = rows.len();
    *matches = rows;
    count
}

pub fn preg_grep(pattern: impl PregPattern, input: &[String]) -> Vec<String> {
    let __resolved = pattern.resolve();
    let (re, _anchored) = __resolved.parts();
    input.iter().filter(|s| re.is_match(s)).cloned().collect()
}

pub fn preg_match_all_offset_capture(
    pattern: impl PregPattern,
    subject: &str,
    matches: &mut PregOffsetCaptureMatches,
) -> usize {
    let __resolved = pattern.resolve();
    let (re, _anchored) = __resolved.parts();
    let group_count = re.captures_len();
    matches.groups = vec![Vec::new(); group_count];

    let mut count = 0;
    for caps in re.captures_iter(subject) {
        count += 1;
        for g in 0..group_count {
            // PHP stores ["", -1] for non-participating groups under
            // PREG_OFFSET_CAPTURE; the unsigned offset here approximates -1 as 0,
            // which callers must not rely on for absent groups.
            let entry = caps
                .get(g)
                .map(|m| (m.as_str().to_string(), m.start()))
                .unwrap_or_else(|| (String::new(), 0));
            matches.groups[g].push(entry);
        }
    }

    count
}

pub fn preg_replace_callback<F>(
    pattern: impl PregPattern,
    mut callback: F,
    subject: &str,
) -> anyhow::Result<String>
where
    F: FnMut(&[Option<String>]) -> anyhow::Result<String>,
{
    let __resolved = pattern.resolve();
    let (re, _anchored) = __resolved.parts();
    let mut out: Vec<u8> = Vec::new();
    let mut last = 0;
    for caps in re.captures_iter(subject) {
        let m = caps.get(0).unwrap();
        out.extend_from_slice(&subject.as_bytes()[last..m.start()]);
        let groups: Vec<Option<String>> = (0..caps.len())
            .map(|g| caps.get(g).map(|x| x.as_str().to_string()))
            .collect();
        let replaced = callback(&groups)?;
        out.extend_from_slice(replaced.as_bytes());
        last = m.end();
    }
    out.extend_from_slice(&subject.as_bytes()[last..]);
    Ok(String::from_utf8_lossy(&out).into_owned())
}

// Returns whether the pattern matched. Unmatched groups are reported as None
// (PREG_UNMATCHED_AS_NULL).
pub fn preg_match2(
    pattern: impl PregPattern,
    subject: &str,
    matches: &mut indexmap::IndexMap<CaptureKey, Option<String>>,
    flags: i64,
    offset: usize,
) -> bool {
    let __resolved = pattern.resolve();
    let (re, anchored) = __resolved.parts();
    let unmatched_as_null = flags & PREG_UNMATCHED_AS_NULL != 0;
    // An anchored (`A`) pattern must match starting exactly at `offset`; the `regex` crate cannot
    // anchor a `captures_at` search, so search the sub-slice beginning at `offset` and require the
    // match to start at its head.
    let caps = if anchored {
        re.captures(&subject[offset..])
            .filter(|c| c.get(0).map(|m| m.start()) == Some(0))
    } else {
        re.captures_at(subject, offset)
    };

    matches.clear();
    if let Some(caps) = &caps {
        let names: Vec<Option<&str>> = re.capture_names().collect();
        *matches = single_match_map(caps, &names, unmatched_as_null);
    }

    caps.is_some()
}

pub fn preg_match_all2(
    pattern: impl PregPattern,
    subject: &str,
    matches: &mut indexmap::IndexMap<CaptureKey, Vec<Option<String>>>,
    flags: i64,
    offset: usize,
) -> usize {
    let __resolved = pattern.resolve();
    let (re, _anchored) = __resolved.parts();
    let unmatched_as_null = flags & PREG_UNMATCHED_AS_NULL != 0;
    let group_count = re.captures_len();
    let names: Vec<Option<&str>> = re.capture_names().collect();

    // PREG_PATTERN_ORDER: one column per group, one row per match occurrence.
    let mut groups: Vec<Vec<Option<String>>> = vec![Vec::new(); group_count];
    let mut count = 0;
    for caps in re.captures_iter(&subject[offset..]) {
        count += 1;
        for (g, column) in groups.iter_mut().enumerate() {
            let value = caps.get(g).map(|m| m.as_str().to_string());
            column.push(if unmatched_as_null {
                value
            } else {
                Some(value.unwrap_or_default())
            });
        }
    }

    matches.clear();
    for (g, column) in groups.into_iter().enumerate() {
        if let Some(Some(name)) = names.get(g) {
            matches.insert(CaptureKey::ByName((*name).to_string()), column.clone());
        }
        matches.insert(CaptureKey::ByIndex(g), column);
    }

    count
}

pub fn preg_match_all_offset_capture2(
    pattern: impl PregPattern,
    subject: &str,
    matches: &mut indexmap::IndexMap<CaptureKey, Vec<(Option<String>, i64)>>,
    flags: i64,
    offset: usize,
) -> usize {
    let __resolved = pattern.resolve();
    let (re, _anchored) = __resolved.parts();
    let unmatched_as_null = flags & PREG_UNMATCHED_AS_NULL != 0;
    let group_count = re.captures_len();
    let names: Vec<Option<&str>> = re.capture_names().collect();

    let mut groups: Vec<Vec<(Option<String>, i64)>> = vec![Vec::new(); group_count];
    let mut count = 0;
    for caps in re.captures_iter(&subject[offset..]) {
        count += 1;
        for (g, column) in groups.iter_mut().enumerate() {
            let entry = match caps.get(g) {
                Some(m) => (Some(m.as_str().to_string()), (m.start() + offset) as i64),
                None if unmatched_as_null => (None, -1),
                None => (Some(String::new()), -1),
            };
            column.push(entry);
        }
    }

    matches.clear();
    for (g, column) in groups.into_iter().enumerate() {
        if let Some(Some(name)) = names.get(g) {
            matches.insert(CaptureKey::ByName((*name).to_string()), column.clone());
        }
        matches.insert(CaptureKey::ByIndex(g), column);
    }

    count
}

pub fn preg_replace2(
    pattern: impl PregPattern,
    replacement: &str,
    subject: &str,
    limit: i64,
    count: Option<&mut usize>,
) -> String {
    let __resolved = pattern.resolve();
    let (re, _anchored) = __resolved.parts();
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

pub fn preg_replace_callback2<
    F: FnMut(&indexmap::IndexMap<CaptureKey, Option<String>>) -> String,
>(
    pattern: impl PregPattern,
    mut callback: F,
    subject: &str,
    limit: i64,
    count: Option<&mut usize>,
    flags: i64,
) -> String {
    let __resolved = pattern.resolve();
    let (re, _anchored) = __resolved.parts();
    let unmatched_as_null = flags & PREG_UNMATCHED_AS_NULL != 0;
    let names: Vec<Option<&str>> = re.capture_names().collect();
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
        let map = single_match_map(&caps, &names, unmatched_as_null);
        out.extend_from_slice(callback(&map).as_bytes());
        last = m.end();
        n += 1;
    }
    out.extend_from_slice(&subject.as_bytes()[last..]);

    if let Some(count) = count {
        *count = n;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn preg_split2(
    pattern: impl PregPattern,
    subject: &str,
    limit: i64,
    flags: i64,
) -> Vec<String> {
    let __resolved = pattern.resolve();
    let (re, _anchored) = __resolved.parts();
    let no_empty = flags & PREG_SPLIT_NO_EMPTY != 0;
    let delim_capture = flags & PREG_SPLIT_DELIM_CAPTURE != 0;
    // `limit` counts the resulting pieces; a non-positive value means no limit.
    let max_delims = if limit > 0 {
        (limit as usize).saturating_sub(1)
    } else {
        usize::MAX
    };

    let mut result: Vec<String> = Vec::new();
    let push = |s: &str, result: &mut Vec<String>| {
        if !(no_empty && s.is_empty()) {
            result.push(s.to_string());
        }
    };

    let mut last = 0usize;
    for (delims, caps) in re.captures_iter(subject).enumerate() {
        if delims >= max_delims {
            break;
        }
        let m = caps.get(0).unwrap();
        push(&subject[last..m.start()], &mut result);
        if delim_capture {
            // Mirror preg_match: trailing unmatched groups are dropped, interior
            // unmatched groups are emitted as "".
            if let Some(last_g) = (1..caps.len()).rev().find(|&g| caps.get(g).is_some()) {
                for g in 1..=last_g {
                    push(caps.get(g).map(|x| x.as_str()).unwrap_or(""), &mut result);
                }
            }
        }
        last = m.end();
    }
    push(&subject[last..], &mut result);

    result
}

pub fn preg_grep2(pattern: impl PregPattern, array: &[&str], flags: i64) -> Vec<String> {
    let __resolved = pattern.resolve();
    let (re, _anchored) = __resolved.parts();
    let invert = flags & PREG_GREP_INVERT != 0;
    array
        .iter()
        .filter(|s| re.is_match(s) != invert)
        .map(|s| s.to_string())
        .collect()
}

// Translates a PHP PCRE pattern (delimiters + trailing modifiers) into a regex
// the `regex` crate can compile. Only delimiter stripping and the i/x/s/m
// modifiers are handled; PCRE-only constructs (possessive quantifiers,
// lookaround, backreferences) are not supported by `regex` and must be avoided
// in the caller's pattern.
// TODO(phase-c): replace with a faithful PCRE engine to restore full semantics.
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
static PATTERN_CACHE: LazyLock<Mutex<IndexMap<String, Arc<(regex::Regex, bool)>>>> =
    LazyLock::new(|| Mutex::new(IndexMap::new()));

fn compile_php_pattern(pattern: &str) -> anyhow::Result<Arc<(regex::Regex, bool)>> {
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

fn compile_php_pattern_uncached(pattern: &str) -> anyhow::Result<(regex::Regex, bool)> {
    let (translated, anchored) = translate_php_pattern(pattern)?;
    Ok((regex::Regex::new(&translated)?, anchored))
}

// Strips PHP-style delimiters and modifiers from `pattern` and translates the body into
// `regex`-crate syntax, without compiling it. Returns the translated source alongside whether the
// PCRE `A` (anchored) modifier was present.
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

    // PCRE's `A` (PCRE_ANCHORED) modifier requires the match to start exactly at the search offset.
    // The `regex` crate has no per-search anchoring, so the offset-based callers
    // (`preg_match2`/`preg_match_all2`) honour it by searching a sub-slice that begins at the offset;
    // here we only surface the flag.
    let anchored = modifiers.contains('A');

    let inner = translate_pcre_literals(inner);
    let translated = if flags.is_empty() {
        inner
    } else {
        format!("(?{flags}){inner}")
    };

    Ok((translated, anchored))
}

/// The result of resolving a `PregPattern`. Deliberately holds either a shared `Arc` (string
/// patterns, via `PATTERN_CACHE`) or a `'static` reference (the `php_regex!` macro's per-call-site
/// `LazyLock<Regex>`) rather than an owned `regex::Regex` — `regex::Regex::clone()` does not share
/// the underlying meta engine's search-cache pool, so producing a fresh owned clone here would pay
/// a ~10us per-call cache warmup cost regardless of which path produced it (measured).
pub enum ResolvedPattern {
    Cached(Arc<(regex::Regex, bool)>),
    Static(&'static regex::Regex, bool),
}

impl ResolvedPattern {
    pub fn parts(&self) -> (&regex::Regex, bool) {
        match self {
            Self::Cached(arc) => (&arc.0, arc.1),
            Self::Static(re, anchored) => (re, *anchored),
        }
    }
}

/// Implemented by anything `preg_*` can accept as a pattern: a PHP-style pattern string (parsed
/// and cached in `PATTERN_CACHE`) or an already-compiled `&'static regex::Regex` paired with its
/// PCRE `A` (anchored) flag, as produced by the `php_regex!` macro.
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

impl PregPattern for (&'static regex::Regex, bool) {
    fn resolve(self) -> ResolvedPattern {
        ResolvedPattern::Static(self.0, self.1)
    }
}

// Used by the `php_regex!` macro to obtain the `regex`-crate-syntax source for a PHP pattern.
pub fn php_regex_source(pattern: &str) -> String {
    translate_php_pattern(pattern)
        .unwrap_or_else(|e| panic!("invalid regex: {e}"))
        .0
}

// Used by the `php_regex!` macro to obtain the PCRE `A` (anchored) modifier flag for a PHP
// pattern.
pub fn php_regex_anchored(pattern: &str) -> bool {
    translate_php_pattern(pattern)
        .unwrap_or_else(|e| panic!("invalid regex: {e}"))
        .1
}

/// Wraps `regex_macro::regex!` so a PHP-style `preg_*` pattern literal (delimiters + modifiers)
/// compiles to a per-call-site cached `&'static regex::Regex`, instead of going through the
/// runtime `PATTERN_CACHE` lookup by string key. Expands to a `(&'static regex::Regex, bool)`
/// tuple, ready to pass straight into any `preg_*` function.
// TODO: `$php_pattern` is still translated from PHP delimiter/modifier syntax at runtime (on
// first use at each call site). Once call sites pass native `regex`-crate syntax directly, drop
// this wrapper and call `regex_macro::regex!` directly.
#[macro_export]
macro_rules! php_regex {
    ($php_pattern:expr $(,)?) => {
        (
            &**$crate::regex!(&$crate::php_regex_source($php_pattern)),
            $crate::php_regex_anchored($php_pattern),
        )
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

// Classic preg_match `$matches` row: index 0 is the full match, trailing
// unmatched groups are truncated and interior unmatched groups become "".
fn php_match_row(caps: &regex::Captures) -> Vec<String> {
    let last = (0..caps.len())
        .rev()
        .find(|&g| caps.get(g).is_some())
        .unwrap_or(0);
    (0..=last)
        .map(|g| {
            caps.get(g)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default()
        })
        .collect()
}

// Builds a single match's `$matches` map with both named and numbered keys
// (the named key precedes its number). With PREG_UNMATCHED_AS_NULL, every group
// is present and non-participating ones are None; otherwise classic semantics
// apply: trailing unmatched groups are dropped and interior ones become "".
fn single_match_map(
    caps: &regex::Captures,
    names: &[Option<&str>],
    unmatched_as_null: bool,
) -> indexmap::IndexMap<CaptureKey, Option<String>> {
    let mut out = indexmap::IndexMap::new();
    let group_count = caps.len();
    let last_participating = (0..group_count).rev().find(|&i| caps.get(i).is_some());

    for i in 0..group_count {
        let m = caps.get(i);
        if !unmatched_as_null
            && m.is_none()
            && let Some(last) = last_participating
            && i > last
        {
            break;
        }
        let value = if unmatched_as_null {
            m.map(|m| m.as_str().to_string())
        } else {
            Some(m.map(|m| m.as_str().to_string()).unwrap_or_default())
        };
        if let Some(Some(name)) = names.get(i) {
            out.insert(CaptureKey::ByName((*name).to_string()), value.clone());
        }
        out.insert(CaptureKey::ByIndex(i), value);
    }
    out
}
