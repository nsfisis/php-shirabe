//! ref: composer/src/Composer/Package/Archiver/PharArchiver.php

use crate::package::archiver::{ArchivableFilesFilter, ArchivableFilesFinder, ArchiverInterface};
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{
    Catch as _, FilesystemIterator, Phar, PharData, RuntimeException, UnexpectedValueException,
    bconcat, bzcompress, file_exists, file_put_contents, gzcompress, path_from_bytes,
    path_to_bytes, realpath, str_repeat, strrpos, unlink,
};
use std::path::{Path, PathBuf};

fn formats() -> IndexMap<&'static [u8], i64> {
    let mut m: IndexMap<&'static [u8], i64> = IndexMap::new();
    m.insert(b"zip", Phar::ZIP);
    m.insert(b"tar", Phar::TAR);
    m.insert(b"tar.gz", Phar::TAR);
    m.insert(b"tar.bz2", Phar::TAR);
    m
}

fn compress_formats() -> IndexMap<&'static [u8], i64> {
    let mut m: IndexMap<&'static [u8], i64> = IndexMap::new();
    m.insert(b"tar.gz", Phar::GZ);
    m.insert(b"tar.bz2", Phar::BZ2);
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
        sources: &Path,
        target: &Path,
        format: &BStr,
        excludes: Vec<BString>,
        ignore_filters: bool,
    ) -> anyhow::Result<PathBuf> {
        let sources = realpath(sources).unwrap_or_else(|| sources.to_path_buf());
        let formats = formats();
        let compress_formats = compress_formats();

        if file_exists(target) {
            unlink(target);
        }

        let target_outer = target;
        let inner = (|| -> anyhow::Result<PathBuf> {
            let target_bytes = path_to_bytes(target);
            let pos = strrpos(target_bytes, format).unwrap_or(target_bytes.len());
            let filename = BString::from(&target_bytes[..pos.saturating_sub(1)]);

            let target = if compress_formats.contains_key(format.as_bytes()) {
                path_from_bytes(&bconcat!(&filename, ".tar")).to_path_buf()
            } else {
                target.to_path_buf()
            };

            let phar = PharData::new_with_format(
                &target,
                FilesystemIterator::KEY_AS_PATHNAME | FilesystemIterator::CURRENT_AS_FILEINFO,
                "",
                *formats.get(format.as_bytes()).unwrap_or(&Phar::TAR),
            )?;
            let files = ArchivableFilesFinder::new(&sources, excludes, ignore_filters)?;
            let mut files_only = ArchivableFilesFilter::new(Box::new(files));
            phar.build_from_iterator(&mut files_only, &sources)?;
            files_only.add_empty_dir(&phar, &sources)?;

            if !file_exists(&target) {
                let target = path_from_bytes(&bconcat!(&filename, ".", format)).to_path_buf();
                drop(phar);

                if format == "tar" {
                    // create an empty tar file (=10240 null bytes) if the tar file is empty and PharData thus did not write it to disk
                    file_put_contents(&target, str_repeat("\0", 10240));
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
                    let compress_algo = *compress_formats.get(format.as_bytes()).unwrap();
                    if !PharData::can_compress(compress_algo) {
                        return Err(RuntimeException::new(bconcat!(
                            "Can not compress to ",
                            format,
                            " format"
                        ))
                        .into());
                    }
                    if format == "tar.gz" {
                        let data = gzcompress(&str_repeat("\0", 10240)).unwrap_or_default();
                        file_put_contents(&target, &data);
                    } else if format == "tar.bz2" {
                        let data = bzcompress(&str_repeat("\0", 10240)).unwrap_or_default();
                        file_put_contents(&target, &data);
                    }
                }

                return Ok(target);
            }

            if compress_formats.contains_key(format.as_bytes()) {
                let compress_algo = *compress_formats.get(format.as_bytes()).unwrap();
                if !PharData::can_compress(compress_algo) {
                    return Err(RuntimeException::new(bconcat!(
                        "Can not compress to ",
                        format,
                        " format"
                    ))
                    .into());
                }

                unlink(&target);

                phar.compress(compress_algo)?;

                let target = path_from_bytes(&bconcat!(&filename, ".", format)).to_path_buf();
                return Ok(target);
            }

            Ok(target)
        })();

        inner.map_err(|e| {
            let Some(caught) = e.catch::<UnexpectedValueException>() else {
                return e;
            };
            let message = bconcat!(
                "Could not create archive '",
                target_outer,
                "' from '",
                sources,
                "': ",
                caught.get_message()
            );
            let code = caught.get_code();
            RuntimeException::with_code_and_previous(message, code, Some(std::sync::Arc::new(e)))
                .into()
        })
    }

    fn supports(&self, format: &BStr, _source_type: Option<&BStr>) -> bool {
        formats().contains_key(format.as_bytes())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
