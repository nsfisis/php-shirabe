//! ref: composer/vendor/symfony/filesystem/Filesystem.php

use crate::exception::io_exception::IOException;
use shirabe_php_shim::Catch as _;

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

    fn copy(&self, origin_file: &str, target_file: &str) -> anyhow::Result<()> {
        // PHP: stream_is_local($originFile) || 0 === stripos($originFile, 'file://')
        let origin_is_local = shirabe_php_shim::stream_is_local(origin_file)
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

        self.mkdir(&shirabe_php_shim::dirname(target_file), 0o777)?;

        let mut do_copy = true;
        // PHP: !$overwriteNewerFiles && !parse_url($originFile, PHP_URL_HOST) && is_file($targetFile)
        let origin_host = shirabe_php_shim::parse_url(origin_file, shirabe_php_shim::PHP_URL_HOST);
        if matches!(
            origin_host,
            shirabe_php_shim::PhpMixed::Null | shirabe_php_shim::PhpMixed::Bool(false)
        ) && shirabe_php_shim::is_file(target_file)
        {
            do_copy = shirabe_php_shim::filemtime(origin_file).unwrap_or(0)
                > shirabe_php_shim::filemtime(target_file).unwrap_or(0);
        }

        if do_copy {
            // PHP writes the target through fopen($targetFile, 'w'), which leaves an existing
            // file's mode alone and gives a new one 0666 & ~umask. copy() stamps the origin's mode
            // on the target instead, so the mode fopen would have left is captured here and put
            // back below.
            let target_perms = if shirabe_php_shim::is_file(target_file) {
                shirabe_php_shim::fileperms(target_file)
            } else {
                0o666 & !(shirabe_php_shim::umask() as i64)
            };

            if !shirabe_php_shim::copy(origin_file, target_file) {
                return Err(IOException::new(
                    format!("Failed to copy \"{}\" to \"{}\".", origin_file, target_file),
                    0,
                    None,
                    Some(origin_file.to_string()),
                )
                .into());
            }
            let bytes_copied = shirabe_php_shim::filesize(target_file);

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
                    (target_perms | (shirabe_php_shim::fileperms(origin_file) & 0o111)) as u32,
                );

                // Like `cp`, preserve the file modification time.
                shirabe_php_shim::touch2(
                    target_file,
                    shirabe_php_shim::filemtime(origin_file).unwrap_or(0),
                );

                let bytes_origin = shirabe_php_shim::filesize(origin_file);
                if bytes_copied != bytes_origin {
                    return Err(IOException::new(
                        format!(
                            "Failed to copy the whole content of \"{}\" to \"{}\" ({} of {} bytes copied).",
                            origin_file,
                            target_file,
                            bytes_copied.unwrap_or(0),
                            bytes_origin.unwrap_or(0)
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

    fn mkdir(&self, dir: &str, mode: u32) -> anyhow::Result<()> {
        if shirabe_php_shim::is_dir(dir) {
            return Ok(());
        }

        if shirabe_php_shim::mkdir(dir, mode, true).is_err() && !shirabe_php_shim::is_dir(dir) {
            return Err(IOException::new(
                format!("Failed to create \"{}\".", dir),
                0,
                None,
                Some(dir.to_string()),
            )
            .into());
        }
        Ok(())
    }

    fn exists(&self, file: &str) -> anyhow::Result<bool> {
        let max_path_length = shirabe_php_shim::PHP_MAXPATHLEN - 2;

        if file.len() as i64 > max_path_length {
            return Err(IOException::new(
                format!(
                    "Could not check if file exist because path length exceeds {} characters.",
                    max_path_length
                ),
                0,
                None,
                Some(file.to_string()),
            )
            .into());
        }

        Ok(shirabe_php_shim::file_exists(file))
    }

    fn remove(&self, file: &str) -> anyhow::Result<()> {
        Self::do_remove(vec![file.to_string()], false)
    }

    fn do_remove(files: Vec<String>, is_recursive: bool) -> anyhow::Result<()> {
        // PHP reverses the list so that directory contents are removed before the directory itself.
        let mut files = files;
        files.reverse();
        for mut file in files {
            if shirabe_php_shim::is_link(&file) {
                // See https://bugs.php.net/52176
                let removed = shirabe_php_shim::unlink(&file).is_ok()
                    || !cfg!(windows)
                    || shirabe_php_shim::rmdir(&file).is_ok();
                if !removed && shirabe_php_shim::file_exists(&file) {
                    return Err(IOException::new(
                        format!("Failed to remove symlink \"{}\".", file),
                        0,
                        None,
                        None,
                    )
                    .into());
                }
            } else if shirabe_php_shim::is_dir(&file) {
                // Removing the directory under a random hidden name keeps another process from
                // recreating the path while its contents are being removed. The rename is undone
                // if the final rmdir fails, so a failure leaves the path where the caller left it.
                let mut orig_file = None;
                if !is_recursive {
                    let tmp_name = format!(
                        "{}/.!{}",
                        shirabe_php_shim::dirname(
                            &shirabe_php_shim::realpath(&file).unwrap_or_default()
                        ),
                        shirabe_php_shim::strrev(&shirabe_php_shim::strtr(
                            &shirabe_php_shim::base64_encode(shirabe_php_shim::random_bytes(2)),
                            "/=",
                            "-!",
                        ))
                    );

                    if shirabe_php_shim::file_exists(&tmp_name)
                        && let Err(error) = Self::do_remove(vec![tmp_name.clone()], true)
                        && error.catch::<IOException>().is_none()
                    {
                        return Err(error);
                    }

                    if !shirabe_php_shim::file_exists(&tmp_name)
                        && shirabe_php_shim::rename(&file, &tmp_name)
                    {
                        orig_file = Some(file);
                        file = tmp_name;
                    }
                }

                let entries = shirabe_php_shim::filesystem_iterator(
                    &file,
                    shirabe_php_shim::FilesystemIterator::CURRENT_AS_PATHNAME
                        | shirabe_php_shim::FilesystemIterator::SKIP_DOTS,
                )?;
                Self::do_remove(entries, true)?;

                if let Err(last_error) = shirabe_php_shim::rmdir(&file)
                    && shirabe_php_shim::file_exists(&file)
                    && !is_recursive
                {
                    if let Some(orig_file) = orig_file {
                        shirabe_php_shim::rename(&file, &orig_file);
                    }
                    return Err(IOException::new(
                        format!("Failed to remove directory \"{}\": {}", file, last_error),
                        0,
                        None,
                        None,
                    )
                    .into());
                }
            } else if let Err(last_error) = shirabe_php_shim::unlink(&file) {
                let last_error = last_error.to_string();
                if last_error.contains("Permission denied") || shirabe_php_shim::file_exists(&file)
                {
                    return Err(IOException::new(
                        format!("Failed to remove file \"{}\": {}", file, last_error),
                        0,
                        None,
                        None,
                    )
                    .into());
                }
            }
        }
        Ok(())
    }

    pub fn symlink(&self, origin_dir: &str, target_dir: &str) -> anyhow::Result<()> {
        let mut origin_dir = origin_dir.to_string();
        let mut target_dir = target_dir.to_string();

        if cfg!(windows) {
            origin_dir = shirabe_php_shim::strtr(&origin_dir, "/", "\\");
            target_dir = shirabe_php_shim::strtr(&target_dir, "/", "\\");
        }

        self.mkdir(&shirabe_php_shim::dirname(&target_dir), 0o777)?;

        if shirabe_php_shim::is_link(&target_dir) {
            if shirabe_php_shim::readlink(&target_dir).as_deref() == Some(origin_dir.as_str()) {
                return Ok(());
            }
            self.remove(&target_dir)?;
        }

        if let Err(last_error) = shirabe_php_shim::symlink(&origin_dir, &target_dir) {
            return Err(IOException::new(
                format!(
                    "Failed to create \"symbolic\" link from \"{}\" to \"{}\": {}",
                    origin_dir, target_dir, last_error
                ),
                0,
                None,
                Some(target_dir.clone()),
            )
            .into());
        }
        Ok(())
    }

    pub fn mirror(
        &self,
        origin_dir: &str,
        target_dir: &str,
        iterator: Vec<String>,
    ) -> anyhow::Result<()> {
        let target_dir = shirabe_php_shim::rtrim(target_dir, Some("/\\"));
        let origin_dir = shirabe_php_shim::rtrim(origin_dir, Some("/\\"));
        let origin_dir_len = origin_dir.len();

        if !self.exists(&origin_dir)? {
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

        self.mkdir(&target_dir, 0o777)?;

        let mut files_created_while_mirroring: indexmap::IndexMap<String, bool> =
            indexmap::IndexMap::new();

        for pathname in iterator {
            // SplFileInfo::getRealPath(), which returns false for a path that cannot be resolved.
            let real_path = shirabe_php_shim::realpath(&pathname);
            if pathname == target_dir
                || real_path.as_deref() == Some(target_dir.as_str())
                || real_path
                    .as_ref()
                    .is_some_and(|p| files_created_while_mirroring.contains_key(p))
            {
                continue;
            }

            let target = format!("{}{}", target_dir, &pathname[origin_dir_len..]);
            files_created_while_mirroring.insert(target.clone(), true);

            if shirabe_php_shim::is_link(&pathname) {
                // PHP coerces the false getLinkTarget() returns on failure to the empty string.
                self.symlink(
                    &shirabe_php_shim::readlink(&pathname).unwrap_or_default(),
                    &target,
                )?;
            } else if shirabe_php_shim::is_dir(&pathname) {
                self.mkdir(&target, 0o777)?;
            } else if shirabe_php_shim::is_file(&pathname) {
                self.copy(&pathname, &target)?;
            } else {
                return Err(IOException::new(
                    format!("Unable to guess \"{}\" file type.", pathname),
                    0,
                    None,
                    Some(pathname),
                )
                .into());
            }
        }
        Ok(())
    }
}
