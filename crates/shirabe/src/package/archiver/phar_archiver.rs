//! ref: composer/src/Composer/Package/Archiver/PharArchiver.php

use crate::package::archiver::ArchivableFilesFilter;
use crate::package::archiver::ArchivableFilesFinder;
use crate::package::archiver::ArchiverInterface;
use indexmap::IndexMap;
use shirabe_php_shim::{
    FilesystemIterator, Phar, PharData, RuntimeException, bzcompress, file_exists,
    file_put_contents, function_exists, gzcompress, str_repeat, strrpos, unlink,
};

fn formats() -> IndexMap<&'static str, i64> {
    let mut m = IndexMap::new();
    m.insert("zip", Phar::ZIP);
    m.insert("tar", Phar::TAR);
    m.insert("tar.gz", Phar::TAR);
    m.insert("tar.bz2", Phar::TAR);
    m
}

fn compress_formats() -> IndexMap<&'static str, i64> {
    let mut m = IndexMap::new();
    m.insert("tar.gz", Phar::GZ);
    m.insert("tar.bz2", Phar::BZ2);
    m
}

#[derive(Debug)]
pub struct PharArchiver;

impl Default for PharArchiver {
    fn default() -> Self {
        Self::new()
    }
}

impl PharArchiver {
    pub fn new() -> Self {
        Self
    }
}

impl ArchiverInterface for PharArchiver {
    fn archive(
        &self,
        sources: String,
        target: String,
        format: String,
        excludes: Vec<String>,
        ignore_filters: bool,
    ) -> anyhow::Result<String> {
        let sources = shirabe_php_shim::realpath(&sources).unwrap_or(sources);
        let formats = formats();
        let compress_formats = compress_formats();

        if file_exists(&target) {
            unlink(&target);
        }

        let target_outer = target.clone();
        let inner = (|| -> anyhow::Result<String> {
            let pos = strrpos(&target, &format).unwrap_or(target.len());
            let filename = target[..pos.saturating_sub(1)].to_string();

            let target = if compress_formats.contains_key(format.as_str()) {
                format!("{}.tar", filename)
            } else {
                target
            };

            let phar = PharData::new_with_format(
                target.clone(),
                FilesystemIterator::KEY_AS_PATHNAME | FilesystemIterator::CURRENT_AS_FILEINFO,
                "",
                *formats.get(format.as_str()).unwrap_or(&Phar::TAR),
            )?;
            let files = ArchivableFilesFinder::new(&sources, excludes, ignore_filters)?;
            let mut files_only = ArchivableFilesFilter::new(Box::new(files));
            phar.build_from_iterator(&mut files_only, &sources)?;
            files_only.add_empty_dir(&phar, &sources)?;

            if !file_exists(&target) {
                let target = format!("{}.{}", filename, format);
                drop(phar);

                if format == "tar" {
                    // create an empty tar file (=10240 null bytes) if the tar file is empty and PharData thus did not write it to disk
                    file_put_contents(&target, &str_repeat("\0", 10240).into_bytes());
                } else if format == "zip" {
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
                    file_put_contents(&target, &eocd);
                } else if format == "tar.gz" || format == "tar.bz2" {
                    let compress_algo = *compress_formats.get(format.as_str()).unwrap();
                    if !PharData::can_compress(compress_algo) {
                        return Err(RuntimeException::new(format!(
                            "Can not compress to {} format",
                            format
                        ))
                        .into());
                    }
                    if format == "tar.gz" && function_exists("gzcompress") {
                        let data =
                            gzcompress(&str_repeat("\0", 10240).into_bytes()).unwrap_or_default();
                        file_put_contents(&target, &data);
                    } else if format == "tar.bz2" && function_exists("bzcompress") {
                        let data =
                            bzcompress(&str_repeat("\0", 10240).into_bytes()).unwrap_or_default();
                        file_put_contents(&target, &data);
                    }
                }

                return Ok(target);
            }

            if compress_formats.contains_key(format.as_str()) {
                let compress_algo = *compress_formats.get(format.as_str()).unwrap();
                if !PharData::can_compress(compress_algo) {
                    return Err(RuntimeException::new(format!(
                        "Can not compress to {} format",
                        format
                    ))
                    .into());
                }

                unlink(&target);

                phar.compress(compress_algo)?;

                let target = format!("{}.{}", filename, format);
                return Ok(target);
            }

            Ok(target)
        })();

        inner.map_err(|e| {
            let message = format!(
                "Could not create archive '{}' from '{}': {}",
                target_outer, sources, e
            );
            RuntimeException::new(message).into()
        })
    }

    fn supports(&self, format: String, _source_type: Option<String>) -> bool {
        formats().contains_key(format.as_str())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
