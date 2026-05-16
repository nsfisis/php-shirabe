//! ref: composer/src/Composer/Package/Archiver/ZipArchiver.php

use indexmap::IndexMap;
use shirabe_php_shim::{class_exists, fileperms, method_exists, pack, realpath, PhpMixed, RuntimeException, ZipArchive};
use crate::package::archiver::archivable_files_finder::ArchivableFilesFinder;
use crate::package::archiver::archiver_interface::ArchiverInterface;
use crate::util::filesystem::Filesystem;
use crate::util::platform::Platform;

#[derive(Debug)]
pub struct ZipArchiver;

impl ZipArchiver {
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
        let fs = Filesystem::new();
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
                let filepath = file.get_pathname();
                let mut relative_path = file.get_relative_pathname();

                if Platform::is_windows() {
                    relative_path = shirabe_php_shim::strtr(&relative_path, "\\", "/");
                }

                if file.is_dir() {
                    zip.add_empty_dir(&relative_path);
                } else {
                    zip.add_file(&filepath, &relative_path);
                }

                // setExternalAttributesName() is only available with libzip 0.11.2 or above
                if method_exists(&PhpMixed::Null, "setExternalAttributesName") {
                    let perms = fileperms(&filepath);
                    zip.set_external_attributes_name(&relative_path, ZipArchive::OPSYS_UNIX, perms << 16);
                }
            }
            if zip.close() {
                if !std::path::Path::new(&target).exists() {
                    // create minimal valid ZIP file (Empty Central Directory + End of Central Directory record)
                    let eocd = pack("VvvvvVVv", &[
                        PhpMixed::Int(0x06054b50), // End of central directory signature
                        PhpMixed::Int(0),          // Number of this disk
                        PhpMixed::Int(0),          // Disk where central directory starts
                        PhpMixed::Int(0),          // Number of central directory records on this disk
                        PhpMixed::Int(0),          // Total number of central directory records
                        PhpMixed::Int(0),          // Size of central directory (bytes)
                        PhpMixed::Int(0),          // Offset of start of central directory
                        PhpMixed::Int(0),          // Comment length
                    ]);
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
        Err(RuntimeException { message, code: 0 }.into())
    }

    fn supports(&self, format: String, _source_type: Option<String>) -> bool {
        Self::formats().contains_key(&format) && self.compression_available()
    }
}
