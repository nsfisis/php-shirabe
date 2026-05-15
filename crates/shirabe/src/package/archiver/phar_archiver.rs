//! ref: composer/src/Composer/Package/Archiver/PharArchiver.php

use indexmap::IndexMap;
use shirabe_php_shim::{
    bzcompress, file_exists, file_put_contents, function_exists, gzcompress, pack, str_repeat,
    strrpos, unlink, FilesystemIterator, Phar, PharData, PhpMixed, RuntimeException,
};

use crate::package::archiver::archivable_files_filter::ArchivableFilesFilter;
use crate::package::archiver::archivable_files_finder::ArchivableFilesFinder;
use crate::package::archiver::archiver_interface::ArchiverInterface;

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
            );
            let files = ArchivableFilesFinder::new(&sources, excludes, ignore_filters)?;
            let mut files_only = ArchivableFilesFilter::new(files);
            phar.build_from_iterator(&mut files_only, &sources);
            files_only.add_empty_dir(&phar, &sources);

            if !file_exists(&target) {
                let target = format!("{}.{}", filename, format);
                drop(phar);

                if format == "tar" {
                    // create an empty tar file (=10240 null bytes) if the tar file is empty and PharData thus did not write it to disk
                    file_put_contents(&target, &str_repeat("\0", 10240).into_bytes());
                } else if format == "zip" {
                    // create minimal valid ZIP file (Empty Central Directory + End of Central Directory record)
                    let eocd = pack(
                        "VvvvvVVv",
                        &[
                            PhpMixed::Int(0x06054b50),  // End of central directory signature
                            PhpMixed::Int(0),           // Number of this disk
                            PhpMixed::Int(0),           // Disk where central directory starts
                            PhpMixed::Int(0),           // Number of central directory records on this disk
                            PhpMixed::Int(0),           // Total number of central directory records
                            PhpMixed::Int(0),           // Size of central directory (bytes)
                            PhpMixed::Int(0),           // Offset of start of central directory
                            PhpMixed::Int(0),           // Comment length
                        ],
                    );
                    file_put_contents(&target, &eocd);
                } else if format == "tar.gz" || format == "tar.bz2" {
                    let compress_algo = *compress_formats.get(format.as_str()).unwrap();
                    if !PharData::can_compress(compress_algo) {
                        return Err(RuntimeException {
                            message: format!("Can not compress to {} format", format),
                            code: 0,
                        }
                        .into());
                    }
                    if format == "tar.gz" && function_exists("gzcompress") {
                        let data = gzcompress(&str_repeat("\0", 10240).into_bytes()).unwrap_or_default();
                        file_put_contents(&target, &data);
                    } else if format == "tar.bz2" && function_exists("bzcompress") {
                        let data = bzcompress(&str_repeat("\0", 10240).into_bytes()).unwrap_or_default();
                        file_put_contents(&target, &data);
                    }
                }

                return Ok(target);
            }

            if compress_formats.contains_key(format.as_str()) {
                let compress_algo = *compress_formats.get(format.as_str()).unwrap();
                if !PharData::can_compress(compress_algo) {
                    return Err(RuntimeException {
                        message: format!("Can not compress to {} format", format),
                        code: 0,
                    }
                    .into());
                }

                unlink(&target);

                phar.compress(compress_algo);

                let target = format!("{}.{}", filename, format);
                return Ok(target);
            }

            Ok(target)
        })();

        inner.map_err(|e| {
            let message = format!(
                "Could not create archive '{}' from '{}': {}",
                target, sources, e
            );
            anyhow::anyhow!(RuntimeException { message, code: 0 })
        })
    }

    fn supports(&self, format: String, _source_type: Option<String>) -> bool {
        formats().contains_key(format.as_str())
    }
}
