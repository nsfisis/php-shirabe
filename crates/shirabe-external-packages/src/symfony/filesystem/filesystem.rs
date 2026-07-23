//! ref: composer/vendor/symfony/filesystem/Filesystem.php

// TODO(phase-c): PHP's box()/self::$lastError mechanism (captures the underlying OS error message
// from a failed native call via set_error_handler) is not modeled anywhere in this file. Every
// IOException message constructed below therefore omits the trailing low-level error string that
// PHP would append (e.g. "Failed to touch \"%s\": ".self::$lastError).

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
                    // TODO(phase-c): PHP distinguishes fopen($originFile) failure ("source file
                    // could not be opened for reading") from fopen($targetFile) failure ("target
                    // file could not be opened for writing"); the shim's copy() collapses both
                    // (and the actual copy failure) into this single generic message.
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

                // TODO(phase-c): PHP preserves the origin file's mtime via
                // touch($targetFile, filemtime($originFile)). shirabe_php_shim::touch2 now exists
                // and could implement this, but it is not wired up here yet.

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
        // TODO(phase-c): PHP throws IOException when a path exceeds PHP_MAXPATHLEN - 2 characters;
        // this port has no such guard, and the plain `bool` return type here cannot express that
        // throw path without changing the signature.
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

    // TODO(phase-c): `is_recursive` is unused. In PHP, doRemove() uses it as a top-level-call guard:
    // on the first (non-recursive) call for a directory, it renames the directory to a random
    // hidden name before recursing into it (and renames it back if rmdir subsequently fails), to
    // avoid a race where another process recreates the path mid-removal. That rename/rollback
    // trick is entirely unported here; this version always operates on the original path.
    fn do_remove(files: Vec<String>, is_recursive: bool) -> anyhow::Result<()> {
        // PHP reverses the list so that directory contents are removed before the directory itself.
        let mut files = files;
        files.reverse();
        for file in files {
            if shirabe_php_shim::is_link(&file) {
                // See https://bugs.php.net/52176
                // TODO(phase-c): PHP's condition is
                // `!(unlink() || '\\' !== DIRECTORY_SEPARATOR || rmdir()) && file_exists()`. On
                // Unix, `'\\' !== DIRECTORY_SEPARATOR` is always true, so the `!(...)` is always
                // false and this branch never throws there regardless of unlink's result (the
                // rmdir fallback and this exception only matter on Windows). This port omits that
                // always-true disjunct, so it CAN throw here on Unix where upstream never would.
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
                // TODO(phase-c): PHP also throws when self::$lastError contains "Permission
                // denied", even if file_exists() is now false (e.g. the file vanished between the
                // failed unlink and this check). That OR-branch is dropped along with the general
                // $lastError omission noted at the top of this file.
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

    // TODO(phase-c): PHP's symlink() has a `'\\' === DIRECTORY_SEPARATOR` branch that (a)
    // normalizes '/' to '\\' in both paths, and (b) when $copyOnWindows is true, mirrors the
    // directory instead of symlinking it and returns early. Neither is ported: `_copy_on_windows`
    // is accepted but unused, so this always symlinks even where PHP would have copied.
    pub fn symlink(
        &self,
        origin_dir: &str,
        target_dir: &str,
        _copy_on_windows: bool,
    ) -> anyhow::Result<()> {
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

    // TODO(phase-c): PHP special-cases a Windows error containing "error code(1314)" with a
    // distinct "Do you have the required Administrator-rights?" message; that check (and the
    // self::$lastError inspection it depends on) is not ported, so this always throws the generic
    // message below.
    fn link_exception(origin: &str, target: &str, link_type: &str) -> anyhow::Result<()> {
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

    // TODO(phase-c): this only ports Symfony's readlink($path, $canonicalize = false) overload;
    // the $canonicalize = true branch (realpath()-based resolution, returning null if the path
    // does not exist at all) is entirely unported. Per the phase B default-argument convention
    // this should become a `read_link2` overload if that branch is ever needed.
    fn read_link(&self, path: &str) -> String {
        // Symfony's readlink() with $canonicalize = false: returns null if the path is not a link.
        // TODO(phase-c): the Rust signature is non-Option, so the non-link case yields the path's
        // readlink result (empty string on failure) instead of PHP's null, to keep the symlink()
        // caller working. Every current caller checks is_link() first, so this never triggers on
        // the live code paths, but the collapsed Option<String> -> String signature is a real
        // divergence from upstream.
        // TODO(phase-c): PHP also has `if ('\\' === DIRECTORY_SEPARATOR && PHP_VERSION_ID < 70400)
        // return realpath($path);` ahead of the plain readlink() call below, working around a
        // pre-7.4 Windows bug. Not ported here; on old Windows PHP this would resolve differently.
        std::fs::read_link(path)
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    // PHP stream_is_local(): true for plain paths and the file:// wrapper, false for remote
    // wrappers (http://, ftp://, ...).
    // TODO(phase-c): this is PHP's built-in stream_is_local(), which queries the registered stream
    // wrapper for STREAM_IS_URL rather than just parsing the scheme. It is approximated here via
    // parse_url()'s scheme instead of being added to shirabe-php-shim, so a registered custom
    // stream wrapper claiming to be local (or vice versa) would be classified differently than PHP.
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

        // TODO(phase-c): PHP's skip condition is
        // `$file->getPathname() === $targetDir || $file->getRealPath() === $targetDir ||
        // isset($filesCreatedWhileMirroring[$file->getRealPath()])`, and it records every
        // `$target` it creates into `$filesCreatedWhileMirroring` to avoid revisiting a path
        // already produced earlier in this same mirror() call (e.g. via a symlink loop back into
        // the tree). Only the plain pathname comparison is ported; the getRealPath() comparison
        // and the created-files dedup set are both omitted.
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
