//! ref: composer/vendor/composer/class-map-generator/src/ClassMapGenerator.php sha1:18d9e8606fcfd875c1cfb2e3341840f1a02ea4c5

use crate::class_map::ClassMap;
use crate::file_list::FileList;
use crate::php_file_parser::PhpFileParser;
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_shim::{
    InvalidArgumentException, LogicException, PATHINFO_EXTENSION, RuntimeException, bformat,
    explode, getcwd, implode, is_dir, is_file, path_from_bytes, path_to_bytes, pathinfo, php_regex,
    preg_is_match, preg_match, preg_quote, preg_replace, preg_replace_callback, realpath,
    str_replace, stream_get_wrappers, strlen, strpos, strrpos, strtr, substr,
};
use shirabe_symfony_finder::Finder;
use std::path::PathBuf;

#[derive(Debug)]
pub struct ClassMapGenerator {
    extensions: Vec<BString>,
    scanned_files: Option<FileList>,
    class_map: ClassMap,
    stream_wrappers_regex: BString,
    max_parse_threads: Option<usize>,
}

impl ClassMapGenerator {
    pub fn new(extensions: Vec<BString>) -> Self {
        let wrappers: Vec<BString> = stream_get_wrappers()
            .iter()
            .map(|w| preg_quote(w, None))
            .collect();
        let stream_wrappers_regex = bformat!("{{^(?:{})://}}", implode("|", &wrappers));

        ClassMapGenerator {
            extensions,
            scanned_files: None,
            class_map: ClassMap::new(),
            stream_wrappers_regex,
            max_parse_threads: None,
        }
    }

    pub fn new_default() -> Self {
        Self::new(vec![BString::from("php"), BString::from("inc")])
    }

    /// When calling scanPaths repeatedly with paths that may overlap, calling this will ensure that the same class is never scanned twice
    pub fn avoid_duplicate_scans(&mut self, scanned_files: Option<FileList>) -> &mut Self {
        self.scanned_files = Some(scanned_files.unwrap_or_default());
        self
    }

    /// For testing only. Caps the number of threads `scan_paths()` parses files on; `1` parses
    /// every file sequentially.
    pub fn __set_max_parse_threads(&mut self, max_parse_threads: usize) {
        self.max_parse_threads = Some(max_parse_threads);
    }

    /// Iterate over all files in the given directory searching for classes
    pub fn create_map(
        path: impl AsRef<[u8]>,
    ) -> anyhow::Result<indexmap::IndexMap<BString, BString>> {
        let mut generator = Self::new_default();
        generator.scan_paths(path, None, BStr::new("classmap"), None, vec![])?;
        Ok(generator.get_class_map().get_map().clone())
    }

    pub fn get_class_map(&self) -> &ClassMap {
        &self.class_map
    }

    /// Take ownership of the inner ClassMap, leaving a default in its place.
    pub fn take_class_map(&mut self) -> ClassMap {
        std::mem::take(&mut self.class_map)
    }

    /// Iterate over all files in the given directory searching for classes
    pub fn scan_paths(
        &mut self,
        path: impl AsRef<[u8]>,
        excluded: Option<bstr::BString>,
        autoload_type: &BStr,
        namespace: Option<BString>,
        excluded_dirs: Vec<BString>,
    ) -> anyhow::Result<()> {
        let path = path.as_ref().as_bstr();
        let fs_path = path_from_bytes(path);
        if !matches!(autoload_type.as_bytes(), b"psr-0" | b"psr-4" | b"classmap") {
            return Err(InvalidArgumentException::new(BString::from(
                "$autoloadType must be one of: \"psr-0\", \"psr-4\" or \"classmap\"",
            ))
            .into());
        }

        let base_path: Option<BString> = if autoload_type != "classmap" {
            if namespace.is_none() {
                return Err(InvalidArgumentException::new(BString::from("$namespace must be given (even if it is an empty string if you do not want to filter) when specifying a psr-0 or psr-4 autoload type")).into());
            }
            Some(BString::from(path))
        } else {
            None
        };

        let files: Vec<BString> = if is_file(fs_path) {
            vec![BString::from(path)]
        } else if is_dir(fs_path) || strpos(path, "*").is_some() {
            let ext_pattern = bformat!(
                "/\\.(?:{})$/",
                implode(
                    "|",
                    &self
                        .extensions
                        .iter()
                        .map(|e| preg_quote(e, None))
                        .collect::<Vec<_>>()
                )
            );
            Finder::create()
                .files()
                .follow_links()
                .name(ext_pattern.as_bstr())
                .r#in(fs_path)
                .exclude(&excluded_dirs)
                .iter()
                .map(|file| BString::from(path_to_bytes(&file)))
                .collect()
        } else {
            return Err(RuntimeException::new(bformat!("Could not scan for classes inside \"{path}\" which does not appear to be a file nor a folder"))
            .into());
        };

        let cwd = BString::from(path_to_bytes(
            &realpath(Self::get_cwd()?).unwrap_or_default(),
        ));

        // PHP prepares, parses and records one file at a time. Here the files are prepared in input
        // order, parsed on worker threads, and then recorded in input order, so the class map, the
        // warnings and the first reported error are the same. Preparation stops at the first
        // failure, which is reported once every file before it has been recorded.
        let mut candidates: Vec<(BString, BString)> = vec![];
        let mut preparation_error: Option<anyhow::Error> = None;
        for file in files {
            let mut file_path = file.clone();
            let ext = pathinfo(&file_path, PATHINFO_EXTENSION);
            if !self
                .extensions
                .iter()
                .any(|e| e.as_bytes() == ext.as_slice())
            {
                continue;
            }

            let is_stream_wrapper_path = preg_is_match(&self.stream_wrappers_regex, &file_path);
            if !Self::is_absolute_path(&file_path) && !is_stream_wrapper_path {
                file_path = bformat!("{cwd}/{file_path}");
                file_path = Self::normalize_path(&file_path);
            } else {
                // Regex pattern compatibility:
                // PHP collapses runs of 2+ slashes/backslashes into one, except when the run is
                // immediately preceded by `:` (to preserve scheme separators like `phar://`). The
                // `regex` crate has no look-behind, so the `(?<!:)` guard is turned into a consuming
                // optional leading group `(^|[^:])` that is re-emitted in the replacement. Slash runs
                // are always separated by path-segment characters, so consuming the single preceding
                // char never prevents an adjacent run from matching.
                file_path = preg_replace(php_regex!(r"{(^|[^:])[\\/]{2,}}"), "${1}/", &file_path);
            }

            if file_path.is_empty() {
                preparation_error =
                    Some(LogicException::new(bformat!("Got an empty $filePath for {file}")).into());
                break;
            }

            let real_path = if is_stream_wrapper_path {
                file_path.clone()
            } else {
                match realpath(path_from_bytes(&file_path)) {
                    Some(p) => BString::from(path_to_bytes(&p)),
                    None => {
                        preparation_error = Some(
                            RuntimeException::new(bformat!(
                                "realpath of {file_path} failed to resolve, got false"
                            ))
                            .into(),
                        );
                        break;
                    }
                }
            };

            // if a list of scanned files is given, avoid scanning twice the same file to save cycles and avoid generating warnings
            // in case a PSR-0/4 declaration follows another more specific one, or a classmap declaration, which covered this file already
            if let Some(ref scanned_files) = self.scanned_files
                && scanned_files.contains(&real_path)
            {
                continue;
            }

            // check the realpath of the file against the excluded paths as the path might be a symlink and the excluded path is realpath'd so symlink are resolved
            if let Some(ref excluded) = excluded {
                if preg_is_match(excluded, strtr(&real_path, "\\", "/")) {
                    continue;
                }
                // check non-realpath of file for directories symlink in project dir
                if preg_is_match(excluded, strtr(&file_path, "\\", "/")) {
                    continue;
                }
            }

            candidates.push((file_path, real_path));
        }

        let mut parsed = self.find_classes_in_parallel(&candidates);

        for (index, (file_path, real_path)) in candidates.into_iter().enumerate() {
            // An earlier candidate of this call may have resolved to the same real path.
            if let Some(ref scanned_files) = self.scanned_files
                && scanned_files.contains(&real_path)
            {
                continue;
            }

            let classes = match parsed.get_mut(index).and_then(Option::take) {
                Some(classes) => classes?,
                None => PhpFileParser::find_classes(file_path.as_bstr())?,
            };
            let effective_classes = if autoload_type != "classmap" && namespace.is_some() {
                let filtered = self.filter_by_namespace(
                    classes,
                    file_path.as_bstr(),
                    namespace.as_ref().map_or(BStr::new(""), |n| n.as_bstr()),
                    autoload_type,
                    base_path.as_ref().map_or(BStr::new(""), |p| p.as_bstr()),
                )?;

                // if no valid class was found in the file then we do not mark it as scanned as it might still be matched by another rule later
                if !filtered.is_empty()
                    && let Some(ref mut scanned_files) = self.scanned_files
                {
                    scanned_files.add(real_path);
                }

                filtered
            } else {
                // classmap autoload rules always collect all classes so for these we definitely do not want to scan again
                if let Some(ref mut scanned_files) = self.scanned_files {
                    scanned_files.add(real_path);
                }
                classes
            };

            for class in effective_classes {
                if !self.class_map.has_class(&class) {
                    self.class_map.add_class(class.clone(), file_path.clone());
                } else if file_path != *self.class_map.get_class_path(&class)? {
                    self.class_map.add_ambiguous_class(class, file_path.clone());
                }
            }
        }

        match preparation_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// Runs `PhpFileParser::find_classes()` for the given candidates on worker threads. Entries
    /// are in candidate order; `None` entries are parsed by the caller in its sequential loop.
    fn find_classes_in_parallel(
        &self,
        candidates: &[(BString, BString)],
    ) -> Vec<Option<anyhow::Result<Vec<BString>>>> {
        // TODO(perf): parsing stops scaling at a few threads, and one thread per logical CPU is
        // slower than this cap.
        let thread_count = self
            .max_parse_threads
            .unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map_or(1, std::num::NonZeroUsize::get)
                    .min(4)
            })
            .min(candidates.len());
        // The PHP version lookup behind the extra types talks to the PHP runtime, so it runs on
        // this thread before any worker starts. If it fails, every file is parsed sequentially so
        // that the failure is reported for the first file PHP would parse.
        if thread_count < 2 || PhpFileParser::get_extra_types().is_err() {
            return vec![];
        }

        let next = std::sync::atomic::AtomicUsize::new(0);
        let mut parsed: Vec<Option<anyhow::Result<Vec<BString>>>> = std::iter::repeat_with(|| None)
            .take(candidates.len())
            .collect();
        std::thread::scope(|scope| {
            let workers: Vec<_> = (0..thread_count)
                .map(|_| {
                    scope.spawn(|| {
                        let mut results = vec![];
                        loop {
                            let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            let Some((file_path, _)) = candidates.get(index) else {
                                break;
                            };
                            results.push((index, PhpFileParser::find_classes(file_path.as_bstr())));
                        }
                        results
                    })
                })
                .collect();
            for worker in workers {
                let results = worker
                    .join()
                    .unwrap_or_else(|payload| std::panic::resume_unwind(payload));
                for (index, result) in results {
                    parsed[index] = Some(result);
                }
            }
        });
        parsed
    }

    /// Remove classes which could not have been loaded by namespace autoloaders
    fn filter_by_namespace(
        &mut self,
        classes: Vec<BString>,
        file_path: &BStr,
        base_namespace: &BStr,
        namespace_type: &BStr,
        base_path: &BStr,
    ) -> anyhow::Result<Vec<BString>> {
        let mut valid_classes = vec![];
        let mut rejected_classes = vec![];

        let real_sub_path_str = substr(file_path, strlen(base_path) + 1, None);
        let dot_position = strrpos(&real_sub_path_str, ".");
        let real_sub_path = substr(
            &real_sub_path_str,
            0,
            Some(dot_position.map(|p| p as i64).unwrap_or(i64::MAX)),
        );

        for class in classes {
            let sub_path: BString;

            if namespace_type == "psr-0" {
                if !base_namespace.is_empty() && !class.starts_with(base_namespace) {
                    rejected_classes.push(class);
                    continue;
                }

                let namespace_length = strrpos(&class, "\\");
                if let Some(ns_len) = namespace_length {
                    let namespace = substr(&class, 0, Some((ns_len + 1) as i64));
                    let class_name = substr(&class, (ns_len + 1) as i64, None);
                    let mut path = str_replace("\\", std::path::MAIN_SEPARATOR_STR, &namespace);
                    path.extend_from_slice(&str_replace(
                        "_",
                        std::path::MAIN_SEPARATOR_STR,
                        &class_name,
                    ));
                    sub_path = path;
                } else {
                    sub_path = str_replace("_", std::path::MAIN_SEPARATOR_STR, &class);
                }
            } else if namespace_type == "psr-4" {
                let sub_namespace = if !base_namespace.is_empty() {
                    substr(&class, strlen(base_namespace), None)
                } else {
                    class.clone()
                };
                sub_path = str_replace("\\", std::path::MAIN_SEPARATOR_STR, &sub_namespace);
            } else {
                return Err(InvalidArgumentException::new(BString::from(
                    "$namespaceType must be \"psr-0\" or \"psr-4\"",
                ))
                .into());
            }

            if sub_path == real_sub_path {
                valid_classes.push(class);
            } else {
                rejected_classes.push(class);
            }
        }

        // warn only if no valid classes, else silently skip invalid
        if valid_classes.is_empty() {
            let cwd_path = Self::get_cwd()?;
            let cwd = realpath(&cwd_path);
            let cwd = Self::normalize_path(path_to_bytes(&cwd.unwrap_or(cwd_path)));
            let short_path = preg_replace(
                bformat!("{{^{}}}", preg_quote(&cwd, None)),
                ".",
                Self::normalize_path(file_path),
            );
            let short_base_path = preg_replace(
                bformat!("{{^{}}}", preg_quote(&cwd, None)),
                ".",
                Self::normalize_path(base_path),
            );

            for class in rejected_classes {
                self.class_map.add_psr_violation(
                    bformat!("Class {class} located in {short_path} does not comply with {namespace_type} autoloading standard (rule: {base_namespace} => {short_base_path}). Skipping."),
                    class.clone(),
                    file_path,
                );
            }

            return Ok(vec![]);
        }

        Ok(valid_classes)
    }

    /// Checks if the given path is absolute
    fn is_absolute_path(path: &[u8]) -> bool {
        // std::path::Path::is_absolute() is environment-dependent, while PHP's
        // ClassMapGenerator::isAbsolutePath() returns the same result in any platforms.
        path.starts_with(b"/") || path.get(1) == Some(&b':') || path.starts_with(b"\\\\")
    }

    /// Normalize a path. This replaces backslashes with slashes, removes ending
    /// slash and collapses redundant separators and up-level references.
    fn normalize_path(path: impl AsRef<[u8]>) -> BString {
        let mut parts: Vec<BString> = vec![];
        let mut path = strtr(path, "\\", "/");
        let mut prefix = BString::default();
        let mut absolute = "";

        // extract windows UNC paths e.g. \\foo\bar
        if strpos(&path, "//") == Some(0) && strlen(&path) > 2 {
            absolute = "//";
            path = substr(&path, 2, None);
        }

        // extract a prefix being a protocol://, protocol:, protocol://drive: or simply drive:
        if let Some(r#match) = preg_match(
            php_regex!(r"{^( [0-9a-z]{2,}+: (?: // (?: [a-z]: )? )? | [a-z]: )}ix"),
            &path,
        ) {
            prefix = BString::from(r#match.get(1).unwrap_or_default());
            path = substr(&path, strlen(&prefix), None);
        }

        if strpos(&path, "/") == Some(0) {
            absolute = "/";
            path = substr(&path, 1, None);
        }

        let mut up = false;
        for chunk in explode("/", &path) {
            if chunk == ".." && (!absolute.is_empty() || up) {
                parts.pop();
                up = !(parts.is_empty() || parts.last().is_some_and(|s| s == ".."));
            } else if chunk != "." && !chunk.is_empty() {
                parts.push(chunk.clone());
                up = chunk != "..";
            }
        }

        // ensure c: is normalized to C:
        let prefix = preg_replace_callback(
            php_regex!(r"{(?:^|://)[a-z]:$}i"),
            |m| Ok(m.get(0).unwrap_or_default().to_ascii_uppercase()),
            &prefix,
        )
        .expect("the replacement callback cannot fail");

        bformat!("{prefix}{absolute}{}", bstr::join("/", &parts))
    }

    fn get_cwd() -> anyhow::Result<PathBuf> {
        getcwd().ok_or_else(|| {
            RuntimeException::new(BString::from(
                "Could not determine the current working directory",
            ))
            .into()
        })
    }
}
