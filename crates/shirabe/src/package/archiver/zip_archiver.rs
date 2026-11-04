//! ref: composer/src/Composer/Package/Archiver/ZipArchiver.php

use crate::package::archiver::{ArchivableFilesFinder, ArchiverInterface};
use crate::util::{Filesystem, Platform};
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{
    RuntimeException, ZipArchive, bconcat, fileperms, path_to_bytes, realpath, strtr,
};
use std::path::{Path, PathBuf};

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

    fn formats() -> IndexMap<&'static [u8], bool> {
        let mut map: IndexMap<&'static [u8], bool> = IndexMap::new();
        map.insert(b"zip", true);
        map
    }

    fn compression_available(&self) -> bool {
        // PHP: class_exists('ZipArchive'). Shirabe implements ZipArchive natively, so it always
        // exists regardless of whether ext-zip is loaded.
        true
    }
}

impl ArchiverInterface for ZipArchiver {
    fn archive(
        &self,
        sources: &Path,
        target: &Path,
        _format: &BStr,
        excludes: Vec<BString>,
        ignore_filters: bool,
    ) -> anyhow::Result<PathBuf> {
        let fs = Filesystem::new(None);
        let sources = realpath(sources).unwrap_or_else(|| sources.to_path_buf());
        let sources = fs.normalize_path(&sources);

        let mut zip = ZipArchive::new();
        if zip.open(target, ZipArchive::CREATE).is_ok() {
            let files = ArchivableFilesFinder::new(&sources, excludes, ignore_filters)?;
            for file in files {
                let filepath = file;
                let mut relative_path = BString::from(path_to_bytes(
                    filepath
                        .strip_prefix(&sources)
                        .unwrap_or(filepath.as_path()),
                ));

                if Platform::is_windows() {
                    relative_path = strtr(&relative_path, "\\", "/");
                }

                // Ensure to preserve the permission umasks for the filepath in the archive.
                let perms = fileperms(&filepath)?;

                if filepath.is_dir() {
                    zip.add_empty_dir(&relative_path, ZipArchive::OPSYS_UNIX, perms << 16);
                } else {
                    zip.add_file(
                        &filepath,
                        &relative_path,
                        ZipArchive::OPSYS_UNIX,
                        perms << 16,
                    );
                }
            }
            if zip.close() {
                if !target.exists() {
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
                    std::fs::write(target, &eocd)?;
                }

                return Ok(target.to_path_buf());
            }
        }
        let message = bconcat!(
            "Could not create archive '",
            target,
            "' from '",
            sources,
            "': ",
            zip.get_status_string()
        );
        Err(RuntimeException::new(message).into())
    }

    fn supports(&self, format: &BStr, _source_type: Option<&BStr>) -> bool {
        Self::formats().contains_key(format.as_bytes()) && self.compression_available()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
