//! ref: composer/src/Composer/Package/Comparer/Comparer.php

use indexmap::IndexMap;
use std::path::Path;

use crate::util::platform::Platform;

#[derive(Debug)]
pub struct Comparer {
    source: String,
    update: String,
    changed: IndexMap<String, Vec<String>>,
}

impl Comparer {
    pub fn new() -> Self {
        Self {
            source: String::new(),
            update: String::new(),
            changed: IndexMap::new(),
        }
    }

    pub fn set_source(&mut self, source: String) {
        self.source = source;
    }

    pub fn set_update(&mut self, update: String) {
        self.update = update;
    }

    pub fn get_changed(&self, explicated: bool) -> Option<IndexMap<String, Vec<String>>> {
        let mut changed = self.changed.clone();
        if changed.is_empty() {
            return None;
        }
        if explicated {
            for (section_key, item_section) in changed.iter_mut() {
                let key = section_key.clone();
                for item in item_section.iter_mut() {
                    *item = format!("{} ({})", item, key);
                }
            }
        }
        Some(changed)
    }

    pub fn get_changed_as_string(&self, _to_string: bool, explicated: bool) -> String {
        let changed = match self.get_changed(explicated) {
            None => return String::new(),
            Some(c) => c,
        };

        let mut strings: Vec<String> = vec![];
        for (_section_key, item_section) in &changed {
            for item in item_section {
                strings.push(format!("{}\r\n", item));
            }
        }

        strings.join("\r\n").trim().to_string()
    }

    pub fn do_compare(&mut self) {
        let mut source: IndexMap<String, IndexMap<String, Option<String>>> = IndexMap::new();
        let mut destination: IndexMap<String, IndexMap<String, Option<String>>> = IndexMap::new();
        self.changed = IndexMap::new();
        let current_directory = Platform::get_cwd();
        shirabe_php_shim::chdir(&self.source);
        if !Self::do_tree(".", &mut source) {
            return;
        }
        shirabe_php_shim::chdir(&current_directory);
        shirabe_php_shim::chdir(&self.update);
        if !Self::do_tree(".", &mut destination) {
            std::process::exit(0);
        }
        shirabe_php_shim::chdir(&current_directory);
        for (dir, value) in &source {
            for (file, hash) in value {
                let dest_file_hash = destination.get(dir).and_then(|d| d.get(file));
                if let Some(dest_hash) = dest_file_hash {
                    if hash != dest_hash {
                        self.changed.entry("changed".to_string()).or_default().push(format!("{}/{}", dir, file));
                    }
                } else {
                    self.changed.entry("removed".to_string()).or_default().push(format!("{}/{}", dir, file));
                }
            }
        }
        for (dir, value) in &destination {
            for (file, _hash) in value {
                if !source.get(dir).map_or(false, |d| d.contains_key(file)) {
                    self.changed.entry("added".to_string()).or_default().push(format!("{}/{}", dir, file));
                }
            }
        }
    }

    fn do_tree(dir: &str, array: &mut IndexMap<String, IndexMap<String, Option<String>>>) -> bool {
        if let Ok(read_dir) = std::fs::read_dir(dir) {
            for entry in read_dir.flatten() {
                let file: String = entry.file_name().to_string_lossy().into_owned();
                if file == "." || file == ".." {
                    continue;
                }
                let path = format!("{}/{}", dir, file);
                if Path::new(&path).is_symlink() {
                    let link_target = std::fs::read_link(&path).ok().and_then(|p| p.to_str().map(|s| s.to_string()));
                    array.entry(dir.to_string()).or_default().insert(file, link_target);
                } else if Path::new(&path).is_dir() {
                    if array.is_empty() {
                        array.insert("0".to_string(), IndexMap::new());
                    }
                    if !Self::do_tree(&path, array) {
                        return false;
                    }
                } else if Path::new(&path).is_file() {
                    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                    if size > 0 {
                        let algo = if shirabe_php_shim::PHP_VERSION_ID > 80100 { "xxh3" } else { "sha1" };
                        let hash = shirabe_php_shim::hash_file(algo, &path);
                        array.entry(dir.to_string()).or_default().insert(file, hash);
                    }
                }
            }
            if array.len() > 1 && array.contains_key("0") {
                array.shift_remove("0");
            }
            return true;
        }
        false
    }
}
