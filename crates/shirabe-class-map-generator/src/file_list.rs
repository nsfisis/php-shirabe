//! ref: composer/vendor/composer/class-map-generator/src/FileList.php

use indexmap::IndexMap;

/// Contains a list of files which were scanned to generate a classmap
#[derive(Debug)]
pub struct FileList {
    pub files: IndexMap<String, bool>,
}

impl FileList {
    pub fn add(&mut self, path: String) {
        self.files.insert(path, true);
    }

    pub fn contains(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }
}
