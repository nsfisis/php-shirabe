//! ref: composer/src/Composer/Autoload/ClassLoader.php

use indexmap::IndexMap;
use std::sync::{LazyLock, Mutex};

use shirabe_php_shim::{
    DIRECTORY_SEPARATOR, FILTER_VALIDATE_BOOLEAN, InvalidArgumentException, PhpMixed, apcu_add,
    apcu_fetch, array_merge, array_values, call_user_func_array, defined, file_exists, filter_var,
    function_exists, include_file, ini_get, spl_autoload_register, spl_autoload_unregister,
    stream_resolve_include_path, strlen, strpos, strrpos, strtr, substr,
};

/// @var array<string, self>
static REGISTERED_LOADERS: LazyLock<Mutex<IndexMap<String, ClassLoader>>> =
    LazyLock::new(|| Mutex::new(IndexMap::new()));

/// ClassLoader implements a PSR-0, PSR-4 and classmap class loader.
#[derive(Debug, Clone)]
pub struct ClassLoader {
    // PHP: private static $includeFile — TODO(phase-b): stash include closure as a static
    /// @var string|null
    vendor_dir: Option<String>,

    // PSR-4
    /// @var array<string, array<string, int>>
    prefix_lengths_psr4: IndexMap<String, IndexMap<String, i64>>,
    /// @var array<string, list<string>>
    prefix_dirs_psr4: IndexMap<String, Vec<String>>,
    /// @var list<string>
    fallback_dirs_psr4: Vec<String>,

    // PSR-0
    /// List of PSR-0 prefixes
    ///
    /// Structured as array('F (first letter)' => array('Foo\Bar (full prefix)' => array('path', 'path2')))
    ///
    /// @var array<string, array<string, list<string>>>
    prefixes_psr0: IndexMap<String, IndexMap<String, Vec<String>>>,
    /// @var list<string>
    fallback_dirs_psr0: Vec<String>,

    /// @var bool
    use_include_path: bool,

    /// @var array<string, string>
    class_map: IndexMap<String, String>,

    /// @var bool
    class_map_authoritative: bool,

    /// @var array<string, bool>
    missing_classes: IndexMap<String, bool>,

    /// @var string|null
    apcu_prefix: Option<String>,
}

impl ClassLoader {
    /// @param string|null $vendorDir
    pub fn new(vendor_dir: Option<String>) -> Self {
        let this = Self {
            vendor_dir,
            prefix_lengths_psr4: IndexMap::new(),
            prefix_dirs_psr4: IndexMap::new(),
            fallback_dirs_psr4: vec![],
            prefixes_psr0: IndexMap::new(),
            fallback_dirs_psr0: vec![],
            use_include_path: false,
            class_map: IndexMap::new(),
            class_map_authoritative: false,
            missing_classes: IndexMap::new(),
            apcu_prefix: None,
        };
        Self::initialize_include_closure();
        this
    }

    /// @return array<string, list<string>>
    pub fn get_prefixes(&self) -> IndexMap<String, Vec<String>> {
        if !self.prefixes_psr0.is_empty() {
            // PHP: call_user_func_array('array_merge', array_values($this->prefixesPsr0))
            let prefixes_as_mixed: IndexMap<String, PhpMixed> = self
                .prefixes_psr0
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        PhpMixed::Array(
                            v.iter()
                                .map(|(k2, v2)| {
                                    (
                                        k2.clone(),
                                        Box::new(PhpMixed::List(
                                            v2.iter()
                                                .map(|s| Box::new(PhpMixed::String(s.clone())))
                                                .collect(),
                                        )),
                                    )
                                })
                                .collect(),
                        ),
                    )
                })
                .collect();
            let arrays = array_values(&prefixes_as_mixed);
            let result = call_user_func_array(
                "array_merge",
                &PhpMixed::List(arrays.into_iter().map(Box::new).collect()),
            );
            // TODO(phase-b): cast result back to IndexMap<String, Vec<String>>
            let _ = result;
            return IndexMap::new();
        }

        IndexMap::new()
    }

    /// @return array<string, list<string>>
    pub fn get_prefixes_psr4(&self) -> &IndexMap<String, Vec<String>> {
        &self.prefix_dirs_psr4
    }

    /// @return list<string>
    pub fn get_fallback_dirs(&self) -> &Vec<String> {
        &self.fallback_dirs_psr0
    }

    /// @return list<string>
    pub fn get_fallback_dirs_psr4(&self) -> &Vec<String> {
        &self.fallback_dirs_psr4
    }

    /// @return array<string, string> Array of classname => path
    pub fn get_class_map(&self) -> &IndexMap<String, String> {
        &self.class_map
    }

    /// @param array<string, string> $classMap Class to filename map
    pub fn add_class_map(&mut self, class_map: IndexMap<String, String>) {
        if !self.class_map.is_empty() {
            // PHP: $this->classMap = array_merge($this->classMap, $classMap);
            let merged = array_merge(
                PhpMixed::Array(
                    self.class_map
                        .iter()
                        .map(|(k, v)| (k.clone(), Box::new(PhpMixed::String(v.clone()))))
                        .collect(),
                ),
                PhpMixed::Array(
                    class_map
                        .iter()
                        .map(|(k, v)| (k.clone(), Box::new(PhpMixed::String(v.clone()))))
                        .collect(),
                ),
            );
            // TODO(phase-b): cast merged back to IndexMap<String, String>
            let _ = merged;
            self.class_map.extend(class_map);
        } else {
            self.class_map = class_map;
        }
    }

    /// Registers a set of PSR-0 directories for a given prefix, either
    /// appending or prepending to the ones previously set for this prefix.
    pub fn add(&mut self, prefix: &str, paths: Vec<String>, prepend: bool) {
        if prefix.is_empty() {
            if prepend {
                let mut new_dirs = paths.clone();
                new_dirs.extend(self.fallback_dirs_psr0.drain(..));
                self.fallback_dirs_psr0 = new_dirs;
            } else {
                self.fallback_dirs_psr0.extend(paths);
            }

            return;
        }

        let first = prefix.chars().next().unwrap_or('\0').to_string();
        let entry = self
            .prefixes_psr0
            .entry(first.clone())
            .or_insert_with(IndexMap::new);
        if !entry.contains_key(prefix) {
            entry.insert(prefix.to_string(), paths);
            return;
        }
        let existing = entry.get_mut(prefix).unwrap();
        if prepend {
            let mut new_dirs = paths.clone();
            new_dirs.extend(existing.drain(..));
            *existing = new_dirs;
        } else {
            existing.extend(paths);
        }
    }

    /// Registers a set of PSR-4 directories for a given namespace, either
    /// appending or prepending to the ones previously set for this namespace.
    ///
    /// @throws \InvalidArgumentException
    pub fn add_psr4(
        &mut self,
        prefix: &str,
        paths: Vec<String>,
        prepend: bool,
    ) -> anyhow::Result<()> {
        if prefix.is_empty() {
            // Register directories for the root namespace.
            if prepend {
                let mut new_dirs = paths.clone();
                new_dirs.extend(self.fallback_dirs_psr4.drain(..));
                self.fallback_dirs_psr4 = new_dirs;
            } else {
                self.fallback_dirs_psr4.extend(paths);
            }
        } else if !self.prefix_dirs_psr4.contains_key(prefix) {
            // Register directories for a new namespace.
            let length = strlen(prefix);
            if "\\" != &prefix[(length as usize - 1)..(length as usize)] {
                return Err(InvalidArgumentException {
                    message: "A non-empty PSR-4 prefix must end with a namespace separator."
                        .to_string(),
                    code: 0,
                }
                .into());
            }
            let first = prefix.chars().next().unwrap_or('\0').to_string();
            self.prefix_lengths_psr4
                .entry(first)
                .or_insert_with(IndexMap::new)
                .insert(prefix.to_string(), length);
            self.prefix_dirs_psr4.insert(prefix.to_string(), paths);
        } else if prepend {
            // Prepend directories for an already registered namespace.
            let existing = self.prefix_dirs_psr4.get_mut(prefix).unwrap();
            let mut new_dirs = paths.clone();
            new_dirs.extend(existing.drain(..));
            *existing = new_dirs;
        } else {
            // Append directories for an already registered namespace.
            let existing = self.prefix_dirs_psr4.get_mut(prefix).unwrap();
            existing.extend(paths);
        }
        Ok(())
    }

    /// Registers a set of PSR-0 directories for a given prefix,
    /// replacing any others previously set for this prefix.
    pub fn set(&mut self, prefix: &str, paths: Vec<String>) {
        if prefix.is_empty() {
            self.fallback_dirs_psr0 = paths;
        } else {
            let first = prefix.chars().next().unwrap_or('\0').to_string();
            self.prefixes_psr0
                .entry(first)
                .or_insert_with(IndexMap::new)
                .insert(prefix.to_string(), paths);
        }
    }

    /// Registers a set of PSR-4 directories for a given namespace,
    /// replacing any others previously set for this namespace.
    ///
    /// @throws \InvalidArgumentException
    pub fn set_psr4(&mut self, prefix: &str, paths: Vec<String>) -> anyhow::Result<()> {
        if prefix.is_empty() {
            self.fallback_dirs_psr4 = paths;
        } else {
            let length = strlen(prefix);
            if "\\" != &prefix[(length as usize - 1)..(length as usize)] {
                return Err(InvalidArgumentException {
                    message: "A non-empty PSR-4 prefix must end with a namespace separator."
                        .to_string(),
                    code: 0,
                }
                .into());
            }
            let first = prefix.chars().next().unwrap_or('\0').to_string();
            self.prefix_lengths_psr4
                .entry(first)
                .or_insert_with(IndexMap::new)
                .insert(prefix.to_string(), length);
            self.prefix_dirs_psr4.insert(prefix.to_string(), paths);
        }
        Ok(())
    }

    /// Turns on searching the include path for class files.
    pub fn set_use_include_path(&mut self, use_include_path: bool) {
        self.use_include_path = use_include_path;
    }

    /// Can be used to check if the autoloader uses the include path to check
    /// for classes.
    pub fn get_use_include_path(&self) -> bool {
        self.use_include_path
    }

    /// Turns off searching the prefix and fallback directories for classes
    /// that have not been registered with the class map.
    pub fn set_class_map_authoritative(&mut self, class_map_authoritative: bool) {
        self.class_map_authoritative = class_map_authoritative;
    }

    /// Should class lookup fail if not found in the current class map?
    pub fn is_class_map_authoritative(&self) -> bool {
        self.class_map_authoritative
    }

    /// APCu prefix to use to cache found/not-found classes, if the extension is enabled.
    pub fn set_apcu_prefix(&mut self, apcu_prefix: Option<String>) {
        self.apcu_prefix = if function_exists("apcu_fetch")
            && filter_var(
                &ini_get("apc.enabled").unwrap_or_default(),
                FILTER_VALIDATE_BOOLEAN,
            ) {
            apcu_prefix
        } else {
            None
        };
    }

    /// The APCu prefix in use, or null if APCu caching is not enabled.
    pub fn get_apcu_prefix(&self) -> Option<&str> {
        self.apcu_prefix.as_deref()
    }

    /// Registers this instance as an autoloader.
    pub fn register(&self, prepend: bool) {
        spl_autoload_register(
            // PHP: array($this, 'loadClass')
            Box::new(|_class: &str| -> PhpMixed { PhpMixed::Null }),
            true,
            prepend,
        );

        if self.vendor_dir.is_none() {
            return;
        }

        let mut registered = REGISTERED_LOADERS.lock().unwrap();
        if prepend {
            let mut new_map: IndexMap<String, ClassLoader> = IndexMap::new();
            new_map.insert(self.vendor_dir.clone().unwrap(), self.clone());
            let old_map: IndexMap<String, ClassLoader> = std::mem::take(&mut *registered);
            for (k, v) in old_map {
                if !new_map.contains_key(&k) {
                    new_map.insert(k, v);
                }
            }
            *registered = new_map;
        } else {
            registered.shift_remove(self.vendor_dir.as_ref().unwrap());
            registered.insert(self.vendor_dir.clone().unwrap(), self.clone());
        }
    }

    /// Unregisters this instance as an autoloader.
    pub fn unregister(&self) {
        spl_autoload_unregister(Box::new(|_class: &str| -> PhpMixed { PhpMixed::Null }));

        if self.vendor_dir.is_some() {
            REGISTERED_LOADERS
                .lock()
                .unwrap()
                .shift_remove(self.vendor_dir.as_ref().unwrap());
        }
    }

    /// Loads the given class or interface.
    ///
    /// @return true|null True if loaded, null otherwise
    pub fn load_class(&mut self, class: &str) -> Option<bool> {
        let file = self.find_file(class);
        if let Some(file) = file {
            include_file(&file);

            return Some(true);
        }

        None
    }

    /// Finds the path to the file where the class is defined.
    ///
    /// @return string|false The path if found, false otherwise
    pub fn find_file(&mut self, class: &str) -> Option<String> {
        // class map lookup
        if let Some(path) = self.class_map.get(class) {
            return Some(path.clone());
        }
        if self.class_map_authoritative || self.missing_classes.contains_key(class) {
            return None;
        }
        if self.apcu_prefix.is_some() {
            let mut hit = false;
            let file = apcu_fetch(
                &format!("{}{}", self.apcu_prefix.as_ref().unwrap(), class),
                &mut hit,
            );
            if hit {
                return file.as_string().map(String::from);
            }
        }

        let mut file = self.find_file_with_extension(class, ".php");

        // Search for Hack files if we are running on HHVM
        if file.is_none() && defined("HHVM_VERSION") {
            file = self.find_file_with_extension(class, ".hh");
        }

        if self.apcu_prefix.is_some() {
            apcu_add(
                &format!("{}{}", self.apcu_prefix.as_ref().unwrap(), class),
                match file.as_ref() {
                    Some(s) => PhpMixed::String(s.clone()),
                    None => PhpMixed::Bool(false),
                },
            );
        }

        if file.is_none() {
            // Remember that this class does not exist.
            self.missing_classes.insert(class.to_string(), true);
        }

        file
    }

    /// Returns the currently registered loaders keyed by their corresponding vendor directories.
    ///
    /// @return array<string, self>
    pub fn get_registered_loaders() -> IndexMap<String, ClassLoader> {
        REGISTERED_LOADERS.lock().unwrap().clone()
    }

    /// @return string|false
    fn find_file_with_extension(&self, class: &str, ext: &str) -> Option<String> {
        // PSR-4 lookup
        let logical_path_psr4 = format!("{}{}", strtr(class, "\\", DIRECTORY_SEPARATOR), ext);

        let first = class.chars().next().unwrap_or('\0').to_string();
        if self.prefix_lengths_psr4.contains_key(&first) {
            let mut sub_path = class.to_string();
            loop {
                let last_pos = strrpos(&sub_path, "\\");
                if last_pos.is_none() {
                    break;
                }
                let last_pos = last_pos.unwrap();
                sub_path = substr(&sub_path, 0, Some(last_pos as i64));
                let search = format!("{}\\", sub_path);
                if let Some(dirs) = self.prefix_dirs_psr4.get(&search) {
                    let path_end = format!(
                        "{}{}",
                        DIRECTORY_SEPARATOR,
                        substr(&logical_path_psr4, (last_pos + 1) as i64, None)
                    );
                    for dir in dirs {
                        let file = format!("{}{}", dir, path_end);
                        if file_exists(&file) {
                            return Some(file);
                        }
                    }
                }
            }
        }

        // PSR-4 fallback dirs
        for dir in &self.fallback_dirs_psr4 {
            let file = format!("{}{}{}", dir, DIRECTORY_SEPARATOR, logical_path_psr4);
            if file_exists(&file) {
                return Some(file);
            }
        }

        // PSR-0 lookup
        let logical_path_psr0: String;
        if let Some(pos) = strrpos(class, "\\") {
            // namespaced class name
            logical_path_psr0 = format!(
                "{}{}",
                substr(&logical_path_psr4, 0, Some((pos + 1) as i64)),
                strtr(
                    &substr(&logical_path_psr4, (pos + 1) as i64, None),
                    "_",
                    DIRECTORY_SEPARATOR
                )
            );
        } else {
            // PEAR-like class name
            logical_path_psr0 = format!("{}{}", strtr(class, "_", DIRECTORY_SEPARATOR), ext);
        }

        if let Some(prefixes) = self.prefixes_psr0.get(&first) {
            for (prefix, dirs) in prefixes {
                if Some(0) == strpos(class, prefix) {
                    for dir in dirs {
                        let file = format!("{}{}{}", dir, DIRECTORY_SEPARATOR, logical_path_psr0);
                        if file_exists(&file) {
                            return Some(file);
                        }
                    }
                }
            }
        }

        // PSR-0 fallback dirs
        for dir in &self.fallback_dirs_psr0 {
            let file = format!("{}{}{}", dir, DIRECTORY_SEPARATOR, logical_path_psr0);
            if file_exists(&file) {
                return Some(file);
            }
        }

        // PSR-0 include paths.
        if self.use_include_path {
            if let Some(file) = stream_resolve_include_path(&logical_path_psr0) {
                return Some(file);
            }
        }

        None
    }

    fn initialize_include_closure() {
        // TODO(phase-b): preserve PHP `\Closure::bind(static fn($file) => include $file, null, null)`
        // Rust has no `include` operator; this is a no-op placeholder.
    }
}
