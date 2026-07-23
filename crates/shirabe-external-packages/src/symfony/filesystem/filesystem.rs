//! ref: composer/vendor/symfony/filesystem/Filesystem.php

use crate::symfony::filesystem::exception::io_exception::IOException;
use shirabe_php_shim::PhpMixed;

#[derive(Debug, Clone)]
pub struct Filesystem;

impl Default for Filesystem {
    fn default() -> Self {
        Self::new()
    }
}

impl Filesystem {
    pub fn new() -> Self {
        Filesystem
    }

    // Symfony's toIterable(): a single string yields one element, an array/iterable yields each
    // element as a string.
    fn to_iterable(files: &PhpMixed) -> Vec<String> {
        match files {
            PhpMixed::String(s) => vec![s.clone()],
            PhpMixed::List(items) => items
                .iter()
                .map(|item| item.as_string().unwrap_or("").to_string())
                .collect(),
            PhpMixed::Array(entries) => entries
                .values()
                .map(|item| item.as_string().unwrap_or("").to_string())
                .collect(),
            _ => vec![files.as_string().unwrap_or("").to_string()],
        }
    }

    fn copy(
        &self,
        origin_file: &str,
        target_file: &str,
        override_file: bool,
    ) -> anyhow::Result<()> {
        // PHP: stream_is_local($originFile) || 0 === stripos($originFile, 'file://')
        let origin_is_local = Self::stream_is_local(origin_file)
            || shirabe_php_shim::stripos(origin_file, "file://") == Some(0);

        if origin_is_local && !shirabe_php_shim::is_file(origin_file) {
            return Err(IOException::new(
                format!(
                    "Failed to copy \"{}\" because file does not exist.",
                    origin_file
                ),
                0,
                None,
                Some(origin_file.to_string()),
            )
            .into());
        }

        self.mkdir(
            PhpMixed::String(shirabe_php_shim::dirname(target_file)),
            0o777,
        )?;

        let mut do_copy = true;
        // PHP: !$overwriteNewerFiles && !parse_url($originFile, PHP_URL_HOST) && is_file($targetFile)
        let origin_host = shirabe_php_shim::parse_url(origin_file, shirabe_php_shim::PHP_URL_HOST);
        if !override_file
            && matches!(origin_host, PhpMixed::Null | PhpMixed::Bool(false))
            && shirabe_php_shim::is_file(target_file)
        {
            do_copy = shirabe_php_shim::filemtime(origin_file).unwrap_or(0)
                > shirabe_php_shim::filemtime(target_file).unwrap_or(0);
        }

        if do_copy {
            let bytes_copied = match shirabe_php_shim::copy(origin_file, target_file) {
                true => shirabe_php_shim::filesize(target_file).unwrap_or(0),
                false => {
                    return Err(IOException::new(
                        format!("Failed to copy \"{}\" to \"{}\".", origin_file, target_file),
                        0,
                        None,
                        Some(origin_file.to_string()),
                    )
                    .into());
                }
            };

            if !shirabe_php_shim::is_file(target_file) {
                return Err(IOException::new(
                    format!("Failed to copy \"{}\" to \"{}\".", origin_file, target_file),
                    0,
                    None,
                    Some(origin_file.to_string()),
                )
                .into());
            }

            if origin_is_local {
                // Like `cp`, preserve executable permission bits.
                shirabe_php_shim::chmod(
                    target_file,
                    (shirabe_php_shim::fileperms(target_file)
                        | (shirabe_php_shim::fileperms(origin_file) & 0o111))
                        as u32,
                );

                // Like `cp`, preserve the file modification time. The shim's touch2/touch3 (explicit
                // mtime) are unimplemented (no utimensat), so mtime preservation is omitted here.

                let bytes_origin = shirabe_php_shim::filesize(origin_file).unwrap_or(0);
                if bytes_copied != bytes_origin {
                    return Err(IOException::new(
                        format!(
                            "Failed to copy the whole content of \"{}\" to \"{}\" ({} of {} bytes copied).",
                            origin_file, target_file, bytes_copied, bytes_origin
                        ),
                        0,
                        None,
                        Some(origin_file.to_string()),
                    )
                    .into());
                }
            }
        }

        Ok(())
    }

    fn mkdir(&self, dirs: PhpMixed, mode: u32) -> anyhow::Result<()> {
        for dir in Self::to_iterable(&dirs) {
            if shirabe_php_shim::is_dir(&dir) {
                continue;
            }

            if !shirabe_php_shim::mkdir(&dir, mode, true) && !shirabe_php_shim::is_dir(&dir) {
                return Err(IOException::new(
                    format!("Failed to create \"{}\": ", dir),
                    0,
                    None,
                    Some(dir.clone()),
                )
                .into());
            }
        }
        Ok(())
    }

    fn exists(&self, files: PhpMixed) -> bool {
        for file in Self::to_iterable(&files) {
            if !shirabe_php_shim::file_exists(&file) {
                return false;
            }
        }
        true
    }

    fn remove(&self, files: PhpMixed) -> anyhow::Result<()> {
        let files = Self::to_iterable(&files);
        Self::do_remove(files, false)
    }

    fn do_remove(files: Vec<String>, is_recursive: bool) -> anyhow::Result<()> {
        // PHP reverses the list so that directory contents are removed before the directory itself.
        let mut files = files;
        files.reverse();
        for file in files {
            if shirabe_php_shim::is_link(&file) {
                // See https://bugs.php.net/52176
                if !shirabe_php_shim::unlink(&file) && shirabe_php_shim::file_exists(&file) {
                    return Err(IOException::new(
                        format!("Failed to remove symlink \"{}\": ", file),
                        0,
                        None,
                        None,
                    )
                    .into());
                }
            } else if shirabe_php_shim::is_dir(&file) {
                let entries = match shirabe_php_shim::recursive_directory_iterator(
                    &file,
                    shirabe_php_shim::FilesystemIterator::KEY_AS_PATHNAME
                        | shirabe_php_shim::SKIP_DOTS,
                ) {
                    Ok(dir) => shirabe_php_shim::recursive_iterator_iterator(
                        dir,
                        shirabe_php_shim::RecursiveIteratorIterator::CHILD_FIRST,
                    ),
                    Err(_) => {
                        return Err(IOException::new(
                            format!("Failed to remove directory \"{}\": ", file),
                            0,
                            None,
                            None,
                        )
                        .into());
                    }
                };
                let child_paths: Vec<String> =
                    (&entries).into_iter().map(|e| e.get_pathname()).collect();
                Self::do_remove(child_paths, true)?;

                if !shirabe_php_shim::rmdir(&file) && shirabe_php_shim::file_exists(&file) {
                    return Err(IOException::new(
                        format!("Failed to remove directory \"{}\": ", file),
                        0,
                        None,
                        None,
                    )
                    .into());
                }
            } else if !shirabe_php_shim::unlink(&file) && shirabe_php_shim::file_exists(&file) {
                return Err(IOException::new(
                    format!("Failed to remove file \"{}\": ", file),
                    0,
                    None,
                    None,
                )
                .into());
            }
        }
        Ok(())
    }

    pub fn symlink(
        &self,
        origin_dir: &str,
        target_dir: &str,
        _copy_on_windows: bool,
    ) -> anyhow::Result<()> {
        // On Unix DIRECTORY_SEPARATOR is '/', so the Windows-only path normalization and
        // copy-on-windows branch never run.
        self.mkdir(
            PhpMixed::String(shirabe_php_shim::dirname(target_dir)),
            0o777,
        )?;

        if shirabe_php_shim::is_link(target_dir) {
            if self.read_link(target_dir) == origin_dir {
                return Ok(());
            }
            self.remove(PhpMixed::String(target_dir.to_string()))?;
        }

        if !shirabe_php_shim::symlink(origin_dir, target_dir) {
            return Self::link_exception(origin_dir, target_dir, "symbolic");
        }
        Ok(())
    }

    fn link_exception(origin: &str, target: &str, link_type: &str) -> anyhow::Result<()> {
        // The Windows error-code-1314 branch never runs on Unix.
        Err(IOException::new(
            format!(
                "Failed to create \"{}\" link from \"{}\" to \"{}\": ",
                link_type, origin, target
            ),
            0,
            None,
            Some(target.to_string()),
        )
        .into())
    }

    fn read_link(&self, path: &str) -> String {
        // Symfony's readlink() with $canonicalize = false: returns null if the path is not a link.
        // The Rust signature is non-Option, so the non-link case yields the path's readlink result
        // (empty string on failure) to keep the symlink() caller working.
        std::fs::read_link(path)
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    // PHP stream_is_local(): true for plain paths and the file:// wrapper, false for remote
    // wrappers (http://, ftp://, ...). Approximated via the URL scheme since there is no shim.
    fn stream_is_local(path: &str) -> bool {
        let scheme = shirabe_php_shim::parse_url(path, shirabe_php_shim::PHP_URL_SCHEME);
        match scheme {
            PhpMixed::Null | PhpMixed::Bool(false) => true,
            PhpMixed::String(s) => s.eq_ignore_ascii_case("file"),
            _ => true,
        }
    }

    pub fn mirror(
        &self,
        origin_dir: &str,
        target_dir: &str,
        iterator: Option<PhpMixed>,
        options: &indexmap::IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        if iterator.is_some() {
            // TODO(phase-c): Symfony's mirror() accepts a \Traversable filter iterator. The
            // external-package Filesystem does not model that iterator type, and every caller passes
            // None, so the filtered case is left unimplemented.
            todo!()
        }

        let target_dir = shirabe_php_shim::rtrim(target_dir, Some("/\\"));
        let origin_dir = shirabe_php_shim::rtrim(origin_dir, Some("/\\"));
        let origin_dir_len = origin_dir.len();

        if !self.exists(PhpMixed::String(origin_dir.clone())) {
            return Err(IOException::new(
                format!(
                    "The origin directory specified \"{}\" was not found.",
                    origin_dir
                ),
                0,
                None,
                Some(origin_dir.clone()),
            )
            .into());
        }

        let delete = options
            .get("delete")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Iterate in destination folder to remove obsolete entries.
        if self.exists(PhpMixed::String(target_dir.clone())) && delete {
            let target_dir_len = target_dir.len();
            if let Ok(dir) = shirabe_php_shim::recursive_directory_iterator(
                &target_dir,
                shirabe_php_shim::SKIP_DOTS,
            ) {
                let delete_iterator = shirabe_php_shim::recursive_iterator_iterator(
                    dir,
                    shirabe_php_shim::RecursiveIteratorIterator::CHILD_FIRST,
                );
                for file in &delete_iterator {
                    let pathname = file.get_pathname();
                    let origin = format!("{}{}", origin_dir, &pathname[target_dir_len..]);
                    if !self.exists(PhpMixed::String(origin)) {
                        self.remove(PhpMixed::String(pathname))?;
                    }
                }
            }
        }

        let copy_on_windows = options
            .get("copy_on_windows")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let flags = if copy_on_windows {
            shirabe_php_shim::SKIP_DOTS
                | shirabe_php_shim::RecursiveDirectoryIterator::FOLLOW_SYMLINKS
        } else {
            shirabe_php_shim::SKIP_DOTS
        };
        let dir = shirabe_php_shim::recursive_directory_iterator(&origin_dir, flags)
            .map_err(|e| anyhow::anyhow!("{}", e.message))?;
        let iterator = shirabe_php_shim::recursive_iterator_iterator(
            dir,
            shirabe_php_shim::RecursiveIteratorIterator::SELF_FIRST,
        );

        self.mkdir(PhpMixed::String(target_dir.clone()), 0o777)?;

        let override_file = options
            .get("override")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        for file in &iterator {
            let pathname = file.get_pathname();
            if pathname == target_dir {
                continue;
            }

            let target = format!("{}{}", target_dir, &pathname[origin_dir_len..]);

            if !copy_on_windows && file.is_link() {
                self.symlink(&self.read_link(&pathname), &target, false)?;
            } else if file.is_dir() {
                self.mkdir(PhpMixed::String(target), 0o777)?;
            } else if file.is_file() {
                self.copy(&pathname, &target, override_file)?;
            } else {
                return Err(IOException::new(
                    format!("Unable to guess \"{}\" file type.", pathname),
                    0,
                    None,
                    Some(pathname.clone()),
                )
                .into());
            }
        }
        Ok(())
    }
}
