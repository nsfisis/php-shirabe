//! ref: composer/vendor/symfony/finder/SplFileInfo.php

use bstr::{BStr, BString, ByteSlice};
use shirabe_php_shim::{
    RuntimeException, basename, basename_with_suffix, bconcat, dirname, filesize, is_dir, is_file,
    is_link, path_from_bytes, realpath,
};
use std::path::PathBuf;

#[derive(Debug)]
pub struct SplFileInfo {
    // The path passed to the underlying \SplFileInfo constructor.
    pathname: BString,
    relative_path: BString,
    relative_pathname: BString,
}

impl SplFileInfo {
    pub fn new(
        file: impl AsRef<[u8]>,
        relative_path: impl AsRef<[u8]>,
        relative_pathname: impl AsRef<[u8]>,
    ) -> Self {
        Self {
            pathname: BString::from(file.as_ref()),
            relative_path: BString::from(relative_path.as_ref()),
            relative_pathname: BString::from(relative_pathname.as_ref()),
        }
    }

    pub fn get_pathname(&self) -> &BStr {
        self.pathname.as_ref()
    }

    pub fn get_path(&self) -> BString {
        dirname(&self.pathname)
    }

    pub fn get_filename(&self) -> BString {
        basename(&self.pathname)
    }

    pub fn get_basename(&self, suffix: Option<&BStr>) -> BString {
        match suffix {
            Some(suffix) => basename_with_suffix(&self.pathname, suffix),
            None => basename(&self.pathname),
        }
    }

    pub fn get_extension(&self) -> BString {
        // \SplFileInfo::getExtension() returns the extension (without the dot), or "" if none.
        let base = basename(&self.pathname);
        match base.rfind_byte(b'.') {
            Some(index) => base[index + 1..].into(),
            None => BString::default(),
        }
    }

    pub fn get_relative_path_name(&self) -> &BStr {
        self.relative_pathname.as_ref()
    }

    pub fn get_relative_path(&self) -> &BStr {
        self.relative_path.as_ref()
    }

    pub fn is_dir(&self) -> bool {
        is_dir(path_from_bytes(&self.pathname))
    }

    pub fn is_file(&self) -> bool {
        is_file(path_from_bytes(&self.pathname))
    }

    pub fn is_link(&self) -> bool {
        is_link(path_from_bytes(&self.pathname))
    }

    pub fn get_real_path(&self) -> Option<PathBuf> {
        // \SplFileInfo::getRealPath() returns the canonicalized absolute path, or false on failure.
        realpath(path_from_bytes(&self.pathname))
    }

    pub fn get_size(&self) -> anyhow::Result<i64> {
        // Inherited from \SplFileInfo, whose failure message names the base class even for subclasses.
        match filesize(path_from_bytes(&self.pathname)) {
            Some(size) => Ok(size),
            None => Err(RuntimeException::new(bconcat!(
                "SplFileInfo::getSize(): stat failed for ",
                self.pathname
            ))
            .into()),
        }
    }
}
