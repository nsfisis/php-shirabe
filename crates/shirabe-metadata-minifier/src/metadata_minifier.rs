//! ref: composer/vendor/composer/metadata-minifier/src/MetadataMinifier.php

use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct MetadataMinifier;

impl MetadataMinifier {
    /// Expands an array of minified versions, keeping each expanded version as a table of
    /// references into `versions` rather than a copy of the values. Callers that discard most
    /// versions (a repository filtering by constraint and stability, say) pay for a deep copy only
    /// on the ones they keep.
    pub fn expand(versions: Vec<IndexMap<String, PhpMixed>>) -> ExpandedVersions {
        let mut keys: IndexMap<String, u32> = IndexMap::new();
        let mut views: Vec<Vec<Entry>> = Vec::with_capacity(versions.len());
        let mut current: Vec<Entry> = Vec::new();

        for (version_index, version_data) in versions.iter().enumerate() {
            let version_index = version_index as u32;

            if current.is_empty() {
                for (entry_index, (key, _)) in version_data.iter().enumerate() {
                    let key = intern(&mut keys, key);
                    current.push(Entry {
                        key,
                        value: Slot::Minified {
                            version_index,
                            entry_index: entry_index as u32,
                        },
                    });
                }
                views.push(current.clone());
                continue;
            }

            // add any changes from the previous version to the expanded one
            for (entry_index, (key, val)) in version_data.iter().enumerate() {
                let key = intern(&mut keys, key);
                if matches!(val, PhpMixed::String(s) if s == "__unset") {
                    if let Some(position) = current.iter().position(|entry| entry.key == key) {
                        current.remove(position);
                    }
                } else {
                    let value = Slot::Minified {
                        version_index,
                        entry_index: entry_index as u32,
                    };
                    match current.iter_mut().find(|entry| entry.key == key) {
                        Some(entry) => entry.value = value,
                        None => current.push(Entry { key, value }),
                    }
                }
            }

            views.push(current.clone());
        }

        ExpandedVersions {
            source: versions,
            keys,
            views,
            overrides: Vec::new(),
        }
    }

    // MetadataMinifier::minify() is not ported because it is not used in Composer itself.
    // The function is mainly for package repositories.
}

fn intern(keys: &mut IndexMap<String, u32>, key: &str) -> u32 {
    if let Some(&id) = keys.get(key) {
        return id;
    }
    let id = keys.len() as u32;
    keys.insert(key.to_string(), id);
    id
}

/// One field of one expanded version.
#[derive(Debug, Clone, Copy)]
struct Entry {
    key: u32,
    value: Slot,
}

#[derive(Debug, Clone, Copy)]
enum Slot {
    /// A value of the minified input: the version that last wrote the field, and the position the
    /// field has within it.
    Minified {
        version_index: u32,
        entry_index: u32,
    },
    /// A value written by [`ExpandedVersions::set`], at this position in `overrides`.
    Overridden(u32),
}

/// The result of [`MetadataMinifier::expand`]: the minified input plus, for each expanded
/// version, the fields it ends up with and where their values live.
#[derive(Debug)]
pub struct ExpandedVersions {
    source: Vec<IndexMap<String, PhpMixed>>,
    keys: IndexMap<String, u32>,
    views: Vec<Vec<Entry>>,
    overrides: Vec<PhpMixed>,
}

impl ExpandedVersions {
    pub fn len(&self) -> usize {
        self.views.len()
    }

    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }

    pub fn version(&self, index: usize) -> ExpandedVersion<'_> {
        ExpandedVersion {
            versions: self,
            index,
        }
    }

    /// Writes one field of one expanded version, leaving the others alone. A field the version
    /// does not have yet is appended, as assigning to a missing key would in PHP.
    pub fn set(&mut self, index: usize, key: &str, val: PhpMixed) {
        let value = Slot::Overridden(self.overrides.len() as u32);
        self.overrides.push(val);

        let key = intern(&mut self.keys, key);
        match self.views[index].iter_mut().find(|entry| entry.key == key) {
            Some(entry) => entry.value = value,
            None => self.views[index].push(Entry { key, value }),
        }
    }

    /// Copies one expanded version out, in the same field order [`MetadataMinifier::expand`]
    /// produces.
    pub fn materialize(&self, index: usize) -> IndexMap<String, PhpMixed> {
        let view = &self.views[index];
        let mut version: IndexMap<String, PhpMixed> = IndexMap::with_capacity(view.len());
        for entry in view {
            version.insert(self.key_of(entry).clone(), self.value_of(entry).clone());
        }
        version
    }

    pub fn into_vec(self) -> Vec<IndexMap<String, PhpMixed>> {
        (0..self.len())
            .map(|index| self.materialize(index))
            .collect()
    }

    fn key_of(&self, entry: &Entry) -> &String {
        self.keys
            .get_index(entry.key as usize)
            .expect("every interned key id is an index into `keys`")
            .0
    }

    fn value_of(&self, entry: &Entry) -> &PhpMixed {
        match entry.value {
            Slot::Minified {
                version_index,
                entry_index,
            } => {
                self.source[version_index as usize]
                    .get_index(entry_index as usize)
                    .expect("expand recorded an entry index that is not in the minified input")
                    .1
            }
            Slot::Overridden(index) => &self.overrides[index as usize],
        }
    }
}

/// One expanded version of an [`ExpandedVersions`], readable without copying it out.
#[derive(Debug, Clone, Copy)]
pub struct ExpandedVersion<'a> {
    versions: &'a ExpandedVersions,
    index: usize,
}

impl<'a> ExpandedVersion<'a> {
    pub fn get(&self, key: &str) -> Option<&'a PhpMixed> {
        let key = *self.versions.keys.get(key)?;
        let entry = self.versions.views[self.index]
            .iter()
            .find(|entry| entry.key == key)?;
        Some(self.versions.value_of(entry))
    }

    pub fn contains_key(&self, key: &str) -> bool {
        let Some(&key) = self.versions.keys.get(key) else {
            return false;
        };
        self.versions.views[self.index]
            .iter()
            .any(|entry| entry.key == key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(fields: &[(&str, &str)]) -> IndexMap<String, PhpMixed> {
        fields
            .iter()
            .map(|(key, val)| (key.to_string(), PhpMixed::String(val.to_string())))
            .collect()
    }

    fn versions() -> Vec<IndexMap<String, PhpMixed>> {
        vec![
            version(&[("name", "foo/bar"), ("version", "3.0"), ("type", "library")]),
            version(&[
                ("version", "2.0"),
                ("type", "__unset"),
                ("abandoned", "yes"),
            ]),
            version(&[("version", "1.0"), ("type", "library")]),
            // every field of the previous version is dropped, so the next one restarts from scratch
            version(&[
                ("name", "__unset"),
                ("version", "__unset"),
                ("abandoned", "__unset"),
                ("type", "__unset"),
            ]),
            version(&[("name", "foo/baz"), ("version", "0.1")]),
        ]
    }

    fn expanded() -> Vec<IndexMap<String, PhpMixed>> {
        vec![
            version(&[("name", "foo/bar"), ("version", "3.0"), ("type", "library")]),
            version(&[
                ("name", "foo/bar"),
                ("version", "2.0"),
                ("abandoned", "yes"),
            ]),
            version(&[
                ("name", "foo/bar"),
                ("version", "1.0"),
                ("abandoned", "yes"),
                ("type", "library"),
            ]),
            IndexMap::new(),
            version(&[("name", "foo/baz"), ("version", "0.1")]),
        ]
    }

    #[test]
    fn expand_applies_each_diff_to_the_previous_version() {
        assert_eq!(MetadataMinifier::expand(versions()).into_vec(), expanded());
    }

    #[test]
    fn a_version_reads_the_same_as_the_materialized_one() {
        let versions = MetadataMinifier::expand(versions());

        for (index, expected) in expanded().iter().enumerate() {
            let version = versions.version(index);
            for key in ["name", "version", "type", "abandoned", "missing"] {
                assert_eq!(version.get(key), expected.get(key), "{index} {key}");
                assert_eq!(
                    version.contains_key(key),
                    expected.contains_key(key),
                    "{index} {key}",
                );
            }
        }
    }

    #[test]
    fn set_overrides_a_field_of_one_version_only() {
        let mut versions = MetadataMinifier::expand(versions());
        versions.set(1, "version", PhpMixed::String("2.0.0.0".to_string()));

        assert_eq!(
            versions.version(1).get("version"),
            Some(&PhpMixed::String("2.0.0.0".to_string())),
        );
        assert_eq!(
            versions.version(2).get("version"),
            Some(&PhpMixed::String("1.0".to_string())),
        );

        let mut expected = expanded();
        expected[1].insert(
            "version".to_string(),
            PhpMixed::String("2.0.0.0".to_string()),
        );
        assert_eq!(versions.into_vec(), expected);
    }

    #[test]
    fn set_appends_a_field_the_version_does_not_have() {
        let mut versions = MetadataMinifier::expand(versions());
        versions.set(
            0,
            "version_normalized",
            PhpMixed::String("3.0.0.0".to_string()),
        );

        let mut expected = expanded();
        expected[0].insert(
            "version_normalized".to_string(),
            PhpMixed::String("3.0.0.0".to_string()),
        );
        assert_eq!(versions.into_vec(), expected);
    }
}
