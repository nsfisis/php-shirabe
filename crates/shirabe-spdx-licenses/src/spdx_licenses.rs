//! ref: composer/vendor/composer/spdx-licenses/src/SpdxLicenses.php

use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

pub const LICENSES_FILE: &str = "spdx-licenses.json";
pub const EXCEPTIONS_FILE: &str = "spdx-exceptions.json";

// PHP reads the resource files from `dirname(__DIR__) . '/res'` at runtime via
// file_get_contents. Composer's res directory ships with the vendored package; embed it at compile
// time so the data is available without filesystem access.
const LICENSES_JSON: &str =
    include_str!("../../../composer/vendor/composer/spdx-licenses/res/spdx-licenses.json");
const EXCEPTIONS_JSON: &str =
    include_str!("../../../composer/vendor/composer/spdx-licenses/res/spdx-exceptions.json");

#[derive(Debug)]
pub struct SpdxLicenses {
    // [ lowercased license identifier => (identifier, full name, osi certified, deprecated) ]
    licenses: IndexMap<String, (String, String, bool, bool)>,
    // [ lowercased exception identifier => (exception identifier, full name) ]
    exceptions: IndexMap<String, (String, String)>,
    // `self.licenses.keys()`, longest first: see `license_id_end`.
    licenses_by_length_desc: Vec<String>,
    // `self.exceptions.keys()`, longest first: see `license_exception_id_end`.
    exceptions_by_length_desc: Vec<String>,
}

impl Default for SpdxLicenses {
    fn default() -> Self {
        Self::new()
    }
}

impl SpdxLicenses {
    pub fn new() -> Self {
        let mut this = SpdxLicenses {
            licenses: IndexMap::new(),
            exceptions: IndexMap::new(),
            licenses_by_length_desc: Vec::new(),
            exceptions_by_length_desc: Vec::new(),
        };
        this.load_licenses();
        this.load_exceptions();
        this.licenses_by_length_desc = sorted_by_length_desc(this.licenses.keys());
        this.exceptions_by_length_desc = sorted_by_length_desc(this.exceptions.keys());
        this
    }

    /// Returns license metadata by license identifier.
    ///
    /// The returned list is in the form of:
    ///   [ 0 => full name, 1 => osi certified, 2 => link to license text, 3 => deprecation status ]
    pub fn get_license_by_identifier(&self, identifier: &str) -> Option<PhpMixed> {
        let key = identifier.to_lowercase();

        let (identifier, name, is_osi_approved, is_deprecated_license_id) =
            self.licenses.get(&key)?;

        Some(PhpMixed::List(vec![
            PhpMixed::String(name.clone()),
            PhpMixed::Bool(*is_osi_approved),
            PhpMixed::String(format!(
                "https://spdx.org/licenses/{}.html#licenseText",
                identifier
            )),
            PhpMixed::Bool(*is_deprecated_license_id),
        ]))
    }

    /// Returns all licenses information, keyed by the lowercased license identifier.
    ///
    /// Each item is [ 0 => identifier, 1 => full name, 2 => osi certified, 3 => deprecated ].
    pub fn get_licenses(&self) -> PhpMixed {
        let mut out: IndexMap<String, PhpMixed> = IndexMap::new();
        for (key, (identifier, name, is_osi_approved, is_deprecated_license_id)) in &self.licenses {
            out.insert(
                key.clone(),
                PhpMixed::List(vec![
                    PhpMixed::String(identifier.clone()),
                    PhpMixed::String(name.clone()),
                    PhpMixed::Bool(*is_osi_approved),
                    PhpMixed::Bool(*is_deprecated_license_id),
                ]),
            );
        }
        PhpMixed::Array(out)
    }

    /// Returns license exception metadata by license exception identifier.
    ///
    /// The returned list is in the form of:
    ///   [ 0 => full name, 1 => link to license text ]
    pub fn get_exception_by_identifier(&self, identifier: &str) -> Option<PhpMixed> {
        let key = identifier.to_lowercase();

        let (identifier, name) = self.exceptions.get(&key)?;

        Some(PhpMixed::List(vec![
            PhpMixed::String(name.clone()),
            PhpMixed::String(format!(
                "https://spdx.org/licenses/{}.html#licenseExceptionText",
                identifier
            )),
        ]))
    }

    /// Returns the short identifier of a license (or license exception) by full name.
    pub fn get_identifier_by_name(&self, name: &str) -> Option<String> {
        for (identifier, full_name, _, _) in self.licenses.values() {
            if full_name == name {
                return Some(identifier.clone());
            }
        }

        for (identifier, full_name) in self.exceptions.values() {
            if full_name == name {
                return Some(identifier.clone());
            }
        }

        None
    }

    /// Returns the OSI Approved status for a license by identifier.
    pub fn is_osi_approved_by_identifier(&self, identifier: &str) -> bool {
        self.licenses[&identifier.to_lowercase()].2
    }

    /// Returns the deprecation status for a license by identifier.
    pub fn is_deprecated_by_identifier(&self, identifier: &str) -> bool {
        self.licenses[&identifier.to_lowercase()].3
    }

    pub fn validate(&self, license: &str) -> bool {
        // PHP also accepts string[] and joins it with ' OR '. The Rust signature here takes a single
        // &str, matching the string-input path of the PHP code; the array path lives in callers.
        self.is_valid_license_string(license)
    }

    fn load_licenses(&mut self) {
        let parsed: IndexMap<String, (String, bool, bool)> =
            serde_json::from_str(LICENSES_JSON).expect("Missing or invalid license file");
        for (identifier, license) in parsed {
            self.licenses.insert(
                identifier.to_lowercase(),
                (identifier, license.0, license.1, license.2),
            );
        }
    }

    fn load_exceptions(&mut self) {
        let parsed: IndexMap<String, (String,)> =
            serde_json::from_str(EXCEPTIONS_JSON).expect("Missing or invalid exceptions file");
        for (identifier, exception) in parsed {
            self.exceptions
                .insert(identifier.to_lowercase(), (identifier, exception.0));
        }
    }

    fn is_valid_license_string(&self, license: &str) -> bool {
        if self.licenses.contains_key(&license.to_lowercase()) {
            return true;
        }

        // Regex pattern compatibility:
        // The PHP implementation matches the license expression grammar with a single PCRE pattern
        // built from `(?(DEFINE) ...)` subpatterns plus `(?&name)` recursion (parenthesized groups
        // and compound AND/OR/WITH expressions). The `regex` crate cannot express recursive
        // subpatterns, so this is a hand-written recursive-descent parser mirroring the same
        // grammar, rather than a regex.
        //
        // Every subpattern below (idstring, licenseid/licenseexceptionid dictionary matches,
        // whitespace, the optional `+`/`WITH`/`AND`/`OR` continuations) is matched greedily with no
        // set-of-candidates backtracking, unlike PCRE. This is deliberate, not an approximation, but
        // it relies on one guard: idstring's character class ([\pL\pN.-]) never contains a valid
        // separator (whitespace, `:`, `)`, end of input), so a licenseid/licenseexceptionid
        // dictionary match is only accepted if the next byte is NOT an idstring byte (see
        // `license_id_end`) -- otherwise a short dictionary entry could match as a coincidental
        // prefix of a longer run (e.g. `DOC`, itself a real license id, prefixing
        // `DocumentRef-...`) and dead-end the parse instead of falling through to the licenseref
        // alternative. With that guard, the longest accepted dictionary match and the longest
        // idstring/whitespace run are always safe to commit to immediately: stopping any of them
        // early always leaves undroppable leftover characters that no rule can consume. The one
        // apparent exception -- a licenseid that is itself a strict prefix of a longer licenseid,
        // e.g. "GPL-2.0" vs "GPL-2.0+" -- is already covered by simple_expression's own optional
        // trailing `+`, which lands on the same end position either way.
        let bytes = license.as_bytes();
        if let Some(end) = starts_with_ci(bytes, 0, "NONE")
            && matches_full_string(bytes, end)
        {
            return true;
        }
        if let Some(end) = starts_with_ci(bytes, 0, "NOASSERTION")
            && matches_full_string(bytes, end)
        {
            return true;
        }
        if let Some(end) = self.compound_expression_end(bytes, 0)
            && matches_full_string(bytes, end)
        {
            return true;
        }
        false
    }

    /// `(?<compound_expression> (?&compound_head) (?: \s+ (?:AND|OR) \s+ (?&compound_expression))? )`
    ///
    /// Also covers `license_expression := compound_expression | simple_expression`, since
    /// `compound_head`'s first alternative already reduces to a bare `simple_expression`.
    fn compound_expression_end(&self, s: &[u8], pos: usize) -> Option<usize> {
        let head_end = self.compound_head_end(s, pos)?;
        if let Some(after_ws) = ws1_end(s, head_end) {
            for keyword in ["AND", "OR"] {
                if let Some(after_kw) = starts_with_ci(s, after_ws, keyword)
                    && let Some(after_ws2) = ws1_end(s, after_kw)
                    && let Some(ce_end) = self.compound_expression_end(s, after_ws2)
                {
                    return Some(ce_end);
                }
            }
        }
        Some(head_end)
    }

    /// ```text
    /// (?<compound_head>
    ///     (?&simple_expression) ( \s+ WITH \s+ (?&licenseexceptionid))?
    ///         | \( \s* (?&compound_expression) \s* \)
    /// )
    /// ```
    fn compound_head_end(&self, s: &[u8], pos: usize) -> Option<usize> {
        if s.get(pos) == Some(&b'(') {
            let after_open = pos + 1;
            let after_ws1 = ws0_end(s, after_open);
            let ce_end = self.compound_expression_end(s, after_ws1)?;
            let after_ws2 = ws0_end(s, ce_end);
            return starts_with_ci(s, after_ws2, ")");
        }

        let se_end = self.simple_expression_end(s, pos)?;
        if let Some(after_ws) = ws1_end(s, se_end)
            && let Some(after_with) = starts_with_ci(s, after_ws, "WITH")
            && let Some(after_ws2) = ws1_end(s, after_with)
            && let Some(exc_end) = self.license_exception_id_end(s, after_ws2)
        {
            return Some(exc_end);
        }
        Some(se_end)
    }

    /// `(?<simple_expression>(?&licenseid)\+? | (?&licenseid) | (?&licenseref))`
    fn simple_expression_end(&self, s: &[u8], pos: usize) -> Option<usize> {
        if let Some(id_end) = self.license_id_end(s, pos) {
            return Some(starts_with_ci(s, id_end, "+").unwrap_or(id_end));
        }
        self.license_ref_end(s, pos)
    }

    /// `(?<licenseref>(?:DocumentRef-(?&idstring):)?LicenseRef-(?&idstring))`
    fn license_ref_end(&self, s: &[u8], pos: usize) -> Option<usize> {
        if let Some(after_doc_prefix) = starts_with_ci(s, pos, "DocumentRef-") {
            let after_id = idstring_end(s, after_doc_prefix)?;
            let after_colon = starts_with_ci(s, after_id, ":")?;
            let after_prefix = starts_with_ci(s, after_colon, "LicenseRef-")?;
            return idstring_end(s, after_prefix);
        }

        let after_prefix = starts_with_ci(s, pos, "LicenseRef-")?;
        idstring_end(s, after_prefix)
    }

    /// `(?<licenseid>{$licenses})`, taken from `self.licenses`, longest identifier first.
    ///
    /// A candidate is rejected if it is immediately followed by another idstring byte (e.g. `DOC`
    /// is a real license id, but must not match inside `DocumentRef-...`): such a match is always a
    /// dead end anyway (see the note in `is_valid_license_string`), and dictionaries can contain
    /// short identifiers that coincide with a longer non-dictionary run of identifier characters.
    fn license_id_end(&self, s: &[u8], pos: usize) -> Option<usize> {
        self.licenses_by_length_desc
            .iter()
            .filter_map(|key| starts_with_ci(s, pos, key))
            .find(|&end| !s.get(end).is_some_and(|&b| is_idstring_byte(b)))
    }

    /// `(?<licenseexceptionid>{$exceptions})`, taken from `self.exceptions`, longest first. See
    /// `license_id_end` for why a trailing idstring byte disqualifies a candidate.
    fn license_exception_id_end(&self, s: &[u8], pos: usize) -> Option<usize> {
        self.exceptions_by_length_desc
            .iter()
            .filter_map(|key| starts_with_ci(s, pos, key))
            .find(|&end| !s.get(end).is_some_and(|&b| is_idstring_byte(b)))
    }
}

fn sorted_by_length_desc<'a>(keys: impl Iterator<Item = &'a String>) -> Vec<String> {
    let mut keys: Vec<String> = keys.cloned().collect();
    keys.sort_by_key(|k| std::cmp::Reverse(k.len()));
    keys
}

// Regex pattern compatibility:
// PCRE's `$` (without the `/m` modifier) also matches immediately before a single trailing
// newline at the end of the subject; replicated here since it is not implied by any (?&name) rule
// in the DEFINE block.
fn matches_full_string(s: &[u8], end: usize) -> bool {
    end == s.len() || (s.last() == Some(&b'\n') && end == s.len() - 1)
}

/// Matches an ASCII `needle` case-insensitively at `pos` and returns the end position, if any.
fn starts_with_ci(s: &[u8], pos: usize, needle: &str) -> Option<usize> {
    let needle = needle.as_bytes();
    let end = pos.checked_add(needle.len())?;
    let candidate = s.get(pos..end)?;
    candidate.eq_ignore_ascii_case(needle).then_some(end)
}

/// The end of the longest run of bytes matching `pred`, starting at `pos`.
fn byte_run_end(s: &[u8], pos: usize, pred: impl Fn(u8) -> bool) -> usize {
    let mut end = pos;
    for &b in &s[pos..] {
        if !pred(b) {
            break;
        }
        end += 1;
    }
    end
}

// Regex pattern compatibility:
// The pattern has no `/u` modifier, so PCRE classifies each byte on its own as a Latin-1
// codepoint rather than decoding UTF-8, hence `b as char` (a lossless byte-to-codepoint mapping)
// instead of decoding `s` as UTF-8.
fn is_idstring_byte(b: u8) -> bool {
    let c = b as char;
    c.is_alphabetic() || c.is_numeric() || c == '.' || c == '-'
}

/// `(?<idstring>[\pL\pN.-]{1,})`
fn idstring_end(s: &[u8], pos: usize) -> Option<usize> {
    let end = byte_run_end(s, pos, is_idstring_byte);
    (end > pos).then_some(end)
}

/// `\s+`
fn ws1_end(s: &[u8], pos: usize) -> Option<usize> {
    let end = byte_run_end(s, pos, |b| b.is_ascii_whitespace());
    (end > pos).then_some(end)
}

/// `\s*`
fn ws0_end(s: &[u8], pos: usize) -> usize {
    byte_run_end(s, pos, |b| b.is_ascii_whitespace())
}

#[cfg(test)]
mod is_valid_license_string_tests {
    use super::SpdxLicenses;

    // Every case below was cross-checked against `Composer\Spdx\SpdxLicenses::validate()` running
    // under real PHP (composer/vendor/composer/spdx-licenses), which has no test suite of its own
    // to port from.
    #[test]
    fn matches_php_spdx_licenses_validate() {
        let s = SpdxLicenses::new();

        let cases: &[(&str, bool)] = &[
            ("MIT", true),
            ("mit", true),
            ("Mit", true),
            ("MIT+", true),
            ("XXXXX", false),
            ("", false),
            ("NONE", true),
            ("none", true),
            ("NOASSERTION", true),
            ("MIT OR Apache-2.0", true),
            ("MIT AND Apache-2.0", true),
            ("(MIT)", true),
            ("(MIT OR Apache-2.0)", true),
            ("(MIT OR Apache-2.0) AND BSD-3-Clause", true),
            ("MIT OR (Apache-2.0 AND BSD-3-Clause)", true),
            // Identifiers that are a prefix of other identifiers rely on longest-match-first.
            ("GPL-2.0-only", true),
            ("GPL-2.0", true),
            ("GPL-2.0+", true),
            ("AGPL-1.0", true),
            ("AGPL-1.0-only", true),
            ("AGPL-1.0-or-later", true),
            ("BSD-2-Clause", true),
            ("BSD-2-Clause-Patent", true),
            ("GPL-2.0-or-later WITH Classpath-exception-2.0", true),
            ("GPL-2.0 WITH Classpath-exception-2.0", true),
            ("MIT WITH Bogus-exception", false),
            ("LicenseRef-foo", true),
            ("LicenseRef-", false),
            ("LicenseRef-foo.bar-1", true),
            ("DocumentRef-doc1:LicenseRef-foo", true),
            ("DocumentRef-doc1LicenseRef-foo", false),
            ("DocumentRef-doc1:LicenseRef-", false),
            ("MIT OR", false),
            ("OR MIT", false),
            ("MIT OR OR Apache-2.0", false),
            ("MIT  OR   Apache-2.0", true),
            ("MIT\tOR\nApache-2.0", true),
            ("(MIT", false),
            ("MIT)", false),
            ("((MIT))", true),
            ("((MIT)", false),
            ("MIT AND", false),
            ("MIT OR MIT", true),
            ("proprietary", false),
            ("MIT AND Apache-2.0 OR BSD-3-Clause", true),
            // PCRE's `$` (without `/m`) also matches right before one trailing newline.
            ("MIT\n", true),
            ("MIT\n\n", false),
            (" MIT", false),
            ("MIT ", false),
            ("MIT+ OR Apache-2.0", true),
            ("0BSD", true),
            ("GPL-2.0-with-classpath-exception", true),
            // No `/u` modifier: idstring bytes are classified as Latin-1, not decoded as UTF-8.
            ("LicenseRef-日本語", false),
            ("LicenseRef-café", false),
            ("LicenseRef-ª", true),
            ("DocumentRef-日:LicenseRef-x", false),
            // "DOC" is itself a real (deprecated) license id and a literal prefix of
            // "DocumentRef-...": a dictionary match must not be accepted here (also see
            // "DocumentRef-doc1:LicenseRef-foo" above, which exercises the same collision).
            ("DOC", true),
            ("DOCxyz", false),
        ];

        for (license, expected) in cases {
            assert_eq!(
                s.validate(license),
                *expected,
                "validate({license:?}) should be {expected}"
            );
        }
    }
}
