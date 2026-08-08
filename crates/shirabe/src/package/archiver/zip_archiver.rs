//! ref: composer/src/Composer/Package/Archiver/ZipArchiver.php

use crate::package::archiver::ArchivableFilesFinder;
use crate::package::archiver::ArchiverInterface;
use crate::util::Filesystem;
use crate::util::Platform;
use indexmap::IndexMap;
use shirabe_php_shim::{RuntimeException, ZipArchive, class_exists, fileperms, realpath};
use std::path::PathBuf;

#[derive(Debug)]
pub struct ZipArchiver;

impl Default for ZipArchiver {
    fn default() -> Self {
        Self::new()
    }
}

impl ZipArchiver {
    pub fn new() -> Self {
        Self
    }

    fn formats() -> IndexMap<String, bool> {
        let mut map = IndexMap::new();
        map.insert("zip".to_string(), true);
        map
    }

    fn compression_available(&self) -> bool {
        class_exists("ZipArchive")
    }
}

impl ArchiverInterface for ZipArchiver {
    fn archive(
        &self,
        sources: String,
        target: String,
        format: String,
        excludes: Vec<String>,
        ignore_filters: bool,
    ) -> anyhow::Result<String> {
        let fs = Filesystem::new(None);
        let sources_realpath = realpath(&sources);
        let sources = if let Some(p) = sources_realpath {
            p
        } else {
            sources
        };
        let sources = fs.normalize_path(&sources);

        let mut zip = ZipArchive::new();
        if zip.open(&target, ZipArchive::CREATE).is_ok() {
            let files = ArchivableFilesFinder::new(&sources, excludes, ignore_filters)?;
            for file in files {
                let filepath = file;
                let mut relative_path = filepath
                    .strip_prefix(&sources)
                    .unwrap_or(filepath.as_path())
                    .to_path_buf();

                if Platform::is_windows() {
                    relative_path = PathBuf::from(shirabe_php_shim::strtr(
                        &relative_path.to_string_lossy(),
                        "\\",
                        "/",
                    ));
                }

                // Ensure to preserve the permission umasks for the filepath in the archive.
                let perms = fileperms(&filepath);

                if filepath.is_dir() {
                    zip.add_empty_dir(
                        &relative_path.to_string_lossy(),
                        ZipArchive::OPSYS_UNIX,
                        perms << 16,
                    );
                } else {
                    zip.add_file(
                        &filepath,
                        &relative_path.to_string_lossy(),
                        ZipArchive::OPSYS_UNIX,
                        perms << 16,
                    );
                }
            }
            if zip.close() {
                if !std::path::Path::new(&target).exists() {
                    // create minimal valid ZIP file (Empty Central Directory + End of Central Directory record)
                    let mut eocd = Vec::with_capacity(22);
                    eocd.extend_from_slice(&0x06054b50u32.to_le_bytes()); // End of central directory signature
                    eocd.extend_from_slice(&0u16.to_le_bytes()); // Number of this disk
                    eocd.extend_from_slice(&0u16.to_le_bytes()); // Disk where central directory starts
                    eocd.extend_from_slice(&0u16.to_le_bytes()); // Number of central directory records on this disk
                    eocd.extend_from_slice(&0u16.to_le_bytes()); // Total number of central directory records
                    eocd.extend_from_slice(&0u32.to_le_bytes()); // Size of central directory (bytes)
                    eocd.extend_from_slice(&0u32.to_le_bytes()); // Offset of start of central directory
                    eocd.extend_from_slice(&0u16.to_le_bytes()); // Comment length
                    std::fs::write(&target, &eocd)?;
                }

                return Ok(target);
            }
        }
        let message = format!(
            "Could not create archive '{}' from '{}': {}",
            target,
            sources,
            zip.get_status_string()
        );
        Err(RuntimeException::new(message).into())
    }

    fn supports(&self, format: String, _source_type: Option<String>) -> bool {
        Self::formats().contains_key(&format) && self.compression_available()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
