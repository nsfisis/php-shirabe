//! ref: composer/src/Composer/Package/Archiver/ArchivableFilesFilter.php sha1:2d1bba20104bf436c2d3edb2315a98d7ad315de8

use shirabe_php_shim::{PharData, bformat, path_to_bytes, str_replace};

pub struct ArchivableFilesFilter {
    inner: Box<dyn Iterator<Item = std::path::PathBuf>>,
    dirs: Vec<std::path::PathBuf>,
}

impl ArchivableFilesFilter {
    pub fn new(inner: Box<dyn Iterator<Item = std::path::PathBuf>>) -> Self {
        Self {
            inner,
            dirs: Vec::new(),
        }
    }

    fn accept(&mut self, file: &std::path::Path) -> bool {
        if file.is_dir() {
            self.dirs.push(file.to_path_buf());
            return false;
        }
        true
    }

    pub fn add_empty_dir(&self, phar: &PharData, sources: &std::path::Path) -> anyhow::Result<()> {
        let prefix = bformat!("{sources}/");
        for filepath in &self.dirs {
            let localname = str_replace(&prefix, "", path_to_bytes(filepath));
            phar.add_empty_dir(&localname)?;
        }
        Ok(())
    }
}

impl Iterator for ArchivableFilesFilter {
    type Item = std::path::PathBuf;

    fn next(&mut self) -> Option<std::path::PathBuf> {
        loop {
            let file = self.inner.next()?;
            if self.accept(&file) {
                return Some(file);
            }
        }
    }
}
