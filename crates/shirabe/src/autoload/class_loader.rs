//! ref: composer/src/Composer/Autoload/ClassLoader.php

use indexmap::IndexMap;
use shirabe_php_shim::{
    DIRECTORY_SEPARATOR, InvalidArgumentException, PhpMixed, defined, file_exists, include_file,
    spl_autoload_register, spl_autoload_unregister, stream_resolve_include_path, strlen, strpos,
    strrpos, strtr, substr,
};
use std::sync::{LazyLock, Mutex};

/// @var array<string, self>
static REGISTERED_LOADERS: LazyLock<Mutex<IndexMap<String, ClassLoader>>> =
    LazyLock::new(|| Mutex::new(IndexMap::new()));

/// ClassLoader implements a PSR-0, PSR-4 and classmap class loader.
#[derive(Debug, Clone)]
pub struct ClassLoader {
    // PHP holds a `private static $includeFile` closure purely to run `include $file` in an
    // isolated scope. The `include_file` shim is a free function with no `self`/`Self` access, so
    // that scope isolation is inherent and there is no static state to keep on the Rust side.
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
            // The per-first-char maps are flattened into one prefix => dirs map. array_merge with
            // string keys keeps the first position and lets the later value win, which is exactly
            // IndexMap::insert.
            let mut result: IndexMap<String, Vec<String>> = IndexMap::new();
            for inner in self.prefixes_psr0.values() {
                for (prefix, dirs) in inner {
                    result.insert(prefix.clone(), dirs.clone());
                }
            }
            return result;
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
            // array_merge keeps existing string keys in place and lets $classMap overwrite their
            // values while appending new ones, which is exactly IndexMap::extend.
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
                new_dirs.append(&mut self.fallback_dirs_psr0);
                self.fallback_dirs_psr0 = new_dirs;
            } else {
                self.fallback_dirs_psr0.extend(paths);
            }

            return;
        }

        let first = prefix.chars().next().unwrap_or('\0').to_string();
        let entry = self.prefixes_psr0.entry(first.clone()).or_default();
        if !entry.contains_key(prefix) {
            entry.insert(prefix.to_string(), paths);
            return;
        }
        let existing = entry.get_mut(prefix).unwrap();
        if prepend {
            let mut new_dirs = paths.clone();
            new_dirs.append(existing);
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
                new_dirs.append(&mut self.fallback_dirs_psr4);
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
                .or_default()
                .insert(prefix.to_string(), length);
            self.prefix_dirs_psr4.insert(prefix.to_string(), paths);
        } else if prepend {
            // Prepend directories for an already registered namespace.
            let existing = self.prefix_dirs_psr4.get_mut(prefix).unwrap();
            let mut new_dirs = paths.clone();
            new_dirs.append(existing);
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
                .or_default()
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
                .or_default()
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
    pub fn set_apcu_prefix(&mut self, _apcu_prefix: Option<String>) {
        // APCu is not available in Rust.
        self.apcu_prefix = None;
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

        if let Some(vendor_dir) = &self.vendor_dir {
            REGISTERED_LOADERS.lock().unwrap().shift_remove(vendor_dir);
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
        if let Some(apcu_prefix) = &self.apcu_prefix {
            // No-op; APCu is not available in Rust.
        }

        let mut file = self.find_file_with_extension(class, ".php");

        // Search for Hack files if we are running on HHVM
        if file.is_none() && defined("HHVM_VERSION") {
            file = self.find_file_with_extension(class, ".hh");
        }

        if let Some(apcu_prefix) = &self.apcu_prefix {
            // No-op; APCu is not available in Rust.
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
        if self.use_include_path
            && let Some(file) = stream_resolve_include_path(&logical_path_psr0)
        {
            return Some(file);
        }

        None
    }

    fn initialize_include_closure() {
        // PHP lazily binds `self::$includeFile` to a scope-isolated `include $file` closure. The
        // Rust `include_file` shim already provides that isolation as a free function, so there is
        // no closure to bind and this is intentionally a no-op.
    }

    /// PHP `(array) $loader`. Every property is private, so keys are mangled as
    /// `"\0Composer\Autoload\ClassLoader\0<propertyName>"` using the original camelCase names, in
    /// declaration order.
    pub fn as_array_iter(&self) -> Vec<(String, PhpMixed)> {
        let key = |name: &str| format!("\0Composer\\Autoload\\ClassLoader\0{}", name);
        let str_list = |v: &Vec<String>| {
            PhpMixed::List(v.iter().map(|s| PhpMixed::String(s.clone())).collect())
        };

        vec![
            (
                key("vendorDir"),
                match &self.vendor_dir {
                    Some(s) => PhpMixed::String(s.clone()),
                    None => PhpMixed::Null,
                },
            ),
            (
                key("prefixLengthsPsr4"),
                PhpMixed::Array(
                    self.prefix_lengths_psr4
                        .iter()
                        .map(|(k, inner)| {
                            (
                                k.clone(),
                                PhpMixed::Array(
                                    inner
                                        .iter()
                                        .map(|(k2, n)| (k2.clone(), PhpMixed::Int(*n)))
                                        .collect(),
                                ),
                            )
                        })
                        .collect(),
                ),
            ),
            (
                key("prefixDirsPsr4"),
                PhpMixed::Array(
                    self.prefix_dirs_psr4
                        .iter()
                        .map(|(k, v)| (k.clone(), str_list(v)))
                        .collect(),
                ),
            ),
            (key("fallbackDirsPsr4"), str_list(&self.fallback_dirs_psr4)),
            (
                key("prefixesPsr0"),
                PhpMixed::Array(
                    self.prefixes_psr0
                        .iter()
                        .map(|(k, inner)| {
                            (
                                k.clone(),
                                PhpMixed::Array(
                                    inner
                                        .iter()
                                        .map(|(k2, v)| (k2.clone(), str_list(v)))
                                        .collect(),
                                ),
                            )
                        })
                        .collect(),
                ),
            ),
            (key("fallbackDirsPsr0"), str_list(&self.fallback_dirs_psr0)),
            (key("useIncludePath"), PhpMixed::Bool(self.use_include_path)),
            (
                key("classMap"),
                PhpMixed::Array(
                    self.class_map
                        .iter()
                        .map(|(k, v)| (k.clone(), PhpMixed::String(v.clone())))
                        .collect(),
                ),
            ),
            (
                key("classMapAuthoritative"),
                PhpMixed::Bool(self.class_map_authoritative),
            ),
            (
                key("missingClasses"),
                PhpMixed::Array(
                    self.missing_classes
                        .iter()
                        .map(|(k, b)| (k.clone(), PhpMixed::Bool(*b)))
                        .collect(),
                ),
            ),
            (
                key("apcuPrefix"),
                match &self.apcu_prefix {
                    Some(s) => PhpMixed::String(s.clone()),
                    None => PhpMixed::Null,
                },
            ),
        ]
    }
}
