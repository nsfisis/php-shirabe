//! ref: composer/vendor/composer/class-map-generator/src/ClassMapGenerator.php

use crate::class_map::ClassMap;
use crate::file_list::FileList;
use crate::php_file_parser::PhpFileParser;
use indexmap::indexmap;
use shirabe_external_packages::composer::pcre::preg::{CaptureKey, Preg};
use shirabe_external_packages::symfony::component::finder::finder::Finder;
use shirabe_external_packages::symfony::component::finder::spl_file_info::SplFileInfo;
use shirabe_php_shim::{
    DIRECTORY_SEPARATOR, InvalidArgumentException, LogicException, PATHINFO_EXTENSION, PHP_INT_MAX,
    PhpMixed, RuntimeException, explode, getcwd, implode, in_array, is_dir, is_file, is_string,
    pathinfo, preg_quote, realpath, sprintf, str_replace, str_starts_with, stream_get_wrappers,
    strlen, strpos, strrpos, strtr, substr,
};

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
        let stream_wrappers_regex =
            sprintf("{^(?:%s)://}", &[PhpMixed::String(implode("|", &wrappers))]);

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
        self.scanned_files = Some(scanned_files.unwrap_or_else(FileList::new));
        self
    }

    /// Iterate over all files in the given directory searching for classes
    pub fn create_map(path: PhpMixed) -> anyhow::Result<indexmap::IndexMap<String, String>> {
        let mut generator = Self::new_default();
        generator.scan_paths(path, None, "classmap", None, vec![])?;
        Ok(generator.get_class_map().get_map().clone())
    }

    pub fn get_class_map(&self) -> &ClassMap {
        &self.class_map
    }

    /// Iterate over all files in the given directory searching for classes
    pub fn scan_paths(
        &mut self,
        path: PhpMixed,
        excluded: Option<String>,
        autoload_type: &str,
        namespace: Option<String>,
        excluded_dirs: Vec<String>,
    ) -> anyhow::Result<()> {
        if !in_array(
            PhpMixed::String(autoload_type.to_string()),
            &PhpMixed::List(vec![
                Box::new(PhpMixed::String("psr-0".to_string())),
                Box::new(PhpMixed::String("psr-4".to_string())),
                Box::new(PhpMixed::String("classmap".to_string())),
            ]),
            true,
        ) {
            return Err(anyhow::anyhow!(InvalidArgumentException {
                message: "$autoloadType must be one of: \"psr-0\", \"psr-4\" or \"classmap\""
                    .to_string(),
                code: 0,
            }));
        }

        let base_path: Option<String>;
        if autoload_type != "classmap" {
            if !is_string(&path) {
                return Err(anyhow::anyhow!(InvalidArgumentException {
                    message:
                        "$path must be a string when specifying a psr-0 or psr-4 autoload type"
                            .to_string(),
                    code: 0,
                }));
            }
            if namespace.is_none() {
                return Err(anyhow::anyhow!(InvalidArgumentException {
                    message: "$namespace must be given (even if it is an empty string if you do not want to filter) when specifying a psr-0 or psr-4 autoload type".to_string(),
                    code: 0,
                }));
            }
            base_path = path.as_string().map(|s| s.to_string());
        } else {
            base_path = None;
        }

        let files: Vec<SplFileInfo> = if is_string(&path) {
            let path_str = path.as_string().unwrap_or("");
            if is_file(path_str) {
                vec![SplFileInfo::new(path_str)]
            } else if is_dir(path_str) || strpos(path_str, "*").is_some() {
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
                    .r#in(path_str)
                    .exclude(&excluded_dirs)
                    .iter()
                    .collect()
            } else {
                return Err(anyhow::anyhow!(RuntimeException {
                    message: format!(
                        "Could not scan for classes inside \"{}\" which does not appear to be a file nor a folder",
                        path_str
                    ),
                    code: 0,
                }));
            }
        } else {
            // $path is already an array or Traversable of SplFileInfo
            todo!(
                "non-string path (Traversable/array of SplFileInfo) is not yet handled in Phase A"
            )
        };

        let cwd = realpath(&getcwd().unwrap_or_default()).unwrap_or_default();

        for file in files {
            let mut file_path = file.get_pathname();
            let ext = pathinfo(PhpMixed::String(file_path.clone()), PATHINFO_EXTENSION);
            if !in_array(
                ext,
                &PhpMixed::List(
                    self.extensions
                        .iter()
                        .map(|e| Box::new(PhpMixed::String(e.clone())))
                        .collect(),
                ),
                true,
            ) {
                continue;
            }

            let is_stream_wrapper_path =
                Preg::is_match(&self.stream_wrappers_regex, &file_path).unwrap_or(false);
            if !Self::is_absolute_path(&file_path) && !is_stream_wrapper_path {
                file_path = format!("{}/{}", cwd, file_path);
                file_path = Self::normalize_path(&file_path);
            } else {
                file_path =
                    Preg::replace(r"{(?<!:)[\\/]{2,}}", "/", &file_path).unwrap_or(file_path);
            }

            if file_path.is_empty() {
                return Err(anyhow::anyhow!(LogicException {
                    message: format!("Got an empty $filePath for {}", file.get_pathname()),
                    code: 0,
                }));
            }

            let real_path = if is_stream_wrapper_path {
                file_path.clone()
            } else {
                match realpath(&file_path) {
                    Some(p) => p,
                    None => {
                        return Err(anyhow::anyhow!(RuntimeException {
                            message: format!(
                                "realpath of {} failed to resolve, got false",
                                file_path
                            ),
                            code: 0,
                        }));
                    }
                }
            };

            // if a list of scanned files is given, avoid scanning twice the same file to save cycles and avoid generating warnings
            // in case a PSR-0/4 declaration follows another more specific one, or a classmap declaration, which covered this file already
            if let Some(ref scanned_files) = self.scanned_files {
                if scanned_files.contains(&real_path) {
                    continue;
                }
            }

            // check the realpath of the file against the excluded paths as the path might be a symlink and the excluded path is realpath'd so symlink are resolved
            if let Some(ref excluded) = excluded {
                if Preg::is_match(excluded, &strtr(&real_path, "\\", "/")).unwrap_or(false) {
                    continue;
                }
                // check non-realpath of file for directories symlink in project dir
                if Preg::is_match(excluded, &strtr(&file_path, "\\", "/")).unwrap_or(false) {
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
                if !filtered.is_empty() {
                    if let Some(ref mut scanned_files) = self.scanned_files {
                        scanned_files.add(real_path);
                    }
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

        let real_sub_path_str = substr(file_path, (strlen(base_path) + 1) as i64, None);
        let dot_position = strrpos(&real_sub_path_str, ".");
        let real_sub_path = substr(
            &real_sub_path_str,
            0,
            Some(dot_position.map(|p| p as i64).unwrap_or(PHP_INT_MAX)),
        );

        for class in classes {
            let sub_path: String;

            if namespace_type == "psr-0" {
                if !base_namespace.is_empty() && !str_starts_with(&class, base_namespace) {
                    rejected_classes.push(class);
                    continue;
                }

                let namespace_length = strrpos(&class, "\\");
                if let Some(ns_len) = namespace_length {
                    let namespace = substr(&class, 0, Some((ns_len + 1) as i64));
                    let class_name = substr(&class, (ns_len + 1) as i64, None);
                    sub_path = str_replace("\\", DIRECTORY_SEPARATOR, &namespace)
                        + &str_replace("_", DIRECTORY_SEPARATOR, &class_name);
                } else {
                    sub_path = str_replace("_", DIRECTORY_SEPARATOR, &class);
                }
            } else if namespace_type == "psr-4" {
                let sub_namespace = if !base_namespace.is_empty() {
                    substr(&class, strlen(base_namespace) as i64, None)
                } else {
                    class.clone()
                };
                sub_path = str_replace("\\", DIRECTORY_SEPARATOR, &sub_namespace);
            } else {
                return Err(anyhow::anyhow!(InvalidArgumentException {
                    message: "$namespaceType must be \"psr-0\" or \"psr-4\"".to_string(),
                    code: 0,
                }));
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
                &format!("{{^{}}}", preg_quote(&cwd, None)),
                ".",
                &Self::normalize_path(file_path),
            )
            .unwrap_or_else(|_| Self::normalize_path(file_path));
            let short_base_path = Preg::replace(
                &format!("{{^{}}}", preg_quote(&cwd, None)),
                ".",
                &Self::normalize_path(base_path),
            )
            .unwrap_or_else(|_| Self::normalize_path(base_path));

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
        let mut r#match: indexmap::IndexMap<_, _> = indexmap![];
        if Preg::is_match_strict_groups3(
            r"{^( [0-9a-z]{2,}+: (?: // (?: [a-z]: )? )? | [a-z]: )}ix",
            &path,
            Some(&mut r#match),
        )
        .unwrap_or(false)
        {
            prefix = r#match
                .get(&CaptureKey::ByIndex(1))
                .cloned()
                .unwrap_or_default();
            path = substr(&path, strlen(&prefix) as i64, None);
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
            r"{(?:^|://)[a-z]:$}i",
            |m| {
                m.get(&CaptureKey::ByIndex(0))
                    .cloned()
                    .unwrap_or_default()
                    .to_uppercase()
            },
            &prefix,
        )
        .unwrap_or(prefix);

        format!("{}{}{}", prefix, absolute, parts.join("/"))
    }

    fn get_cwd() -> anyhow::Result<String> {
        match getcwd() {
            Some(cwd) => Ok(cwd),
            None => Err(anyhow::anyhow!(RuntimeException {
                message: "Could not determine the current working directory".to_string(),
                code: 0,
            })),
        }
    }
}
