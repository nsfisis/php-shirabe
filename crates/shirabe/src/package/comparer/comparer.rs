//! ref: composer/src/Composer/Package/Comparer/Comparer.php

use crate::util::Platform;
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{bconcat, chdir, hash_file, implode, path_from_bytes, path_to_bytes, trim};
use std::path::PathBuf;

#[derive(Debug)]
pub struct Comparer {
    source: PathBuf,
    update: PathBuf,
    changed: IndexMap<BString, Vec<BString>>,
}

impl Default for Comparer {
    fn default() -> Self {
        Self::new()
    }
}

impl Comparer {
    pub fn new() -> Self {
        Self {
            source: PathBuf::new(),
            update: PathBuf::new(),
            changed: IndexMap::new(),
        }
    }

    pub fn set_source(&mut self, source: PathBuf) {
        self.source = source;
    }

    pub fn set_update(&mut self, update: PathBuf) {
        self.update = update;
    }

    pub fn get_changed(&self, explicated: bool) -> Option<IndexMap<BString, Vec<BString>>> {
        let mut changed = self.changed.clone();
        if changed.is_empty() {
            return None;
        }
        if explicated {
            for (section_key, item_section) in changed.iter_mut() {
                for item in item_section.iter_mut() {
                    *item = bconcat!(&*item, " (", section_key, ")");
                }
            }
        }
        Some(changed)
    }

    pub fn get_changed_as_string(&self, _to_string: bool, explicated: bool) -> BString {
        let changed = match self.get_changed(explicated) {
            None => return BString::default(),
            Some(c) => c,
        };

        let mut strings: Vec<BString> = vec![];
        for item_section in changed.values() {
            for item in item_section {
                strings.push(bconcat!(item, "\r\n"));
            }
        }

        trim(implode("\r\n", &strings))
    }

    pub fn do_compare(&mut self) {
        let mut source: IndexMap<BString, IndexMap<BString, Option<BString>>> = IndexMap::new();
        let mut destination: IndexMap<BString, IndexMap<BString, Option<BString>>> =
            IndexMap::new();
        self.changed = IndexMap::new();
        let current_directory = Platform::get_cwd(false).unwrap_or_default();
        chdir(&self.source);
        if !Self::do_tree(bstr::B(".").as_bstr(), &mut source) {
            return;
        }
        chdir(&current_directory);
        chdir(&self.update);
        if !Self::do_tree(bstr::B(".").as_bstr(), &mut destination) {
            // PHP calls `exit` here, but this is almost certainly a bug. The source branch above
            // handles the same failure with `return`, and there is no reason for the destination
            // branch to terminate the whole process. We treat it as `return`.
            return;
        }
        chdir(&current_directory);
        for (dir, value) in &source {
            for (file, hash) in value {
                let dest_file_hash = destination.get(dir).and_then(|d| d.get(file));
                if let Some(dest_hash) = dest_file_hash {
                    if hash != dest_hash {
                        self.changed
                            .entry(BString::from("changed"))
                            .or_default()
                            .push(bconcat!(dir, "/", file));
                    }
                } else {
                    self.changed
                        .entry(BString::from("removed"))
                        .or_default()
                        .push(bconcat!(dir, "/", file));
                }
            }
        }
        for (dir, value) in &destination {
            for file in value.keys() {
                if !source.get(dir).is_some_and(|d| d.contains_key(file)) {
                    self.changed
                        .entry(BString::from("added"))
                        .or_default()
                        .push(bconcat!(dir, "/", file));
                }
            }
        }
    }

    fn do_tree(
        dir: &BStr,
        array: &mut IndexMap<BString, IndexMap<BString, Option<BString>>>,
    ) -> bool {
        if let Ok(read_dir) = std::fs::read_dir(path_from_bytes(dir)) {
            for entry in read_dir.flatten() {
                let file = BString::from(entry.file_name().as_encoded_bytes());
                if file == "." || file == ".." {
                    continue;
                }
                let path = bconcat!(dir, "/", file);
                if path_from_bytes(&path).is_symlink() {
                    let link_target = std::fs::read_link(path_from_bytes(&path))
                        .ok()
                        .map(|p| BString::from(path_to_bytes(&p)));
                    array
                        .entry(dir.to_owned())
                        .or_default()
                        .insert(file, link_target);
                } else if path_from_bytes(&path).is_dir() {
                    if array.is_empty() {
                        array.insert(BString::from("0"), IndexMap::new());
                    }
                    if !Self::do_tree(path.as_bstr(), array) {
                        return false;
                    }
                } else if path_from_bytes(&path).is_file() {
                    let size = std::fs::metadata(path_from_bytes(&path))
                        .map(|m| m.len())
                        .unwrap_or(0);
                    if size > 0 {
                        let hash = hash_file("xxh3", path_from_bytes(&path));
                        array.entry(dir.to_owned()).or_default().insert(file, hash);
                    }
                }
            }
            if array.len() > 1 && array.contains_key(bstr::B("0").as_bstr()) {
                array.shift_remove(bstr::B("0").as_bstr());
            }
            return true;
        }
        false
    }
}
