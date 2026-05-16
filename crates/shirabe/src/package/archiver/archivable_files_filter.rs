//! ref: composer/src/Composer/Package/Archiver/ArchivableFilesFilter.php

use shirabe_php_shim::PharData;
use std::path::PathBuf;

pub struct ArchivableFilesFilter {
    inner: Box<dyn Iterator<Item = PathBuf>>,
    dirs: Vec<String>,
}

impl ArchivableFilesFilter {
    pub fn new(inner: Box<dyn Iterator<Item = PathBuf>>) -> Self {
        Self {
            inner,
            dirs: Vec::new(),
        }
    }

    fn accept(&mut self, file: &PathBuf) -> bool {
        if file.is_dir() {
            self.dirs.push(file.to_string_lossy().into_owned());
            return false;
        }
        true
    }

    pub fn add_empty_dir(&self, phar: &PharData, sources: &str) {
        for filepath in &self.dirs {
            let localname = filepath.replace(&format!("{}/", sources), "");
            phar.add_empty_dir(&localname);
        }
    }
}

impl Iterator for ArchivableFilesFilter {
    type Item = PathBuf;

    fn next(&mut self) -> Option<PathBuf> {
        loop {
            let file = self.inner.next()?;
            if self.accept(&file) {
                return Some(file);
            }
        }
    }
}
