//! ref: composer/vendor/composer/class-map-generator/src/FileList.php

use bstr::{BString, ByteSlice};
use indexmap::IndexMap;

/// Contains a list of files which were scanned to generate a classmap
#[derive(Debug)]
pub struct FileList {
    pub files: IndexMap<BString, bool>,
}

impl Default for FileList {
    fn default() -> Self {
        Self::new()
    }
}

impl FileList {
    pub fn new() -> Self {
        FileList {
            files: IndexMap::new(),
        }
    }

    pub fn add(&mut self, path: impl Into<BString>) {
        self.files.insert(path.into(), true);
    }

    pub fn contains(&self, path: impl AsRef<[u8]>) -> bool {
        self.files.contains_key(path.as_ref().as_bstr())
    }
}
