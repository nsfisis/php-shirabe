//! ref: composer/vendor/composer/class-map-generator/src/ClassMapGenerator.php

use crate::class_map::ClassMap;
use crate::file_list::FileList;
use crate::php_file_parser::PhpFileParser;
use shirabe_pcre::{CaptureKey, Preg};
use shirabe_php_shim::{
    InvalidArgumentException, LogicException, PATHINFO_EXTENSION, RuntimeException, explode,
    getcwd, implode, is_dir, is_file, pathinfo, php_regex, preg_quote, realpath, str_replace,
    stream_get_wrappers, strlen, strpos, strrpos, strtr, substr,
};
use shirabe_symfony_finder::Finder;
use std::path::PathBuf;

#[derive(Debug)]
pub struct ClassMapGenerator {
    extensions: Vec<String>,
    scanned_files: Option<FileList>,
    class_map: ClassMap,
    stream_wrappers_regex: String,
}

impl ClassMapGenerator {
    pub fn new(extensions: Vec<String>) -> Self {
        let wrappers: Vec<String> = stream_get_wrappers()
            .iter()
            .map(|w| preg_quote(w, None))
            .collect();
        let stream_wrappers_regex = format!("{{^(?:{})://}}", implode("|", &wrappers));

        ClassMapGenerator {
            extensions,
            scanned_files: None,
            class_map: ClassMap::new(),
            stream_wrappers_regex,
        }
    }

    pub fn new_default() -> Self {
        Self::new(vec!["php".to_string(), "inc".to_string()])
    }

    /// When calling scanPaths repeatedly with paths that may overlap, calling this will ensure that the same class is never scanned twice
    pub fn avoid_duplicate_scans(&mut self, scanned_files: Option<FileList>) -> &mut Self {
        self.scanned_files = Some(scanned_files.unwrap_or_default());
        self
    }

    /// Iterate over all files in the given directory searching for classes
    pub fn create_map(path: &str) -> anyhow::Result<indexmap::IndexMap<String, String>> {
        let mut generator = Self::new_default();
        generator.scan_paths(path, None, "classmap", None, vec![])?;
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
        path: &str,
        excluded: Option<String>,
        autoload_type: &str,
        namespace: Option<String>,
        excluded_dirs: Vec<String>,
    ) -> anyhow::Result<()> {
        if !matches!(autoload_type, "psr-0" | "psr-4" | "classmap") {
            return Err(InvalidArgumentException::new(
                "$autoloadType must be one of: \"psr-0\", \"psr-4\" or \"classmap\"".to_string(),
            )
            .into());
        }

        let base_path: Option<String> = if autoload_type != "classmap" {
            if namespace.is_none() {
                return Err(InvalidArgumentException::new("$namespace must be given (even if it is an empty string if you do not want to filter) when specifying a psr-0 or psr-4 autoload type".to_string()).into());
            }
            Some(path.to_owned())
        } else {
            None
        };

        let files: Vec<PathBuf> = if is_file(path) {
            vec![PathBuf::from(path)]
        } else if is_dir(path) || strpos(path, "*").is_some() {
            let ext_pattern = format!(
                "/\\.(?:{})$/",
                implode(
                    "|",
                    &self
                        .extensions
                        .iter()
                        .map(|e| preg_quote(e, None))
                        .collect::<Vec<_>>(),
                )
            );
            Finder::create()
                .files()
                .follow_links()
                .name(&ext_pattern)
                .r#in(path)
                .exclude(&excluded_dirs)
                .iter()
                .collect()
        } else {
            return Err(RuntimeException::new(format!(
                "Could not scan for classes inside \"{}\" which does not appear to be a file nor a folder",
                path
            )).into());
        };

        let cwd = realpath(getcwd().unwrap_or_default()).unwrap_or_default();

        for file in files {
            let mut file_path = match file.to_str() {
                Some(s) => s.to_string(),
                None => {
                    return Err(RuntimeException::new(format!(
                        "Path contains invalid UTF-8: {}",
                        file.display()
                    ))
                    .into());
                }
            };
            let ext = pathinfo(&file_path, PATHINFO_EXTENSION);
            if !self.extensions.contains(&ext) {
                continue;
            }

            let is_stream_wrapper_path = Preg::is_match(&self.stream_wrappers_regex, &file_path);
            if !Self::is_absolute_path(&file_path) && !is_stream_wrapper_path {
                file_path = format!("{}/{}", cwd, file_path);
                file_path = Self::normalize_path(&file_path);
            } else {
                // Regex pattern compatibility:
                // PHP collapses runs of 2+ slashes/backslashes into one, except when the run is
                // immediately preceded by `:` (to preserve scheme separators like `phar://`). The
                // `regex` crate has no look-behind, so the `(?<!:)` guard is turned into a consuming
                // optional leading group `(^|[^:])` that is re-emitted in the replacement. Slash runs
                // are always separated by path-segment characters, so consuming the single preceding
                // char never prevents an adjacent run from matching.
                file_path = Preg::replace(php_regex!(r"{(^|[^:])[\\/]{2,}}"), "${1}/", &file_path);
            }

            if file_path.is_empty() {
                return Err(LogicException::new(format!(
                    "Got an empty $filePath for {}",
                    file.display()
                ))
                .into());
            }

            let real_path = if is_stream_wrapper_path {
                file_path.clone()
            } else {
                match realpath(&file_path) {
                    Some(p) => p,
                    None => {
                        return Err(RuntimeException::new(format!(
                            "realpath of {} failed to resolve, got false",
                            file_path
                        ))
                        .into());
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
                if Preg::is_match(excluded, &strtr(&real_path, "\\", "/")) {
                    continue;
                }
                // check non-realpath of file for directories symlink in project dir
                if Preg::is_match(excluded, &strtr(&file_path, "\\", "/")) {
                    continue;
                }
            }

            let classes = PhpFileParser::find_classes(&file_path)?;
            let effective_classes = if autoload_type != "classmap" && namespace.is_some() {
                let filtered = self.filter_by_namespace(
                    classes,
                    &file_path,
                    namespace.as_deref().unwrap_or(""),
                    autoload_type,
                    base_path.as_deref().unwrap_or(""),
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
                } else if file_path != self.class_map.get_class_path(&class)? {
                    self.class_map.add_ambiguous_class(class, file_path.clone());
                }
            }
        }

        Ok(())
    }

    /// Remove classes which could not have been loaded by namespace autoloaders
    fn filter_by_namespace(
        &mut self,
        classes: Vec<String>,
        file_path: &str,
        base_namespace: &str,
        namespace_type: &str,
        base_path: &str,
    ) -> anyhow::Result<Vec<String>> {
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
            let sub_path: String;

            if namespace_type == "psr-0" {
                if !base_namespace.is_empty() && !class.starts_with(base_namespace) {
                    rejected_classes.push(class);
                    continue;
                }

                let namespace_length = strrpos(&class, "\\");
                if let Some(ns_len) = namespace_length {
                    let namespace = substr(&class, 0, Some((ns_len + 1) as i64));
                    let class_name = substr(&class, (ns_len + 1) as i64, None);
                    sub_path = str_replace("\\", std::path::MAIN_SEPARATOR_STR, &namespace)
                        + &str_replace("_", std::path::MAIN_SEPARATOR_STR, &class_name);
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
                return Err(InvalidArgumentException::new(
                    "$namespaceType must be \"psr-0\" or \"psr-4\"".to_string(),
                )
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
            let cwd_str = Self::get_cwd()?;
            let cwd = realpath(&cwd_str);
            let cwd = match cwd {
                Some(c) => c,
                None => cwd_str,
            };
            let cwd = Self::normalize_path(&cwd);
            let short_path = Preg::replace(
                format!("{{^{}}}", preg_quote(&cwd, None)),
                ".",
                &Self::normalize_path(file_path),
            );
            let short_base_path = Preg::replace(
                format!("{{^{}}}", preg_quote(&cwd, None)),
                ".",
                &Self::normalize_path(base_path),
            );

            for class in rejected_classes {
                self.class_map.add_psr_violation(
                    format!(
                        "Class {} located in {} does not comply with {} autoloading standard (rule: {} => {}). Skipping.",
                        class, short_path, namespace_type, base_namespace, short_base_path
                    ),
                    class.clone(),
                    file_path.to_string(),
                );
            }

            return Ok(vec![]);
        }

        Ok(valid_classes)
    }

    /// Checks if the given path is absolute
    fn is_absolute_path(path: &str) -> bool {
        strpos(path, "/") == Some(0)
            || substr(path, 1, Some(1)) == ":"
            || strpos(path, "\\\\") == Some(0)
    }

    /// Normalize a path. This replaces backslashes with slashes, removes ending
    /// slash and collapses redundant separators and up-level references.
    fn normalize_path(path: &str) -> String {
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
        if let Some(r#match) = Preg::is_match3(
            php_regex!(r"{^( [0-9a-z]{2,}+: (?: // (?: [a-z]: )? )? | [a-z]: )}ix"),
            &path,
        ) {
            prefix = r#match
                .get(&CaptureKey::ByIndex(1))
                .unwrap_or_default()
                .to_string();
            path = substr(&path, strlen(&prefix), None);
        }

        if strpos(&path, "/") == Some(0) {
            absolute = "/".to_string();
            path = substr(&path, 1, None);
        }

        let mut up = false;
        for chunk in explode("/", &path) {
            if chunk == ".." && (!absolute.is_empty() || up) {
                parts.pop();
                up = !(parts.is_empty() || parts.last().map(|s| s.as_str()) == Some(".."));
            } else if chunk != "." && !chunk.is_empty() {
                parts.push(chunk.clone());
                up = chunk != "..";
            }
        }

        // ensure c: is normalized to C:
        let prefix = Preg::replace_callback(
            php_regex!(r"{(?:^|://)[a-z]:$}i"),
            |m| {
                m.get(&CaptureKey::ByIndex(0))
                    .unwrap_or_default()
                    .to_string()
                    .to_uppercase()
            },
            &prefix,
        );

        format!("{}{}{}", prefix, absolute, parts.join("/"))
    }

    fn get_cwd() -> anyhow::Result<String> {
        match getcwd() {
            Some(cwd) => Ok(cwd),
            None => Err(RuntimeException::new(
                "Could not determine the current working directory".to_string(),
            )
            .into()),
        }
    }
}
