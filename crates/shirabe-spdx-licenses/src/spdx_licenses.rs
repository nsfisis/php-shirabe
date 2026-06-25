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
        };
        this.load_licenses();
        this.load_exceptions();
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

        // The remaining validation matches a license expression against a recursive PCRE pattern
        // (DEFINE subpatterns plus `(?&name)` recursion for compound AND/OR/WITH expressions). The
        // `regex` crate cannot express recursive subpatterns, so this branch needs a hand-written
        // SPDX expression parser. Left unported pending that work.
        todo!("port SPDX license-expression grammar; recursive PCRE not expressible in regex crate")
    }
}
