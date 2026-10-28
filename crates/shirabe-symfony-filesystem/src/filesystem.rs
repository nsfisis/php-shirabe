//! ref: composer/vendor/symfony/filesystem/Filesystem.php

use crate::exception::IOException;
use shirabe_php_shim::{
    Catch as _, FilesystemIterator, PHP_MAXPATHLEN, base64_encode, chmod, copy, dirname,
    file_exists, filemtime, fileperms, filesize, filesystem_iterator, is_dir, is_file, is_link,
    legacy_lossy, mkdir, parse_url, random_bytes, readlink, realpath, rename, rmdir, rtrim2,
    stream_is_local, stripos, strrev, strtr, symlink, touch2, umask, unlink,
};

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
        let origin_is_local =
            stream_is_local(origin_file) || stripos(origin_file, "file://") == Some(0);

        if origin_is_local && !is_file(origin_file) {
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

        self.mkdir(&dirname(target_file), 0o777)?;

        let mut do_copy = true;
        // PHP: !$overwriteNewerFiles && !parse_url($originFile, PHP_URL_HOST) && is_file($targetFile)
        let origin_host = parse_url(origin_file).and_then(|url| url.host);
        if origin_host.is_none() && is_file(target_file) {
            do_copy = filemtime(origin_file).unwrap_or(0) > filemtime(target_file).unwrap_or(0);
        }

        if do_copy {
            // PHP writes the target through fopen($targetFile, 'w'), which leaves an existing
            // file's mode alone and gives a new one 0666 & ~umask. copy() stamps the origin's mode
            // on the target instead, so the mode fopen would have left is captured here and put
            // back below.
            let target_perms = if is_file(target_file) {
                fileperms(target_file)?
            } else {
                0o666 & !umask()
            };

            if copy(origin_file, target_file).is_err() {
                return Err(IOException::new(
                    format!("Failed to copy \"{}\" to \"{}\".", origin_file, target_file),
                    0,
                    None,
                    Some(origin_file.to_string()),
                )
                .into());
            }
            let bytes_copied = filesize(target_file);

            if !is_file(target_file) {
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
                chmod(
                    target_file,
                    (target_perms | (fileperms(origin_file)? & 0o111)) as u32,
                );

                // Like `cp`, preserve the file modification time.
                touch2(target_file, filemtime(origin_file).unwrap_or(0));

                let bytes_origin = filesize(origin_file);
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
        if is_dir(dir) {
            return Ok(());
        }

        if mkdir(dir, mode, true).is_err() && !is_dir(dir) {
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
        let max_path_length = PHP_MAXPATHLEN - 2;

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

        Ok(file_exists(file))
    }

    fn remove(&self, file: &str) -> anyhow::Result<()> {
        Self::do_remove(vec![file.to_string()], false)
    }

    fn do_remove(files: Vec<String>, is_recursive: bool) -> anyhow::Result<()> {
        // PHP reverses the list so that directory contents are removed before the directory itself.
        let mut files = files;
        files.reverse();
        for mut file in files {
            if is_link(&file) {
                // See https://bugs.php.net/52176
                let removed = unlink(&file).is_ok() || !cfg!(windows) || rmdir(&file).is_ok();
                if !removed && file_exists(&file) {
                    return Err(IOException::new(
                        format!("Failed to remove symlink \"{}\".", file),
                        0,
                        None,
                        None,
                    )
                    .into());
                }
            } else if is_dir(&file) {
                // Removing the directory under a random hidden name keeps another process from
                // recreating the path while its contents are being removed. The rename is undone
                // if the final rmdir fails, so a failure leaves the path where the caller left it.
                let mut orig_file = None;
                if !is_recursive {
                    let tmp_name = format!(
                        "{}/.!{}",
                        dirname(&realpath(&file).unwrap_or_default()),
                        strrev(strtr(base64_encode(random_bytes(2)), "/=", "-!"))
                    );

                    if file_exists(&tmp_name)
                        && let Err(error) = Self::do_remove(vec![tmp_name.clone()], true)
                        && error.catch::<IOException>().is_none()
                    {
                        return Err(error);
                    }

                    if !file_exists(&tmp_name) && rename(&file, &tmp_name).is_ok() {
                        orig_file = Some(file);
                        file = tmp_name;
                    }
                }

                let entries = filesystem_iterator(
                    &file,
                    FilesystemIterator::CURRENT_AS_PATHNAME | FilesystemIterator::SKIP_DOTS,
                )?;
                // TODO(bytes): do_remove takes the paths as Strings.
                let entries = entries
                    .into_iter()
                    .map(|entry| {
                        entry
                            .into_os_string()
                            .into_string()
                            .unwrap_or_else(|entry| panic!("path is not valid UTF-8: {entry:?}"))
                    })
                    .collect();
                Self::do_remove(entries, true)?;

                if let Err(last_error) = rmdir(&file)
                    && file_exists(&file)
                    && !is_recursive
                {
                    if let Some(orig_file) = orig_file {
                        let _ = rename(&file, &orig_file);
                    }
                    return Err(IOException::new(
                        format!("Failed to remove directory \"{}\": {}", file, last_error),
                        0,
                        None,
                        None,
                    )
                    .into());
                }
            } else if let Err(last_error) = unlink(&file) {
                let last_error = last_error.to_string();
                if last_error.contains("Permission denied") || file_exists(&file) {
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
            origin_dir = legacy_lossy(&strtr(&origin_dir, "/", "\\")).into_owned();
            target_dir = legacy_lossy(&strtr(&target_dir, "/", "\\")).into_owned();
        }

        self.mkdir(&dirname(&target_dir), 0o777)?;

        if is_link(&target_dir) {
            // PHP compares the raw strings, so `Path`'s component-wise equality is not used.
            if readlink(&target_dir)
                .is_some_and(|link| link.as_os_str() == std::ffi::OsStr::new(&origin_dir))
            {
                return Ok(());
            }
            self.remove(&target_dir)?;
        }

        if let Err(last_error) = symlink(&origin_dir, &target_dir) {
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
        let target_dir = legacy_lossy(&rtrim2(target_dir, "/\\")).into_owned();
        let origin_dir = legacy_lossy(&rtrim2(origin_dir, "/\\")).into_owned();
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
            let real_path = realpath(&pathname);
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

            if is_link(&pathname) {
                // PHP coerces the false getLinkTarget() returns on failure to the empty string.
                // TODO(bytes): symlink() takes the link target as a &str.
                let link_target = readlink(&pathname)
                    .unwrap_or_default()
                    .into_os_string()
                    .into_string()
                    .unwrap_or_else(|link| panic!("link target is not valid UTF-8: {link:?}"));
                self.symlink(&link_target, &target)?;
            } else if is_dir(&pathname) {
                self.mkdir(&target, 0o777)?;
            } else if is_file(&pathname) {
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
