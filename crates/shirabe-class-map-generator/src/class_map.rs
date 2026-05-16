//! ref: composer/vendor/composer/class-map-generator/src/ClassMap.php

use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{
    Countable, InvalidArgumentException, OutOfBoundsException, rtrim, strpos, strtr,
};

#[derive(Debug, Clone)]
pub struct PsrViolationEntry {
    pub warning: String,
    pub class_name: String,
}

#[derive(Debug)]
pub struct ClassMap {
    pub map: IndexMap<String, String>,
    ambiguous_classes: IndexMap<String, Vec<String>>,
    psr_violations: IndexMap<String, Vec<PsrViolationEntry>>,
}

impl ClassMap {
    pub fn new() -> Self {
        ClassMap {
            map: IndexMap::new(),
            ambiguous_classes: IndexMap::new(),
            psr_violations: IndexMap::new(),
        }
    }

    /// Returns the class map, which is a list of paths indexed by class name
    pub fn get_map(&self) -> &IndexMap<String, String> {
        &self.map
    }

    /// Returns warning strings containing details about PSR-0/4 violations that were detected
    pub fn get_psr_violations(&self) -> Vec<String> {
        if self.psr_violations.is_empty() {
            return vec![];
        }

        self.psr_violations
            .values()
            .flatten()
            .map(|violation| violation.warning.clone())
            .collect()
    }

    /// A map of class names to their list of ambiguous paths
    ///
    /// Pass `None` for `duplicates_filter` to disable filtering (equivalent to PHP's `false`).
    /// Pass `Some(pattern)` for a regex pattern to filter out matching paths.
    pub fn get_ambiguous_classes(
        &self,
        duplicates_filter: Option<&str>,
    ) -> anyhow::Result<IndexMap<String, Vec<String>>> {
        let duplicates_filter = match duplicates_filter {
            None => return Ok(self.ambiguous_classes.clone()),
            Some(pattern) => pattern,
        };

        let mut ambiguous_classes: IndexMap<String, Vec<String>> = IndexMap::new();
        for (class, paths) in &self.ambiguous_classes {
            let paths: Vec<String> = paths
                .iter()
                .filter(|path| {
                    !Preg::is_match(duplicates_filter, &strtr(path, "\\", "/")).unwrap_or(false)
                })
                .cloned()
                .collect();
            if !paths.is_empty() {
                ambiguous_classes.insert(class.clone(), paths);
            }
        }

        Ok(ambiguous_classes)
    }

    /// Sorts the class map alphabetically by class names
    pub fn sort(&mut self) {
        self.map.sort_keys();
    }

    pub fn add_class(&mut self, class_name: String, path: String) {
        self.psr_violations.remove(&strtr(&path, "\\", "/"));

        self.map.insert(class_name, path);
    }

    pub fn get_class_path(&self, class_name: &str) -> anyhow::Result<&str> {
        match self.map.get(class_name) {
            Some(path) => Ok(path.as_str()),
            None => Err(anyhow::anyhow!(OutOfBoundsException {
                message: format!("Class {} is not present in the map", class_name),
                code: 0,
            })),
        }
    }

    pub fn has_class(&self, class_name: &str) -> bool {
        self.map.contains_key(class_name)
    }

    pub fn add_psr_violation(&mut self, warning: String, class_name: String, path: String) {
        let path = rtrim(&strtr(&path, "\\", "/"), Some("/"));

        self.psr_violations
            .entry(path)
            .or_default()
            .push(PsrViolationEntry {
                warning,
                class_name,
            });
    }

    pub fn clear_psr_violations_by_path(&mut self, path_prefix: &str) {
        let path_prefix = rtrim(&strtr(path_prefix, "\\", "/"), Some("/"));

        self.psr_violations.retain(|path, _| {
            path != &path_prefix && strpos(path, &format!("{}/", path_prefix)) != Some(0)
        });
    }

    pub fn add_ambiguous_class(&mut self, class_name: String, path: String) {
        self.ambiguous_classes
            .entry(class_name)
            .or_default()
            .push(path);
    }

    /// Get the raw psr violations
    pub fn get_raw_psr_violations(&self) -> &IndexMap<String, Vec<PsrViolationEntry>> {
        &self.psr_violations
    }
}

impl Countable for ClassMap {
    fn count(&self) -> i64 {
        self.map.len() as i64
    }
}
