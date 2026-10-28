//! ref: composer/vendor/symfony/finder/SplFileInfo.php

use shirabe_php_shim::{
    RuntimeException, basename, basename_with_suffix, dirname, filesize, is_dir, is_file, is_link,
    realpath,
};

#[derive(Debug)]
pub struct SplFileInfo {
    // The path passed to the underlying \SplFileInfo constructor.
    pathname: String,
    relative_path: String,
    relative_pathname: String,
}

impl std::fmt::Display for SplFileInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.get_pathname())
    }
}

impl SplFileInfo {
    pub fn new(file: &str, relative_path: &str, relative_pathname: &str) -> Self {
        Self {
            pathname: file.to_string(),
            relative_path: relative_path.to_string(),
            relative_pathname: relative_pathname.to_string(),
        }
    }

    pub fn get_pathname(&self) -> String {
        self.pathname.clone()
    }

    pub fn get_path(&self) -> String {
        dirname(&self.pathname)
    }

    pub fn get_filename(&self) -> String {
        basename(&self.pathname)
    }

    pub fn get_basename(&self, suffix: Option<&str>) -> String {
        match suffix {
            Some(suffix) => basename_with_suffix(&self.pathname, suffix),
            None => basename(&self.pathname),
        }
    }

    pub fn get_extension(&self) -> String {
        // \SplFileInfo::getExtension() returns the extension (without the dot), or "" if none.
        let base = basename(&self.pathname);
        match base.rfind('.') {
            Some(index) => base[index + 1..].to_string(),
            None => String::new(),
        }
    }

    pub fn get_relative_path_name(&self) -> String {
        self.relative_pathname.clone()
    }

    pub fn get_relative_path(&self) -> String {
        self.relative_path.clone()
    }

    pub fn is_dir(&self) -> bool {
        is_dir(&self.pathname)
    }

    pub fn is_file(&self) -> bool {
        is_file(&self.pathname)
    }

    pub fn is_link(&self) -> bool {
        is_link(&self.pathname)
    }

    pub fn get_real_path(&self) -> Option<String> {
        // \SplFileInfo::getRealPath() returns the canonicalized absolute path, or false on failure.
        realpath(&self.pathname)
    }

    pub fn get_size(&self) -> anyhow::Result<i64> {
        // Inherited from \SplFileInfo, whose failure message names the base class even for subclasses.
        match filesize(&self.pathname) {
            Some(size) => Ok(size),
            None => Err(RuntimeException::new(format!(
                "SplFileInfo::getSize(): stat failed for {}",
                self.pathname
            ))
            .into()),
        }
    }
}
