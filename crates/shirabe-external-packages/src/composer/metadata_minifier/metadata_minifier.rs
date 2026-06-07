use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct MetadataMinifier;

impl MetadataMinifier {
    pub fn expand(versions: Vec<IndexMap<String, PhpMixed>>) -> Vec<IndexMap<String, PhpMixed>> {
        let mut expanded: Vec<IndexMap<String, PhpMixed>> = Vec::new();
        let mut expanded_version: Option<IndexMap<String, PhpMixed>> = None;
        for version_data in versions {
            if expanded_version.as_ref().map_or(true, |ev| ev.is_empty()) {
                expanded.push(version_data.clone());
                expanded_version = Some(version_data);
                continue;
            }

            // add any changes from the previous version to the expanded one
            let ev = expanded_version.as_mut().unwrap();
            for (key, val) in version_data {
                if matches!(&val, PhpMixed::String(s) if s == "__unset") {
                    ev.shift_remove(&key);
                } else {
                    ev.insert(key, val);
                }
            }

            expanded.push(ev.clone());
        }

        expanded
    }

    // MetadataMinifier::minify() is not ported because it is not used in Composer itself.
    // The function is mainly for package repositories.
}
