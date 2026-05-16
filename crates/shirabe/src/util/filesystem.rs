//! ref: composer/src/Composer/Util/Filesystem.php

use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use shirabe_external_packages::symfony::component::filesystem::exception::io_exception::IOException;
use shirabe_external_packages::symfony::component::finder::finder::Finder;
use shirabe_php_shim::{
    DIRECTORY_SEPARATOR, ErrorException, InvalidArgumentException, LogicException, PhpMixed,
    RuntimeException, UnexpectedValueException, array_pop, basename, chdir, clearstatcache, copy,
    count, dirname, end, error_get_last, explode, fclose, feof, file_exists, file_get_contents,
    file_put_contents, fileatime, filemtime, filesize, fopen, fread, function_exists, fwrite,
    implode, is_array, is_dir, is_file, is_link, is_readable, lstat, mkdir, react_promise_resolve,
    rename, rmdir, rtrim, sprintf, str_contains, str_repeat, str_replace, str_starts_with, strlen,
    strpos, strtolower, strtoupper, strtr, substr, substr_count, symlink, touch, unlink, usleep,
    var_export,
};

use crate::util::platform::Platform;
use crate::util::process_executor::ProcessExecutor;
use crate::util::silencer::Silencer;

#[derive(Debug)]
pub struct Filesystem {
    process_executor: Option<ProcessExecutor>,
}

impl Filesystem {
    pub fn new(executor: Option<ProcessExecutor>) -> Self {
        Self {
            process_executor: executor,
        }
    }

    pub fn remove(&mut self, file: &str) -> anyhow::Result<bool> {
        if is_dir(file) {
            return self.remove_directory(file);
        }

        if file_exists(file) {
            return self.unlink(file);
        }

        Ok(false)
    }

    /// Checks if a directory is empty
    pub fn is_dir_empty(&self, dir: &str) -> bool {
        let finder = Finder::create()
            .ignore_vcs(false)
            .ignore_dot_files(false)
            .depth(0)
            .r#in(dir);

        count(&finder) == 0
    }

    pub fn empty_directory(
        &mut self,
        dir: &str,
        ensure_directory_exists: bool,
    ) -> anyhow::Result<()> {
        if is_link(dir) && file_exists(dir) {
            self.unlink(dir)?;
        }

        if ensure_directory_exists {
            self.ensure_directory_exists(dir)?;
        }

        if is_dir(dir) {
            let finder = Finder::create()
                .ignore_vcs(false)
                .ignore_dot_files(false)
                .depth(0)
                .r#in(dir);

            for path in &finder {
                self.remove(&path.to_string())?;
            }
        }
        Ok(())
    }

    /// Recursively remove a directory
    ///
    /// Uses the process component if proc_open is enabled on the PHP
    /// installation.
    pub fn remove_directory(&mut self, directory: &str) -> anyhow::Result<bool> {
        let edge_case_result = self.remove_edge_cases(directory, true)?;
        if let Some(r) = edge_case_result {
            return Ok(r);
        }

        let cmd: Vec<String> = if Platform::is_windows() {
            vec![
                "rmdir".to_string(),
                "/S".to_string(),
                "/Q".to_string(),
                Platform::realpath(directory),
            ]
        } else {
            vec!["rm".to_string(), "-rf".to_string(), directory.to_string()]
        };

        let mut output = String::new();
        let result = self.get_process().execute(&cmd, &mut output) == 0;

        // clear stat cache because external processes aren't tracked by the php stat cache
        clearstatcache(false, "");

        if result && !is_dir(directory) {
            return Ok(true);
        }

        self.remove_directory_php(directory)
    }

    /// Recursively remove a directory asynchronously
    ///
    /// Uses the process component if proc_open is enabled on the PHP
    /// installation.
    pub fn remove_directory_async(
        &mut self,
        directory: &str,
    ) -> anyhow::Result<Box<dyn PromiseInterface>> {
        let edge_case_result = self.remove_edge_cases(directory, true)?;
        if let Some(r) = edge_case_result {
            return Ok(react_promise_resolve(PhpMixed::Bool(r)));
        }

        let cmd: Vec<String> = if Platform::is_windows() {
            vec![
                "rmdir".to_string(),
                "/S".to_string(),
                "/Q".to_string(),
                Platform::realpath(directory),
            ]
        } else {
            vec!["rm".to_string(), "-rf".to_string(), directory.to_string()]
        };

        let promise = self.get_process().execute_async(&cmd);

        let directory_owned = directory.to_string();
        // TODO(plugin): closure capture of $this in PHP — port wires the same logic via a callback handle.
        Ok(promise.then(Box::new(
            move |process: PhpMixed| -> Box<dyn PromiseInterface> {
                // clear stat cache because external processes aren't tracked by the php stat cache
                clearstatcache(false, "");

                let is_successful = process
                    .as_object()
                    .map(|o| {
                        o.call_method("isSuccessful", &[])
                            .as_bool()
                            .unwrap_or(false)
                    })
                    .unwrap_or(false);
                if is_successful && !is_dir(&directory_owned) {
                    return react_promise_resolve(PhpMixed::Bool(true));
                }

                // PHP: \React\Promise\resolve($this->removeDirectoryPhp($directory))
                // The recursive PHP call doesn't have a clean async equivalent; we resort to a sync call.
                let mut fs = Filesystem::new(None);
                let res = fs.remove_directory_php(&directory_owned).unwrap_or(false);
                react_promise_resolve(PhpMixed::Bool(res))
            },
        )))
    }

    /// Returns null when no edge case was hit. Otherwise a bool whether removal was successful
    fn remove_edge_cases(
        &mut self,
        directory: &str,
        fallback_to_php: bool,
    ) -> anyhow::Result<Option<bool>> {
        if self.is_symlinked_directory(directory) {
            return Ok(Some(self.unlink_symlinked_directory(directory)?));
        }

        if self.is_junction(directory) {
            return Ok(Some(self.remove_junction(directory)?));
        }

        if is_link(directory) {
            return Ok(Some(unlink(directory)));
        }

        if !is_dir(directory) || !file_exists(directory) {
            return Ok(Some(true));
        }

        if Preg::is_match("{^(?:[a-z]:)?[/\\\\]+$}i", directory, None).unwrap_or(false) {
            return Err(RuntimeException {
                message: format!("Aborting an attempted deletion of {}, this was probably not intended, if it is a real use case please report it.", directory),
                code: 0,
            }
            .into());
        }

        if !function_exists("proc_open") && fallback_to_php {
            return Ok(Some(self.remove_directory_php(directory)?));
        }

        Ok(None)
    }

    /// Recursively delete directory using PHP iterators.
    ///
    /// Uses a CHILD_FIRST RecursiveIteratorIterator to sort files
    /// before directories, creating a single non-recursive loop
    /// to delete files/directories in the correct order.
    pub fn remove_directory_php(&mut self, directory: &str) -> anyhow::Result<bool> {
        let edge_case_result = self.remove_edge_cases(directory, false)?;
        if let Some(r) = edge_case_result {
            return Ok(r);
        }

        // PHP: $it = new RecursiveDirectoryIterator($directory, RecursiveDirectoryIterator::SKIP_DOTS);
        let mut it_result =
            shirabe_php_shim::recursive_directory_iterator(directory, shirabe_php_shim::SKIP_DOTS);
        if let Err(e) = &it_result {
            if e.downcast_ref::<UnexpectedValueException>().is_some() {
                // re-try once after clearing the stat cache if it failed as it
                // sometimes fails without apparent reason, see https://github.com/composer/composer/issues/4009
                clearstatcache(false, "");
                usleep(100000);
                if !is_dir(directory) {
                    return Ok(true);
                }
                it_result = shirabe_php_shim::recursive_directory_iterator(
                    directory,
                    shirabe_php_shim::SKIP_DOTS,
                );
            }
        }
        let it = it_result?;
        let ri = shirabe_php_shim::recursive_iterator_iterator(it, shirabe_php_shim::CHILD_FIRST);

        for file in &ri {
            if file.is_dir() {
                self.rmdir(&file.get_pathname())?;
            } else {
                self.unlink(&file.get_pathname())?;
            }
        }

        // release locks on the directory, see https://github.com/composer/composer/issues/9945
        drop(ri);

        self.rmdir(directory)
    }

    pub fn ensure_directory_exists(&mut self, directory: &str) -> anyhow::Result<()> {
        if !is_dir(directory) {
            if file_exists(directory) {
                return Err(RuntimeException {
                    message: format!("{} exists and is not a directory.", directory),
                    code: 0,
                }
                .into());
            }

            if is_link(directory) && !self.unlink_implementation(directory) {
                return Err(RuntimeException {
                    message: format!(
                        "Could not delete symbolic link {}: {}",
                        directory,
                        error_get_last()
                            .get("message")
                            .and_then(|v| v.as_string())
                            .unwrap_or("")
                    ),
                    code: 0,
                }
                .into());
            }

            if !mkdir(directory, 0o777, true) {
                let e = RuntimeException {
                    message: format!(
                        "{} does not exist and could not be created: {}",
                        directory,
                        error_get_last()
                            .get("message")
                            .and_then(|v| v.as_string())
                            .unwrap_or("")
                    ),
                    code: 0,
                };

                // in pathological cases with paths like path/to/broken-symlink/../foo is_dir will fail to detect path/to/foo
                // but normalizing the ../ away first makes it work so we attempt this just in case, and if it still fails we
                // report the initial error we had with the original path, and ignore the normalized path exception
                // see https://github.com/composer/composer/issues/11864
                let normalized = self.normalize_path(directory);
                if normalized != directory {
                    let _ = self.ensure_directory_exists(&normalized);
                    if is_dir(&normalized) {
                        return Ok(());
                    }
                }

                return Err(e.into());
            }
        }
        Ok(())
    }

    /// Attempts to unlink a file and in case of failure retries after 350ms on windows
    pub fn unlink(&self, path: &str) -> anyhow::Result<bool> {
        let mut unlinked = self.unlink_implementation(path);
        if !unlinked {
            // retry after a bit on windows since it tends to be touchy with mass removals
            if Platform::is_windows() {
                usleep(350000);
                unlinked = self.unlink_implementation(path);
            }

            if !unlinked {
                let error = error_get_last();
                let mut message = format!(
                    "Could not delete {}: {}",
                    path,
                    error
                        .get("message")
                        .and_then(|v| v.as_string())
                        .unwrap_or("")
                );
                if Platform::is_windows() {
                    message.push_str("\nThis can be due to an antivirus or the Windows Search Indexer locking the file while they are analyzed");
                }

                return Err(RuntimeException { message, code: 0 }.into());
            }
        }

        Ok(true)
    }

    /// Attempts to rmdir a file and in case of failure retries after 350ms on windows
    pub fn rmdir(&self, path: &str) -> anyhow::Result<bool> {
        let mut deleted = rmdir(path);
        if !deleted {
            // retry after a bit on windows since it tends to be touchy with mass removals
            if Platform::is_windows() {
                usleep(350000);
                deleted = rmdir(path);
            }

            if !deleted {
                let error = error_get_last();
                let mut message = format!(
                    "Could not delete {}: {}",
                    path,
                    error
                        .get("message")
                        .and_then(|v| v.as_string())
                        .unwrap_or("")
                );
                if Platform::is_windows() {
                    message.push_str("\nThis can be due to an antivirus or the Windows Search Indexer locking the file while they are analyzed");
                }

                return Err(RuntimeException { message, code: 0 }.into());
            }
        }

        Ok(true)
    }

    /// Copy then delete is a non-atomic version of rename.
    ///
    /// Some systems can't rename and also don't have proc_open,
    /// which requires this solution.
    pub fn copy_then_remove(&mut self, source: &str, target: &str) -> anyhow::Result<()> {
        self.copy(source, target)?;
        if !is_dir(source) {
            self.unlink(source)?;

            return Ok(());
        }

        self.remove_directory_php(source)?;
        Ok(())
    }

    /// Copies a file or directory from $source to $target.
    pub fn copy(&mut self, source: &str, target: &str) -> anyhow::Result<bool> {
        // refs https://github.com/composer/composer/issues/11864
        let target = self.normalize_path(target);

        if !is_dir(source) {
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| copy(source, &target)));
            match result {
                Ok(b) => return Ok(b),
                Err(payload) => {
                    let e = match payload.downcast_ref::<ErrorException>() {
                        Some(e) => e.clone(),
                        None => return Err(anyhow::anyhow!("Copy panicked")),
                    };

                    // if copy fails we attempt to copy it manually as this can help bypass issues with VirtualBox shared folders
                    // see https://github.com/composer/composer/issues/12057
                    if str_contains(&e.message, "Bad address") {
                        let source_handle = fopen(source, "r");
                        let target_handle = fopen(&target, "w");
                        if source_handle.is_none() || target_handle.is_none() {
                            return Err(e.into());
                        }
                        let source_handle = source_handle.unwrap();
                        let target_handle = target_handle.unwrap();
                        while !feof(&source_handle) {
                            if !fwrite(&target_handle, &fread(&source_handle, 1024 * 1024)) {
                                return Err(e.into());
                            }
                        }
                        fclose(&source_handle);
                        fclose(&target_handle);

                        return Ok(true);
                    }
                    return Err(e.into());
                }
            }
        }

        let it =
            shirabe_php_shim::recursive_directory_iterator(source, shirabe_php_shim::SKIP_DOTS)?;
        let ri = shirabe_php_shim::recursive_iterator_iterator(it, shirabe_php_shim::SELF_FIRST);
        self.ensure_directory_exists(&target)?;

        let mut result = true;
        for file in &ri {
            let target_path = format!("{}{}{}", target, DIRECTORY_SEPARATOR, ri.get_sub_pathname());
            if file.is_dir() {
                self.ensure_directory_exists(&target_path)?;
            } else {
                result = result && copy(&file.get_pathname(), &target_path);
            }
        }

        Ok(result)
    }

    pub fn rename(&mut self, source: &str, target: &str) -> anyhow::Result<()> {
        if rename(source, target) {
            return Ok(());
        }

        if !function_exists("proc_open") {
            return self.copy_then_remove(source, target);
        }

        if Platform::is_windows() {
            // Try to copy & delete - this is a workaround for random "Access denied" errors.
            let mut output = String::new();
            let result = self.get_process().execute(
                &vec![
                    "xcopy".to_string(),
                    source.to_string(),
                    target.to_string(),
                    "/E".to_string(),
                    "/I".to_string(),
                    "/Q".to_string(),
                    "/Y".to_string(),
                ],
                &mut output,
            );

            // clear stat cache because external processes aren't tracked by the php stat cache
            clearstatcache(false, "");

            if 0 == result {
                self.remove(source)?;

                return Ok(());
            }
        } else {
            // We do not use PHP's "rename" function here since it does not support
            // the case where $source, and $target are located on different partitions.
            let mut output = String::new();
            let result = self.get_process().execute(
                &vec!["mv".to_string(), source.to_string(), target.to_string()],
                &mut output,
            );

            // clear stat cache because external processes aren't tracked by the php stat cache
            clearstatcache(false, "");

            if 0 == result {
                return Ok(());
            }
        }

        self.copy_then_remove(source, target)
    }

    /// Returns the shortest path from $from to $to
    pub fn find_shortest_path(
        &self,
        from: &str,
        to: &str,
        directories: bool,
        prefer_relative: bool,
    ) -> String {
        if !self.is_absolute_path(from) || !self.is_absolute_path(to) {
            // PHP throws InvalidArgumentException
            // Returning early-formatted Result is not possible without changing signature; panic to surface in tests.
            panic!(
                "{}",
                sprintf(
                    "$from (%s) and $to (%s) must be absolute paths.",
                    &[from.to_string().into(), to.to_string().into()]
                )
            );
        }

        let mut from = self.normalize_path(from);
        let to = self.normalize_path(to);

        if directories {
            from = format!("{}/dummy_file", rtrim(&from, "/"));
        }

        if dirname(&from) == dirname(&to) {
            return format!("./{}", basename(&to));
        }

        let mut common_path = to.clone();
        while strpos(&format!("{}/", from), &format!("{}/", common_path)) != Some(0)
            && "/" != common_path
            && !Preg::is_match("{^[A-Z]:/?$}i", &common_path, None).unwrap_or(false)
        {
            common_path = strtr(&dirname(&common_path), "\\", "/");
        }

        // no commonality at all
        if Some(0) != strpos(&from, &common_path) {
            return to;
        }

        common_path = format!("{}/", rtrim(&common_path, "/"));
        let source_path_depth =
            substr_count(&substr(&from, strlen(&common_path) as isize, None), "/");
        let common_path_code = str_repeat("../", source_path_depth);

        // allow top level /foo & /bar dirs to be addressed relatively as this is common in Docker setups
        if !prefer_relative && "/" == common_path && source_path_depth > 1 {
            return to;
        }

        let result = format!(
            "{}{}",
            common_path_code,
            substr(&to, strlen(&common_path) as isize, None)
        );
        if strlen(&result) == 0 {
            return "./".to_string();
        }

        result
    }

    /// Returns PHP code that, when executed in $from, will return the path to $to
    pub fn find_shortest_path_code(
        &self,
        from: &str,
        to: &str,
        directories: bool,
        static_code: bool,
        prefer_relative: bool,
    ) -> String {
        if !self.is_absolute_path(from) || !self.is_absolute_path(to) {
            panic!(
                "{}",
                sprintf(
                    "$from (%s) and $to (%s) must be absolute paths.",
                    &[from.to_string().into(), to.to_string().into()]
                )
            );
        }

        let from = self.normalize_path(from);
        let to = self.normalize_path(to);

        if from == to {
            return (if directories { "__DIR__" } else { "__FILE__" }).to_string();
        }

        let mut common_path = to.clone();
        while strpos(&format!("{}/", from), &format!("{}/", common_path)) != Some(0)
            && "/" != common_path
            && !Preg::is_match("{^[A-Z]:/?$}i", &common_path, None).unwrap_or(false)
            && "." != common_path
        {
            common_path = strtr(&dirname(&common_path), "\\", "/");
        }

        // no commonality at all
        if Some(0) != strpos(&from, &common_path) || "." == common_path {
            return var_export(&PhpMixed::String(to), true);
        }

        common_path = format!("{}/", rtrim(&common_path, "/"));
        if str_starts_with(&to, &format!("{}/", from)) {
            return format!(
                "__DIR__ . {}",
                var_export(
                    &PhpMixed::String(substr(&to, strlen(&from) as isize, None)),
                    true
                )
            );
        }
        let source_path_depth =
            (substr_count(&substr(&from, strlen(&common_path) as isize, None), "/") as i64)
                + (if directories { 1 } else { 0 });

        // allow top level /foo & /bar dirs to be addressed relatively as this is common in Docker setups
        if !prefer_relative && "/" == common_path && source_path_depth > 1 {
            return var_export(&PhpMixed::String(to), true);
        }

        let common_path_code = if static_code {
            format!(
                "__DIR__ . '{}'",
                str_repeat("/..", source_path_depth as usize)
            )
        } else {
            format!(
                "{}{}{}",
                str_repeat("dirname(", source_path_depth as usize),
                "__DIR__",
                str_repeat(")", source_path_depth as usize)
            )
        };
        let rel_target = substr(&to, strlen(&common_path) as isize, None);

        format!(
            "{}{}",
            common_path_code,
            if strlen(&rel_target) > 0 {
                format!(
                    ".{}",
                    var_export(&PhpMixed::String(format!("/{}", rel_target)), true)
                )
            } else {
                String::new()
            }
        )
    }

    /// Checks if the given path is absolute
    pub fn is_absolute_path(&self, path: &str) -> bool {
        strpos(path, "/") == Some(0)
            || substr(path, 1, Some(1)) == ":"
            || strpos(path, "\\\\") == Some(0)
    }

    /// Returns size of a file or directory specified by path. If a directory is
    /// given, its size will be computed recursively.
    pub fn size(&self, path: &str) -> anyhow::Result<i64> {
        if !file_exists(path) {
            return Err(RuntimeException {
                message: format!("{} does not exist.", path),
                code: 0,
            }
            .into());
        }
        if is_dir(path) {
            return Ok(self.directory_size(path));
        }

        Ok(filesize(path) as i64)
    }

    /// Normalize a path. This replaces backslashes with slashes, removes ending
    /// slash and collapses redundant separators and up-level references.
    pub fn normalize_path(&self, path: &str) -> String {
        let mut parts: Vec<String> = vec![];
        let mut path = strtr(path, "\\", "/");
        let mut prefix = String::new();
        let mut absolute = String::new();

        // extract windows UNC paths e.g. \\foo\bar
        if strpos(&path, "//") == Some(0) && strlen(&path) > 2 {
            absolute = "//".to_string();
            path = substr(&path, 2, None);
        }

        // extract a prefix being a protocol://, protocol:, protocol://drive: or simply drive:
        let mut prefix_match: Vec<String> = vec![];
        if Preg::is_match_strict_groups(
            "{^( [0-9a-z]{2,}+: (?: // (?: [a-z]: )? )? | [a-z]: )}ix",
            &path,
            Some(&mut prefix_match),
        )
        .unwrap_or(false)
        {
            prefix = prefix_match[1].clone();
            path = substr(&path, strlen(&prefix) as isize, None);
        }

        if strpos(&path, "/") == Some(0) {
            absolute = "/".to_string();
            path = substr(&path, 1, None);
        }

        let mut up = false;
        for chunk in explode("/", &path) {
            if ".." == chunk && (strlen(&absolute) > 0 || up) {
                array_pop(&mut parts);
                up = !(count(&parts) == 0 || ".." == end(&parts).unwrap_or_default());
            } else if "." != chunk && "" != chunk {
                parts.push(chunk.clone());
                up = ".." != chunk;
            }
        }

        // ensure c: is normalized to C:
        prefix = Preg::replace_callback(
            "{(^|://)[a-z]:$}i",
            Box::new(|m: &Vec<String>| -> String { strtoupper(&m[0]) }),
            &prefix,
        );

        format!("{}{}{}", prefix, absolute, implode("/", &parts))
    }

    /// Remove trailing slashes if present to avoid issues with symlinks
    ///
    /// And other possible unforeseen disasters, see https://github.com/composer/composer/pull/9422
    pub fn trim_trailing_slash(path: &str) -> String {
        let mut path = path.to_string();
        if !Preg::is_match("{^[/\\\\]+$}", &path, None).unwrap_or(false) {
            path = rtrim(&path, "/\\");
        }

        path
    }

    /// Return if the given path is local
    pub fn is_local_path(path: &str) -> bool {
        // on windows, \\foo indicates network paths so we exclude those from local paths, however it is unsafe
        // on linux as file:////foo (which would be a network path \\foo on windows) will resolve to /foo which could be a local path
        if Platform::is_windows() {
            return Preg::is_match(
                "{^(file://(?!//)|/(?!/)|/?[a-z]:[\\\\/]|\\.\\.[\\\\/]|[a-z0-9_.-]+[\\\\/])}i",
                path,
                None,
            )
            .unwrap_or(false);
        }

        Preg::is_match(
            "{^(file://|/|/?[a-z]:[\\\\/]|\\.\\.[\\\\/]|[a-z0-9_.-]+[\\\\/])}i",
            path,
            None,
        )
        .unwrap_or(false)
    }

    pub fn get_platform_path(path: &str) -> String {
        let mut path = path.to_string();
        if Platform::is_windows() {
            path = Preg::replace("{^(?:file:///([a-z]):?/)}i", "file://$1:/", &path);
        }

        Preg::replace("{^file://}i", "", &path)
    }

    /// Cross-platform safe version of is_readable()
    ///
    /// This will also check for readability by reading the file as is_readable can not be trusted on network-mounts
    /// and \\\\wsl$ paths. See https://github.com/composer/composer/issues/8231 and https://bugs.php.net/bug.php?id=68926
    pub fn is_readable(path: &str) -> bool {
        if is_readable(path) {
            return true;
        }

        if is_file(path) {
            return Silencer::call(|| Ok(file_get_contents(path).is_some())).unwrap_or(false);
        }

        if is_dir(path) {
            return Silencer::call(|| Ok(shirabe_php_shim::opendir(path).is_some()))
                .unwrap_or(false);
        }

        // assume false otherwise
        false
    }

    pub(crate) fn directory_size(&self, directory: &str) -> i64 {
        let it =
            shirabe_php_shim::recursive_directory_iterator(directory, shirabe_php_shim::SKIP_DOTS)
                .unwrap();
        let ri = shirabe_php_shim::recursive_iterator_iterator(it, shirabe_php_shim::CHILD_FIRST);

        let mut size: i64 = 0;
        for file in &ri {
            if file.is_file() {
                size += file.get_size();
            }
        }

        size
    }

    pub(crate) fn get_process(&mut self) -> &mut ProcessExecutor {
        if self.process_executor.is_none() {
            self.process_executor = Some(ProcessExecutor::new(None));
        }

        self.process_executor.as_mut().unwrap()
    }

    /// delete symbolic link implementation (commonly known as "unlink()")
    ///
    /// symbolic links on windows which link to directories need rmdir instead of unlink
    fn unlink_implementation(&self, path: &str) -> bool {
        if Platform::is_windows() && is_dir(path) && is_link(path) {
            return rmdir(path);
        }

        unlink(path)
    }

    /// Creates a relative symlink from $link to $target
    pub fn relative_symlink(&self, target: &str, link: &str) -> bool {
        if !function_exists("symlink") {
            return false;
        }

        let cwd = Platform::get_cwd();

        let relative_path = self.find_shortest_path(link, target, false, false);
        chdir(&dirname(link));
        let result = symlink(&relative_path, link);

        chdir(&cwd);

        result
    }

    /// return true if that directory is a symlink.
    pub fn is_symlinked_directory(&self, directory: &str) -> bool {
        if !is_dir(directory) {
            return false;
        }

        let resolved = self.resolve_symlinked_directory_symlink(directory);

        is_link(&resolved)
    }

    fn unlink_symlinked_directory(&self, directory: &str) -> anyhow::Result<bool> {
        let resolved = self.resolve_symlinked_directory_symlink(directory);

        self.unlink(&resolved)
    }

    /// resolve pathname to symbolic link of a directory
    fn resolve_symlinked_directory_symlink(&self, pathname: &str) -> String {
        if !is_dir(pathname) {
            return pathname.to_string();
        }

        let resolved = rtrim(pathname, "/");

        if 0 == strlen(&resolved) {
            return pathname.to_string();
        }

        resolved
    }

    /// Creates an NTFS junction.
    pub fn junction(&mut self, target: &str, junction: &str) -> anyhow::Result<()> {
        if !Platform::is_windows() {
            return Err(LogicException {
                message: format!(
                    "Function {} is not available on non-Windows platform",
                    "Composer\\Util\\Filesystem"
                ),
                code: 0,
            }
            .into());
        }
        if !is_dir(target) {
            return Err(IOException::new(
                format!(
                    "Cannot junction to \"{}\" as it is not a directory.",
                    target
                ),
                0,
                None,
                Some(target.to_string()),
            )
            .into());
        }

        // Removing any previously junction to ensure clean execution.
        if !is_dir(junction) || self.is_junction(junction) {
            let _ = rmdir(junction);
        }

        let cmd = vec![
            "mklink".to_string(),
            "/J".to_string(),
            str_replace("/", DIRECTORY_SEPARATOR, junction),
            Platform::realpath(target),
        ];
        let mut output = String::new();
        if self.get_process().execute(&cmd, &mut output) != 0 {
            return Err(IOException::new(
                format!(
                    "Failed to create junction to \"{}\" at \"{}\".",
                    target, junction
                ),
                0,
                None,
                Some(target.to_string()),
            )
            .into());
        }
        clearstatcache(true, junction);
        Ok(())
    }

    /// Returns whether the target directory is a Windows NTFS Junction.
    ///
    /// We test if the path is a directory and not an ordinary link, then check
    /// that the mode value returned from lstat (which gives the status of the
    /// link itself) is not a directory, by replicating the POSIX S_ISDIR test.
    ///
    /// This logic works because PHP does not set the mode value for a junction,
    /// since there is no universal file type flag for it. Unfortunately an
    /// uninitialized variable in PHP prior to 7.2.16 and 7.3.3 may cause a
    /// random value to be returned. See https://bugs.php.net/bug.php?id=77552
    ///
    /// If this random value passes the S_ISDIR test, then a junction will not be
    /// detected and a recursive delete operation could lead to loss of data in
    /// the target directory. Note that Windows rmdir can handle this situation
    /// and will only delete the junction (from Windows 7 onwards).
    pub fn is_junction(&self, junction: &str) -> bool {
        if !Platform::is_windows() {
            return false;
        }

        // Important to clear all caches first
        clearstatcache(true, junction);

        if !is_dir(junction) || is_link(junction) {
            return false;
        }

        let stat = lstat(junction);

        // S_ISDIR test (S_IFDIR is 0x4000, S_IFMT is 0xF000 bitmask)
        if let Some(arr) = stat.as_array() {
            let mode = arr.get("mode").and_then(|v| v.as_int()).unwrap_or(0);
            return 0x4000 != (mode & 0xF000);
        }
        false
    }

    /// Removes a Windows NTFS junction.
    pub fn remove_junction(&mut self, junction: &str) -> anyhow::Result<bool> {
        if !Platform::is_windows() {
            return Ok(false);
        }
        let junction = rtrim(
            &str_replace("/", DIRECTORY_SEPARATOR, junction),
            DIRECTORY_SEPARATOR,
        );
        if !self.is_junction(&junction) {
            return Err(IOException::new(
                format!(
                    "{} is not a junction and thus cannot be removed as one",
                    junction
                ),
                0,
                None,
                None,
            )
            .into());
        }

        self.rmdir(&junction)
    }

    pub fn file_put_contents_if_modified(&self, path: &str, content: &str) -> anyhow::Result<i64> {
        let current_content =
            Silencer::call(|| Ok(file_get_contents(path).unwrap_or_default())).unwrap_or_default();
        if current_content.is_empty() || current_content != content {
            return Ok(file_put_contents(path, content) as i64);
        }

        Ok(0)
    }

    /// Copy file using stream_copy_to_stream to work around https://bugs.php.net/bug.php?id=6463
    pub fn safe_copy(&self, source: &str, target: &str) -> anyhow::Result<()> {
        if !file_exists(target) || !file_exists(source) || !self.files_are_equal(source, target) {
            let source_handle = fopen(source, "r")
                .ok_or_else(|| anyhow::anyhow!("Could not open \"{}\" for reading.", source))?;
            let target_handle = fopen(target, "w+")
                .ok_or_else(|| anyhow::anyhow!("Could not open \"{}\" for writing.", target))?;

            shirabe_php_shim::stream_copy_to_stream(&source_handle, &target_handle);
            fclose(&source_handle);
            fclose(&target_handle);

            touch(target);
            // PHP also passes filemtime/fileatime — skipping detailed timestamp restore here.
            let _ = (filemtime(source), fileatime(source));
        }
        Ok(())
    }

    /// compare 2 files
    /// https://stackoverflow.com/questions/3060125/can-i-use-file-get-contents-to-compare-two-files
    fn files_are_equal(&self, a: &str, b: &str) -> bool {
        // Check if filesize is different
        if filesize(a) != filesize(b) {
            return false;
        }

        // Check if content is different
        let a_handle = match fopen(a, "rb") {
            Some(h) => h,
            None => return false,
        };
        let b_handle = match fopen(b, "rb") {
            Some(h) => h,
            None => return false,
        };

        let mut result = true;
        while !feof(&a_handle) {
            if fread(&a_handle, 8192) != fread(&b_handle, 8192) {
                result = false;
                break;
            }
        }

        fclose(&a_handle);
        fclose(&b_handle);

        result
    }
}
