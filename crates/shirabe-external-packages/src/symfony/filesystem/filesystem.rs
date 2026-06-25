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

    pub fn copy(
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

    pub fn mkdir(&self, dirs: PhpMixed, mode: u32) -> anyhow::Result<()> {
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

    pub fn exists(&self, files: PhpMixed) -> bool {
        for file in Self::to_iterable(&files) {
            if !shirabe_php_shim::file_exists(&file) {
                return false;
            }
        }
        true
    }

    pub fn touch(
        &self,
        files: PhpMixed,
        time: Option<i64>,
        atime: Option<i64>,
    ) -> anyhow::Result<()> {
        for file in Self::to_iterable(&files) {
            let ok = match time {
                // PHP: self::box('touch', $file, $time, $atime)
                Some(t) => match atime {
                    Some(a) => shirabe_php_shim::touch3(&file, t, a),
                    None => shirabe_php_shim::touch2(&file, t),
                },
                None => shirabe_php_shim::touch(&file),
            };
            if !ok {
                return Err(IOException::new(
                    format!("Failed to touch \"{}\": ", file),
                    0,
                    None,
                    Some(file.clone()),
                )
                .into());
            }
        }
        Ok(())
    }

    pub fn remove(&self, files: PhpMixed) -> anyhow::Result<()> {
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

    pub fn chmod(
        &self,
        files: PhpMixed,
        mode: u32,
        umask: u32,
        recursive: bool,
    ) -> anyhow::Result<()> {
        for file in Self::to_iterable(&files) {
            if !shirabe_php_shim::chmod(&file, mode & !umask) {
                return Err(IOException::new(
                    format!("Failed to chmod file \"{}\": ", file),
                    0,
                    None,
                    Some(file.clone()),
                )
                .into());
            }
            if recursive && shirabe_php_shim::is_dir(&file) && !shirabe_php_shim::is_link(&file) {
                let children = Self::dir_children(&file);
                self.chmod(PhpMixed::List(children), mode, umask, true)?;
            }
        }
        Ok(())
    }

    // Immediate directory children, matching PHP's new \FilesystemIterator($file) (non-recursive,
    // skips "." and ".."). Returned as PhpMixed::String list entries for the chmod recursion.
    fn dir_children(dir: &str) -> Vec<PhpMixed> {
        match std::fs::read_dir(dir) {
            Ok(rd) => rd
                .flatten()
                .map(|e| PhpMixed::String(e.path().to_string_lossy().into_owned()))
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    pub fn chown(&self, _files: PhpMixed, _user: PhpMixed, _recursive: bool) -> anyhow::Result<()> {
        // TODO(phase-d): chown/lchown have no std or existing-shim equivalent (changing a file's
        // owner needs chown(2), not exposed by std and no libc/syscall crate is available).
        todo!()
    }

    pub fn chgrp(
        &self,
        _files: PhpMixed,
        _group: PhpMixed,
        _recursive: bool,
    ) -> anyhow::Result<()> {
        // TODO(phase-d): chgrp/lchgrp have no std or existing-shim equivalent (changing a file's
        // group needs chown(2)/chgrp, not exposed by std and no libc/syscall crate is available).
        todo!()
    }

    pub fn rename(&self, origin: &str, target: &str, override_file: bool) -> anyhow::Result<()> {
        // we check that target does not exist
        if !override_file && self.is_readable_instance(target)? {
            return Err(IOException::new(
                format!(
                    "Cannot rename because the target \"{}\" already exists.",
                    target
                ),
                0,
                None,
                Some(target.to_string()),
            )
            .into());
        }

        if !shirabe_php_shim::rename(origin, target) {
            if shirabe_php_shim::is_dir(origin) {
                // See https://bugs.php.net/54097 & https://php.net/rename#113943
                let mut options: indexmap::IndexMap<String, PhpMixed> = indexmap::IndexMap::new();
                options.insert("override".to_string(), PhpMixed::Bool(override_file));
                options.insert("delete".to_string(), PhpMixed::Bool(override_file));
                self.mirror(origin, target, None, &options)?;
                self.remove(PhpMixed::String(origin.to_string()))?;
                return Ok(());
            }
            return Err(IOException::new(
                format!("Cannot rename \"{}\" to \"{}\": ", origin, target),
                0,
                None,
                Some(target.to_string()),
            )
            .into());
        }
        Ok(())
    }

    // Symfony's private isReadable(): like exists() it guards a path-length limit, then defers to
    // PHP's is_readable().
    fn is_readable_instance(&self, filename: &str) -> anyhow::Result<bool> {
        Ok(shirabe_php_shim::is_readable(filename))
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

    pub fn hard_link(&self, origin_file: &str, target_files: PhpMixed) -> anyhow::Result<()> {
        if !self.exists(PhpMixed::String(origin_file.to_string())) {
            // FileNotFoundException is not modeled in this crate's exception module; surface the
            // path-only message as an IOException, matching the FileNotFoundException(null,...) form.
            return Err(IOException::new(
                format!("File \"{}\" could not be found.", origin_file),
                0,
                None,
                Some(origin_file.to_string()),
            )
            .into());
        }

        if !shirabe_php_shim::is_file(origin_file) {
            return Err(IOException::new(
                format!("Origin file \"{}\" is not a file.", origin_file),
                0,
                None,
                None,
            )
            .into());
        }

        for target_file in Self::to_iterable(&target_files) {
            if shirabe_php_shim::is_file(&target_file) {
                if Self::fileinode(origin_file) == Self::fileinode(&target_file) {
                    continue;
                }
                self.remove(PhpMixed::String(target_file.clone()))?;
            }

            if std::fs::hard_link(origin_file, &target_file).is_err() {
                return Self::link_exception(origin_file, &target_file, "hard");
            }
        }
        Ok(())
    }

    // PHP fileinode(): the file's inode number, or None on failure.
    fn fileinode(path: &str) -> Option<u64> {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(path).ok().map(|m| m.ino())
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

    pub fn read_link(&self, path: &str) -> String {
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

    pub fn make_path_relative(&self, end_path: &str, start_path: &str) -> String {
        // PHP throws InvalidArgumentException when either path is not absolute; that exception type
        // is not modeled here and callers pass absolute paths, so the guards are omitted.

        // On Unix the Windows separator normalization and drive-letter splitting are no-ops.
        let split_path = |path: &str| -> Vec<String> {
            let mut result: Vec<String> = Vec::new();
            for segment in shirabe_php_shim::trim(path, Some("/")).split('/') {
                if segment == ".." {
                    result.pop();
                } else if segment != "." && !segment.is_empty() {
                    result.push(segment.to_string());
                }
            }
            result
        };

        let start_path_arr = split_path(start_path);
        let end_path_arr = split_path(end_path);

        // Find for which directory the common path stops.
        let mut index = 0;
        while index < start_path_arr.len()
            && index < end_path_arr.len()
            && start_path_arr[index] == end_path_arr[index]
        {
            index += 1;
        }

        // Determine how deep the start path is relative to the common path.
        let depth = if start_path_arr.len() == 1 && start_path_arr[0].is_empty() {
            0
        } else {
            start_path_arr.len() - index
        };

        // Repeated "../" for each level needed to reach the common path.
        let traverser = "../".repeat(depth);

        let end_path_remainder = end_path_arr[index..].join("/");

        let relative_path = format!(
            "{}{}",
            traverser,
            if !end_path_remainder.is_empty() {
                format!("{}/", end_path_remainder)
            } else {
                String::new()
            }
        );

        if relative_path.is_empty() {
            "./".to_string()
        } else {
            relative_path
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

    pub fn is_absolute_path(&self, file: &str) -> bool {
        if file.is_empty() {
            return false;
        }
        let bytes = file.as_bytes();
        // strspn($file, '/\\', 0, 1): the first character is a forward or back slash.
        let leading_slash = matches!(bytes[0], b'/' | b'\\');
        // Windows drive letter form: "C:\" / "C:/".
        let drive_letter = file.len() > 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'/' | b'\\');
        // null !== parse_url($file, PHP_URL_SCHEME): the path has a URL scheme.
        let scheme = shirabe_php_shim::parse_url(file, shirabe_php_shim::PHP_URL_SCHEME);
        let has_scheme = !matches!(scheme, PhpMixed::Null | PhpMixed::Bool(false));
        leading_slash || drive_letter || has_scheme
    }

    pub fn dump_file(&self, filename: &str, content: &str) -> anyhow::Result<()> {
        let dir = shirabe_php_shim::dirname(filename);

        if shirabe_php_shim::is_link(filename) {
            let link_target = self.read_link(filename);
            if !link_target.is_empty() {
                // Path::makeAbsolute resolves the link target against the file's directory; the
                // Symfony Path helper is not ported, so an absolute target is used as-is and a
                // relative one is joined to the directory.
                let resolved = if self.is_absolute_path(&link_target) {
                    link_target
                } else {
                    format!("{}/{}", dir, link_target)
                };
                return self.dump_file(&resolved, content);
            }
        }

        if !shirabe_php_shim::is_dir(&dir) {
            self.mkdir(PhpMixed::String(dir.clone()), 0o777)?;
        }

        // Creates a temp file with 0600 access rights when the filesystem supports chmod.
        let tmp_file = self.temp_nam(&dir, &shirabe_php_shim::basename(filename))?;

        let result: anyhow::Result<()> = (|| {
            if shirabe_php_shim::file_put_contents(&tmp_file, content.as_bytes()).is_none() {
                return Err(IOException::new(
                    format!("Failed to write file \"{}\": ", filename),
                    0,
                    None,
                    Some(filename.to_string()),
                )
                .into());
            }

            let perms = shirabe_php_shim::fileperms(filename);
            let mode = if perms != 0 {
                perms as u32
            } else {
                0o666 & !shirabe_php_shim::umask()
            };
            shirabe_php_shim::chmod(&tmp_file, mode);

            self.rename(&tmp_file, filename, true)
        })();

        // finally: clean up the temp file if it still exists.
        if shirabe_php_shim::file_exists(&tmp_file) {
            shirabe_php_shim::unlink(&tmp_file);
        }

        result
    }

    pub fn append_to_file(&self, filename: &str, content: &str) -> anyhow::Result<()> {
        let dir = shirabe_php_shim::dirname(filename);

        if !shirabe_php_shim::is_dir(&dir) {
            self.mkdir(PhpMixed::String(dir), 0o777)?;
        }

        if shirabe_php_shim::file_put_contents3(filename, content, shirabe_php_shim::FILE_APPEND)
            .is_none()
        {
            return Err(IOException::new(
                format!("Failed to write file \"{}\": ", filename),
                0,
                None,
                Some(filename.to_string()),
            )
            .into());
        }
        Ok(())
    }

    pub fn temp_nam(&self, dir: &str, prefix: &str) -> anyhow::Result<String> {
        // getSchemeAndHierarchy(): with no "scheme://" the scheme is null and the hierarchy is the
        // whole path, so the local-filesystem branch (empty suffix) always applies here.
        let (scheme, hierarchy) = Self::get_scheme_and_hierarchy(dir);

        if scheme.is_none() || scheme.as_deref() == Some("file") || scheme.as_deref() == Some("gs")
        {
            if let Some(tmp_file) = shirabe_php_shim::tempnam(&hierarchy, prefix) {
                if let Some(scheme) = &scheme {
                    if scheme != "gs" {
                        return Ok(format!("{}://{}", scheme, tmp_file));
                    }
                }
                return Ok(tmp_file);
            }

            return Err(IOException::new(
                "A temporary file could not be created: ".to_string(),
                0,
                None,
                None,
            )
            .into());
        }

        Err(IOException::new(
            "A temporary file could not be created: ".to_string(),
            0,
            None,
            None,
        )
        .into())
    }

    // Gets a (scheme, hierarchy) tuple of a filename (e.g. file:///tmp -> (Some("file"), "/tmp")).
    fn get_scheme_and_hierarchy(filename: &str) -> (Option<String>, String) {
        match filename.split_once("://") {
            Some((scheme, hierarchy)) => (Some(scheme.to_string()), hierarchy.to_string()),
            None => (None, filename.to_string()),
        }
    }

    // The following methods belong to Composer's own Composer\Util\Filesystem, not Symfony's
    // Filesystem. They were copied into this stub by mistake; every caller resolves to the ported
    // Composer\Util\Filesystem in crates/shirabe/src/util/filesystem.rs instead. They have no
    // Symfony semantics, so they are left unimplemented here.

    pub fn is_readable(_path: &str) -> bool {
        // TODO(phase-d): not a Symfony Filesystem method; see note above.
        todo!()
    }

    pub fn is_local_path(_path: &str) -> bool {
        // TODO(phase-d): not a Symfony Filesystem method; see note above.
        todo!()
    }

    pub fn trim_trailing_slash(_path: &str) -> String {
        // TODO(phase-d): not a Symfony Filesystem method; see note above.
        todo!()
    }

    pub fn get_platform_path(_path: &str) -> String {
        // TODO(phase-d): not a Symfony Filesystem method; see note above.
        todo!()
    }
}
