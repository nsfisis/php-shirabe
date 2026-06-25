//! ref: composer/vendor/symfony/finder/SplFileInfo.php

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
        shirabe_php_shim::dirname(&self.pathname)
    }

    pub fn get_filename(&self) -> String {
        shirabe_php_shim::basename(&self.pathname)
    }

    pub fn get_basename(&self, suffix: Option<&str>) -> String {
        match suffix {
            Some(suffix) => shirabe_php_shim::basename_with_suffix(&self.pathname, suffix),
            None => shirabe_php_shim::basename(&self.pathname),
        }
    }

    pub fn get_extension(&self) -> String {
        // \SplFileInfo::getExtension() returns the extension (without the dot), or "" if none.
        let base = shirabe_php_shim::basename(&self.pathname);
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
        shirabe_php_shim::is_dir(&self.pathname)
    }

    pub fn is_file(&self) -> bool {
        shirabe_php_shim::is_file(&self.pathname)
    }

    pub fn is_link(&self) -> bool {
        shirabe_php_shim::is_link(&self.pathname)
    }

    pub fn get_real_path(&self) -> Option<String> {
        // \SplFileInfo::getRealPath() returns the canonicalized absolute path, or false on failure.
        shirabe_php_shim::realpath(&self.pathname)
    }

    pub fn get_size(&self) -> i64 {
        // \SplFileInfo::getSize() returns the file size in bytes (throws on failure).
        // TODO(phase-d): PHP throws a \RuntimeException on stat failure; this returns 0 instead.
        shirabe_php_shim::filesize(&self.pathname).unwrap_or(0)
    }
}
