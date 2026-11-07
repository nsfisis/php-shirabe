//! ref: composer/src/Composer/Autoload/ClassLoader.php sha1:7f146de662b0599355b3b049a161e23cbdbbdaeb

use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_rpc::stream_resolve_include_path;
use shirabe_php_shim::{
    InvalidArgumentException, PhpMixed, bformat, file_exists, path_from_bytes, path_to_bytes,
    strlen, strpos, strrpos, strtr, substr,
};

thread_local! {
    static REGISTERED_LOADERS: std::cell::RefCell<IndexMap<BString, std::rc::Rc<std::cell::RefCell<ClassLoader>>>> =
        std::cell::RefCell::new(IndexMap::new());

    /// The loaders `register` puts on PHP's spl autoload stack, in stack order. Unlike
    /// `REGISTERED_LOADERS`, which keeps one loader per vendor directory, it keeps every registered
    /// loader, as the spl stack does.
    static AUTOLOAD_STACK: std::cell::RefCell<Vec<std::rc::Rc<std::cell::RefCell<ClassLoader>>>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// ClassLoader implements a PSR-0, PSR-4 and classmap class loader.
///
/// `loadClass` is intentionally not ported. This function resolves class names to concrete files
/// and `include`s them. Shirabe resolves files on the Rust side and lets the PHP worker `include`
/// them. `$includeFile` and `initializeIncludeClosure` are not ported for the same reason.
#[derive(Debug)]
pub struct ClassLoader {
    vendor_dir: Option<BString>,

    // PSR-4
    prefix_lengths_psr4: IndexMap<BString, IndexMap<BString, i64>>,
    prefix_dirs_psr4: IndexMap<BString, Vec<BString>>,
    fallback_dirs_psr4: Vec<BString>,

    // PSR-0
    /// List of PSR-0 prefixes
    ///
    /// Structured as array('F (first letter)' => array('Foo\Bar (full prefix)' => array('path', 'path2')))
    prefixes_psr0: IndexMap<BString, IndexMap<BString, Vec<BString>>>,
    fallback_dirs_psr0: Vec<BString>,

    use_include_path: bool,
    class_map: IndexMap<BString, BString>,
    class_map_authoritative: bool,
    missing_classes: IndexMap<BString, bool>,
    apcu_prefix: Option<BString>,
}

// PHP: $string[0], the first byte.
impl ClassLoader {
    pub fn new(vendor_dir: Option<BString>) -> Self {
        Self {
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
        }
    }

    pub fn get_prefixes(&self) -> IndexMap<BString, Vec<BString>> {
        if !self.prefixes_psr0.is_empty() {
            // PHP: call_user_func_array('array_merge', array_values($this->prefixesPsr0))
            // The per-first-char maps are flattened into one prefix => dirs map. array_merge with
            // string keys keeps the first position and lets the later value win, which is exactly
            // IndexMap::insert.
            let mut result: IndexMap<BString, Vec<BString>> = IndexMap::new();
            for inner in self.prefixes_psr0.values() {
                for (prefix, dirs) in inner {
                    result.insert(prefix.clone(), dirs.clone());
                }
            }
            return result;
        }

        IndexMap::new()
    }

    pub fn get_prefixes_psr4(&self) -> &IndexMap<BString, Vec<BString>> {
        &self.prefix_dirs_psr4
    }

    pub fn get_fallback_dirs(&self) -> &Vec<BString> {
        &self.fallback_dirs_psr0
    }

    pub fn get_fallback_dirs_psr4(&self) -> &Vec<BString> {
        &self.fallback_dirs_psr4
    }

    pub fn get_class_map(&self) -> &IndexMap<BString, BString> {
        &self.class_map
    }

    pub fn add_class_map(&mut self, class_map: IndexMap<BString, BString>) {
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
    pub fn add(&mut self, prefix: impl AsRef<[u8]>, paths: Vec<BString>, prepend: bool) {
        let prefix = prefix.as_ref().as_bstr();
        if prefix.is_empty() {
            if prepend {
                let mut new_dirs = paths;
                new_dirs.append(&mut self.fallback_dirs_psr0);
                self.fallback_dirs_psr0 = new_dirs;
            } else {
                self.fallback_dirs_psr0.extend(paths);
            }

            return;
        }

        let first = BString::from(&prefix[..1]);
        let entry = self.prefixes_psr0.entry(first).or_default();
        if !entry.contains_key(prefix) {
            entry.insert(prefix.to_owned(), paths);
            return;
        }
        let existing = entry.get_mut(prefix).unwrap();
        if prepend {
            let mut new_dirs = paths;
            new_dirs.append(existing);
            *existing = new_dirs;
        } else {
            existing.extend(paths);
        }
    }

    /// Registers a set of PSR-4 directories for a given namespace, either
    /// appending or prepending to the ones previously set for this namespace.
    pub fn add_psr4(
        &mut self,
        prefix: impl AsRef<[u8]>,
        paths: Vec<BString>,
        prepend: bool,
    ) -> anyhow::Result<()> {
        let prefix = prefix.as_ref().as_bstr();
        if prefix.is_empty() {
            // Register directories for the root namespace.
            if prepend {
                let mut new_dirs = paths;
                new_dirs.append(&mut self.fallback_dirs_psr4);
                self.fallback_dirs_psr4 = new_dirs;
            } else {
                self.fallback_dirs_psr4.extend(paths);
            }
        } else if !self.prefix_dirs_psr4.contains_key(prefix) {
            // Register directories for a new namespace.
            let length = strlen(prefix);
            if "\\" != substr(prefix, length - 1, Some(1)) {
                return Err(InvalidArgumentException::new(BString::from(
                    "A non-empty PSR-4 prefix must end with a namespace separator.",
                ))
                .into());
            }
            self.prefix_lengths_psr4
                .entry(BString::from(&prefix[..1]))
                .or_default()
                .insert(prefix.to_owned(), length);
            self.prefix_dirs_psr4.insert(prefix.to_owned(), paths);
        } else if prepend {
            // Prepend directories for an already registered namespace.
            let existing = self.prefix_dirs_psr4.get_mut(prefix).unwrap();
            let mut new_dirs = paths;
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
    pub fn set(&mut self, prefix: impl AsRef<[u8]>, paths: Vec<BString>) {
        let prefix = prefix.as_ref().as_bstr();
        if prefix.is_empty() {
            self.fallback_dirs_psr0 = paths;
        } else {
            self.prefixes_psr0
                .entry(BString::from(&prefix[..1]))
                .or_default()
                .insert(prefix.to_owned(), paths);
        }
    }

    /// Registers a set of PSR-4 directories for a given namespace,
    /// replacing any others previously set for this namespace.
    pub fn set_psr4(
        &mut self,
        prefix: impl AsRef<[u8]>,
        paths: Vec<BString>,
    ) -> anyhow::Result<()> {
        let prefix = prefix.as_ref().as_bstr();
        if prefix.is_empty() {
            self.fallback_dirs_psr4 = paths;
        } else {
            let length = strlen(prefix);
            if "\\" != substr(prefix, length - 1, Some(1)) {
                return Err(InvalidArgumentException::new(BString::from(
                    "A non-empty PSR-4 prefix must end with a namespace separator.",
                ))
                .into());
            }
            self.prefix_lengths_psr4
                .entry(BString::from(&prefix[..1]))
                .or_default()
                .insert(prefix.to_owned(), length);
            self.prefix_dirs_psr4.insert(prefix.to_owned(), paths);
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
    pub fn set_apcu_prefix(&mut self, _apcu_prefix: Option<BString>) {
        // APCu is not available in Rust.
        self.apcu_prefix = None;
    }

    /// The APCu prefix in use, or null if APCu caching is not enabled.
    pub fn get_apcu_prefix(&self) -> Option<&BStr> {
        self.apcu_prefix.as_ref().map(|p| p.as_bstr())
    }

    /// Registers this instance as an autoloader.
    pub fn register(this: &std::rc::Rc<std::cell::RefCell<Self>>, prepend: bool) {
        // PHP registers loadClass on the spl autoload stack. Here classes are loaded by the PHP
        // worker, whose autoloader looks files up in the loaders of this stack.
        AUTOLOAD_STACK.with_borrow_mut(|stack| {
            // Like spl_autoload_register, a callback already on the stack stays where it is.
            if !stack.iter().any(|loader| std::rc::Rc::ptr_eq(loader, this)) {
                if prepend {
                    stack.insert(0, this.clone());
                } else {
                    stack.push(this.clone());
                }
            }
        });

        let Some(vendor_dir) = this.borrow().vendor_dir.clone() else {
            return;
        };

        REGISTERED_LOADERS.with_borrow_mut(|registered| {
            if prepend {
                let mut new_map: IndexMap<BString, std::rc::Rc<std::cell::RefCell<ClassLoader>>> =
                    IndexMap::new();
                new_map.insert(vendor_dir, this.clone());
                for (k, v) in std::mem::take(registered) {
                    if !new_map.contains_key(&k) {
                        new_map.insert(k, v);
                    }
                }
                *registered = new_map;
            } else {
                registered.shift_remove(&vendor_dir);
                registered.insert(vendor_dir, this.clone());
            }
        });
    }

    /// Unregisters this instance as an autoloader.
    pub fn unregister(this: &std::rc::Rc<std::cell::RefCell<Self>>) {
        AUTOLOAD_STACK
            .with_borrow_mut(|stack| stack.retain(|loader| !std::rc::Rc::ptr_eq(loader, this)));

        if let Some(vendor_dir) = &this.borrow().vendor_dir {
            REGISTERED_LOADERS.with_borrow_mut(|registered| registered.shift_remove(vendor_dir));
        }
    }

    /// The loaders on the spl autoload stack, in the order PHP calls them for a class.
    pub(crate) fn autoload_stack() -> Vec<std::rc::Rc<std::cell::RefCell<ClassLoader>>> {
        AUTOLOAD_STACK.with_borrow(|stack| stack.clone())
    }

    /// Finds the path to the file where the class is defined.
    pub fn find_file(&mut self, class: impl AsRef<[u8]>) -> anyhow::Result<Option<BString>> {
        let class = class.as_ref().as_bstr();
        // class map lookup
        if let Some(path) = self.class_map.get(class) {
            return Ok(Some(path.clone()));
        }
        if self.class_map_authoritative || self.missing_classes.contains_key(class) {
            return Ok(None);
        }
        // PHP also consults and fills the APCu cache around the lookup below. APCu is never enabled
        // here (see set_apcu_prefix), so the prefix is always null.

        // TODO(php-runtime): PHP also looks for a Hack file (`.hh`) when it runs on HHVM. Only the
        // worker, which is the runtime that includes the file, can answer whether it is HHVM, and
        // asking boots it, while this lookup is what decides whether it is needed at all.
        let file = self.find_file_with_extension(class, BStr::new(".php"))?;

        if file.is_none() {
            // Remember that this class does not exist.
            self.missing_classes.insert(class.to_owned(), true);
        }

        Ok(file)
    }

    /// Returns the currently registered loaders keyed by their corresponding vendor directories.
    pub fn get_registered_loaders()
    -> IndexMap<BString, std::rc::Rc<std::cell::RefCell<ClassLoader>>> {
        REGISTERED_LOADERS.with_borrow(|registered| registered.clone())
    }

    fn find_file_with_extension(
        &self,
        class: &BStr,
        ext: &BStr,
    ) -> anyhow::Result<Option<BString>> {
        // PSR-4 lookup
        let logical_path_psr4 =
            bformat!("{}{ext}", strtr(class, "\\", std::path::MAIN_SEPARATOR_STR));

        let first = BString::from(&class[..1.min(class.len())]);
        if self.prefix_lengths_psr4.contains_key(&first) {
            let mut sub_path = class.to_owned();
            while let Some(last_pos) = strrpos(&sub_path, "\\") {
                sub_path = substr(&sub_path, 0, Some(last_pos as i64));
                let search = bformat!("{sub_path}\\");
                if let Some(dirs) = self.prefix_dirs_psr4.get(&search) {
                    let path_end = bformat!(
                        "{}{}",
                        std::path::MAIN_SEPARATOR_STR,
                        substr(&logical_path_psr4, (last_pos + 1) as i64, None)
                    );
                    for dir in dirs {
                        let file = bformat!("{dir}{path_end}");
                        if file_exists(path_from_bytes(&file)) {
                            return Ok(Some(file));
                        }
                    }
                }
            }
        }

        // PSR-4 fallback dirs
        for dir in &self.fallback_dirs_psr4 {
            let file = bformat!("{dir}{}{logical_path_psr4}", std::path::MAIN_SEPARATOR_STR);
            if file_exists(path_from_bytes(&file)) {
                return Ok(Some(file));
            }
        }

        // PSR-0 lookup
        let logical_path_psr0 = if let Some(pos) = strrpos(class, "\\") {
            // namespaced class name
            bformat!(
                "{}{}",
                substr(&logical_path_psr4, 0, Some((pos + 1) as i64)),
                strtr(
                    substr(&logical_path_psr4, (pos + 1) as i64, None),
                    "_",
                    std::path::MAIN_SEPARATOR_STR,
                )
            )
        } else {
            // PEAR-like class name
            bformat!("{}{ext}", strtr(class, "_", std::path::MAIN_SEPARATOR_STR))
        };

        if let Some(prefixes) = self.prefixes_psr0.get(&first) {
            for (prefix, dirs) in prefixes {
                if Some(0) == strpos(class, prefix) {
                    for dir in dirs {
                        let file =
                            bformat!("{dir}{}{logical_path_psr0}", std::path::MAIN_SEPARATOR_STR);
                        if file_exists(path_from_bytes(&file)) {
                            return Ok(Some(file));
                        }
                    }
                }
            }
        }

        // PSR-0 fallback dirs
        for dir in &self.fallback_dirs_psr0 {
            let file = bformat!("{dir}{}{logical_path_psr0}", std::path::MAIN_SEPARATOR_STR);
            if file_exists(path_from_bytes(&file)) {
                return Ok(Some(file));
            }
        }

        // PSR-0 include paths.
        if self.use_include_path
            && let Some(file) = stream_resolve_include_path(&logical_path_psr0)?
        {
            return Ok(Some(BString::from(path_to_bytes(&file))));
        }

        Ok(None)
    }

    /// PHP `(array) $loader`. Every property is private, so keys are mangled as
    /// `"\0Composer\Autoload\ClassLoader\0<propertyName>"` using the original camelCase names, in
    /// declaration order.
    pub fn as_array_iter(&self) -> Vec<(BString, PhpMixed)> {
        let key = |name: &BStr| bformat!("\0Composer\\Autoload\\ClassLoader\0{name}");
        let str_list = |v: &Vec<BString>| {
            PhpMixed::List(v.iter().map(|s| PhpMixed::string(s.clone())).collect())
        };

        vec![
            (
                key(BStr::new("vendorDir")),
                match &self.vendor_dir {
                    Some(s) => PhpMixed::string(s.clone()),
                    None => PhpMixed::Null,
                },
            ),
            (
                key(BStr::new("prefixLengthsPsr4")),
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
                key(BStr::new("prefixDirsPsr4")),
                PhpMixed::Array(
                    self.prefix_dirs_psr4
                        .iter()
                        .map(|(k, v)| (k.clone(), str_list(v)))
                        .collect(),
                ),
            ),
            (
                key(BStr::new("fallbackDirsPsr4")),
                str_list(&self.fallback_dirs_psr4),
            ),
            (
                key(BStr::new("prefixesPsr0")),
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
            (
                key(BStr::new("fallbackDirsPsr0")),
                str_list(&self.fallback_dirs_psr0),
            ),
            (
                key(BStr::new("useIncludePath")),
                PhpMixed::Bool(self.use_include_path),
            ),
            (
                key(BStr::new("classMap")),
                PhpMixed::Array(
                    self.class_map
                        .iter()
                        .map(|(k, v)| (k.clone(), PhpMixed::string(v.clone())))
                        .collect(),
                ),
            ),
            (
                key(BStr::new("classMapAuthoritative")),
                PhpMixed::Bool(self.class_map_authoritative),
            ),
            (
                key(BStr::new("missingClasses")),
                PhpMixed::Array(
                    self.missing_classes
                        .iter()
                        .map(|(k, b)| (k.clone(), PhpMixed::Bool(*b)))
                        .collect(),
                ),
            ),
            (
                key(BStr::new("apcuPrefix")),
                match &self.apcu_prefix {
                    Some(s) => PhpMixed::string(s.clone()),
                    None => PhpMixed::Null,
                },
            ),
        ]
    }
}
