//! ref: composer/src/Composer/Package/Locker.php

use anyhow::Result;
use indexmap::IndexMap;

use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_external_packages::seld::json_lint::ParsingException;
use shirabe_php_shim::{
    DATE_RFC3339, LogicException, PhpMixed, RuntimeException, array_intersect, array_keys,
    array_map, array_merge, file_get_contents, filemtime, function_exists, hash, in_array, is_int,
    ksort, realpath, strcmp, strtolower, touch2, trim, usort,
};

use crate::installer::InstallationManager;
use crate::io::IOInterface;
use crate::json::JsonEncodeOptions;
use crate::json::JsonFile;
use crate::package::BasePackageHandle;
use crate::package::CompleteAliasPackageHandle;
use crate::package::Link;
use crate::package::PackageInterfaceHandle;
use crate::package::RootPackageInterfaceHandle;
use crate::package::dumper::ArrayDumper;
use crate::package::loader::ArrayLoader;
use crate::package::loader::LoaderInterface;
use crate::package::version::VersionParser;
use crate::plugin::plugin_interface::{self};
use crate::repository::FindPackageConstraint;
use crate::repository::InstalledRepository;
use crate::repository::LockArrayRepository;
use crate::repository::LockArrayRepositoryHandle;
use crate::repository::PlatformRepository;
use crate::repository::RootPackageRepository;
use crate::util::Git as GitUtil;
use crate::util::ProcessExecutor;

/// Reads/writes project lockfile (composer.lock).
#[derive(Debug)]
pub struct Locker {
    /// @var JsonFile
    lock_file: JsonFile,
    /// @var InstallationManager
    installation_manager: std::rc::Rc<std::cell::RefCell<InstallationManager>>,
    /// @var string
    hash: String,
    /// @var string
    content_hash: String,
    /// @var ArrayLoader
    loader: ArrayLoader,
    /// @var ArrayDumper
    dumper: ArrayDumper,
    /// @var ProcessExecutor
    process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    /// @var mixed[]|null
    lock_data_cache: std::cell::RefCell<Option<IndexMap<String, PhpMixed>>>,
    /// @var bool
    virtual_file_written: bool,
}

impl Locker {
    /// Initializes packages locker.
    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        lock_file: JsonFile,
        installation_manager: std::rc::Rc<std::cell::RefCell<InstallationManager>>,
        composer_file_contents: &str,
        process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    ) -> Self {
        Self {
            lock_file,
            installation_manager,
            hash: hash("md5", composer_file_contents),
            content_hash: Self::get_content_hash(composer_file_contents).unwrap_or_default(),
            loader: ArrayLoader::new(None, true),
            dumper: ArrayDumper::new(),
            process,
            lock_data_cache: std::cell::RefCell::new(None),
            virtual_file_written: false,
        }
    }

    /// @internal
    pub fn get_json_file(&self) -> &JsonFile {
        &self.lock_file
    }

    /// Returns the md5 hash of the sorted content of the composer file.
    pub fn get_content_hash(composer_file_contents: &str) -> Result<String> {
        let content = JsonFile::parse_json(Some(composer_file_contents), Some("composer.json"))?;
        let content_map: IndexMap<String, PhpMixed> = match &content {
            PhpMixed::Array(m) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            _ => IndexMap::new(),
        };

        let relevant_keys: Vec<&str> = vec![
            "name",
            "version",
            "require",
            "require-dev",
            "conflict",
            "replace",
            "provide",
            "minimum-stability",
            "prefer-stable",
            "repositories",
            "extra",
        ];

        let mut relevant_content: IndexMap<String, PhpMixed> = IndexMap::new();

        let content_keys: Vec<String> = array_keys(&content_map);
        let relevant_keys_strings: Vec<String> =
            relevant_keys.iter().map(|s| s.to_string()).collect();
        let intersected = array_intersect(&relevant_keys_strings, &content_keys);
        for key in intersected {
            if let Some(value) = content_map.get(&key) {
                relevant_content.insert(key, value.clone());
            }
        }
        let platform_value = content_map.get("config").and_then(|v| match v {
            PhpMixed::Array(m) => m.get("platform").cloned(),
            _ => None,
        });
        if let Some(platform) = platform_value {
            let mut config_map: IndexMap<String, PhpMixed> = IndexMap::new();
            config_map.insert("platform".to_string(), platform);
            relevant_content.insert("config".to_string(), PhpMixed::Array(config_map));
        }

        ksort(&mut relevant_content);

        Ok(hash(
            "md5",
            &JsonFile::encode_with_options(
                &PhpMixed::Array(relevant_content.into_iter().collect()),
                JsonEncodeOptions::none(),
            ),
        ))
    }

    /// Checks whether locker has been locked (lockfile found).
    pub fn is_locked(&mut self) -> bool {
        if !self.virtual_file_written && !self.lock_file.exists() {
            return false;
        }

        let data_result = self.get_lock_data();
        if let Ok(data) = data_result {
            return data.contains_key("packages");
        }
        false
    }

    /// Checks whether the lock file is still up to date with the current hash
    pub fn is_fresh(&mut self) -> Result<bool> {
        let lock = self.lock_file.read()?;
        let lock_map: IndexMap<String, PhpMixed> = match lock {
            PhpMixed::Array(m) => m.into_iter().collect(),
            _ => IndexMap::new(),
        };

        let content_hash = lock_map.get("content-hash");
        if let Some(content_hash) = content_hash
            && !shirabe_php_shim::empty(content_hash)
        {
            // There is a content hash key, use that instead of the file hash
            return Ok(self.content_hash == content_hash.as_string().unwrap_or(""));
        }

        // BC support for old lock files without content-hash
        let lock_hash = lock_map.get("hash");
        if let Some(lock_hash) = lock_hash
            && !shirabe_php_shim::empty(lock_hash)
        {
            return Ok(self.hash == lock_hash.as_string().unwrap_or(""));
        }

        // should not be reached unless the lock file is corrupted, so assume it's out of date
        Ok(false)
    }

    /// Searches and returns an array of locked packages, retrieved from registered repositories.
    pub fn get_locked_repository(
        &mut self,
        with_dev_reqs: bool,
    ) -> Result<LockArrayRepositoryHandle> {
        let lock_data = self.get_lock_data()?;
        let packages: LockArrayRepositoryHandle =
            LockArrayRepositoryHandle::new(LockArrayRepository::new(vec![])?);

        let mut locked_packages = lock_data
            .get("packages")
            .cloned()
            .unwrap_or(PhpMixed::List(vec![]));
        if with_dev_reqs {
            if let Some(packages_dev) = lock_data.get("packages-dev").cloned() {
                locked_packages = array_merge(locked_packages, packages_dev);
            } else {
                return Err(RuntimeException {
                    message: "The lock file does not contain require-dev information, run install with the --no-dev option or delete it and run composer update to generate a new lock file.".to_string(),
                    code: 0,
                }
                .into());
            }
        }

        if shirabe_php_shim::empty(&locked_packages) {
            return Ok(packages);
        }

        let has_name = if let PhpMixed::List(list) = &locked_packages {
            list.first()
                .map(|v| match v {
                    PhpMixed::Array(m) => m.contains_key("name"),
                    _ => false,
                })
                .unwrap_or(false)
        } else {
            false
        };
        if has_name {
            let mut package_by_name: IndexMap<String, BasePackageHandle> = IndexMap::new();
            if let PhpMixed::List(list) = locked_packages {
                for info in list {
                    if let PhpMixed::Array(m) = info {
                        let info_map: IndexMap<String, PhpMixed> =
                            m.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                        let package = self.loader.load(info_map, None)?;
                        packages.add_package(package.clone())?;
                        package_by_name.insert(package.get_name(), package.clone());

                        if let Some(alias) = package.as_alias() {
                            let alias_of: BasePackageHandle = alias.get_alias_of().into();
                            package_by_name.insert(alias_of.get_name(), alias_of);
                        }
                    }
                }
            }

            if let Some(aliases) = lock_data.get("aliases")
                && let PhpMixed::List(alias_list) = aliases
            {
                for alias in alias_list {
                    if let PhpMixed::Array(m) = alias {
                        let alias_pkg_name = m
                            .get("package")
                            .and_then(|v| v.as_string())
                            .unwrap_or("")
                            .to_string();
                        if let Some(base_pkg) = package_by_name.get(&alias_pkg_name) {
                            let alias_of = base_pkg.as_complete_package().expect(
                                    "CompleteAliasPackage requires aliasOf to be a real CompletePackage",
                                );
                            let alias_pkg = CompleteAliasPackageHandle::new(
                                alias_of,
                                m.get("alias_normalized")
                                    .and_then(|v| v.as_string())
                                    .unwrap_or("")
                                    .to_string(),
                                m.get("alias")
                                    .and_then(|v| v.as_string())
                                    .unwrap_or("")
                                    .to_string(),
                            );
                            alias_pkg.set_root_package_alias(true);
                            packages.add_package(alias_pkg.into())?;
                        }
                    }
                }
            }

            return Ok(packages);
        }

        Err(RuntimeException {
            message:
                "Your composer.lock is invalid. Run \"composer update\" to generate a new one."
                    .to_string(),
            code: 0,
        }
        .into())
    }

    /// @return string[] Names of dependencies installed through require-dev
    pub fn get_dev_package_names(&mut self) -> Result<Vec<String>> {
        let mut names: Vec<String> = vec![];
        let lock_data = self.get_lock_data()?;
        if let Some(PhpMixed::List(list)) = lock_data.get("packages-dev") {
            for package in list {
                if let PhpMixed::Array(m) = package {
                    names.push(strtolower(
                        m.get("name").and_then(|v| v.as_string()).unwrap_or(""),
                    ));
                }
            }
        }

        Ok(names)
    }

    /// Returns the platform requirements stored in the lock file
    pub fn get_platform_requirements(&mut self, with_dev_reqs: bool) -> Result<Vec<Link>> {
        let lock_data = self.get_lock_data()?;
        let mut requirements: IndexMap<String, Link> = IndexMap::new();

        let platform_value = lock_data.get("platform");
        if let Some(platform_value) = platform_value
            && !shirabe_php_shim::empty(platform_value)
        {
            requirements = self.loader.parse_links(
                "__root__",
                "1.0.0",
                Link::TYPE_REQUIRE,
                match platform_value {
                    PhpMixed::Array(m) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                    _ => IndexMap::new(),
                },
            )?;
        }

        let platform_dev_value = lock_data.get("platform-dev");
        if with_dev_reqs
            && let Some(platform_dev_value) = platform_dev_value
            && !shirabe_php_shim::empty(platform_dev_value)
        {
            let dev_requirements = self.loader.parse_links(
                "__root__",
                "1.0.0",
                Link::TYPE_REQUIRE,
                match platform_dev_value {
                    PhpMixed::Array(m) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                    _ => IndexMap::new(),
                },
            )?;

            for (k, v) in dev_requirements {
                requirements.insert(k, v);
            }
        }

        Ok(requirements.into_iter().map(|(_, v)| v).collect())
    }

    /// @return key-of<BasePackage::STABILITIES>
    pub fn get_minimum_stability(&mut self) -> Result<String> {
        let lock_data = self.get_lock_data()?;

        Ok(lock_data
            .get("minimum-stability")
            .and_then(|v| v.as_string())
            .unwrap_or("stable")
            .to_string())
    }

    /// @return array<string, string>
    pub fn get_stability_flags(&mut self) -> Result<IndexMap<String, String>> {
        let lock_data = self.get_lock_data()?;

        Ok(lock_data
            .get("stability-flags")
            .and_then(|v| match v {
                PhpMixed::Array(m) => Some(
                    m.iter()
                        .map(|(k, v)| (k.clone(), v.as_string().unwrap_or("").to_string()))
                        .collect(),
                ),
                _ => None,
            })
            .unwrap_or_default())
    }

    pub fn get_prefer_stable(&mut self) -> Result<Option<bool>> {
        let lock_data = self.get_lock_data()?;

        // return null if not set to allow caller logic to choose the
        // right behavior since old lock files have no prefer-stable
        Ok(lock_data.get("prefer-stable").and_then(|v| v.as_bool()))
    }

    pub fn get_prefer_lowest(&mut self) -> Result<Option<bool>> {
        let lock_data = self.get_lock_data()?;

        Ok(lock_data.get("prefer-lowest").and_then(|v| v.as_bool()))
    }

    /// @return array<string, string>
    pub fn get_platform_overrides(&mut self) -> Result<IndexMap<String, String>> {
        let lock_data = self.get_lock_data()?;

        Ok(lock_data
            .get("platform-overrides")
            .and_then(|v| match v {
                PhpMixed::Array(m) => Some(
                    m.iter()
                        .map(|(k, v)| (k.clone(), v.as_string().unwrap_or("").to_string()))
                        .collect(),
                ),
                _ => None,
            })
            .unwrap_or_default())
    }

    /// @return string[][]
    pub fn get_aliases(&mut self) -> Result<Vec<IndexMap<String, String>>> {
        let lock_data = self.get_lock_data()?;

        Ok(lock_data
            .get("aliases")
            .and_then(|v| match v {
                PhpMixed::List(list) => Some(
                    list.iter()
                        .filter_map(|v| match v {
                            PhpMixed::Array(m) => Some(
                                m.iter()
                                    .map(|(k, v)| {
                                        (k.clone(), v.as_string().unwrap_or("").to_string())
                                    })
                                    .collect(),
                            ),
                            _ => None,
                        })
                        .collect(),
                ),
                _ => None,
            })
            .unwrap_or_default())
    }

    pub fn get_plugin_api(&mut self) -> Result<String> {
        let lock_data = self.get_lock_data()?;

        Ok(lock_data
            .get("plugin-api-version")
            .and_then(|v| v.as_string())
            .unwrap_or("1.1.0")
            .to_string())
    }

    /// @return array<string, mixed>
    pub fn get_lock_data(&mut self) -> Result<IndexMap<String, PhpMixed>> {
        if let Some(cache) = self.lock_data_cache.borrow().clone() {
            return Ok(cache);
        }

        if !self.lock_file.exists() {
            return Err(LogicException {
                message: "No lockfile found. Unable to read locked packages".to_string(),
                code: 0,
            }
            .into());
        }

        let data_php = self.lock_file.read()?;
        let data: IndexMap<String, PhpMixed> = match data_php {
            PhpMixed::Array(m) => m.into_iter().collect(),
            _ => IndexMap::new(),
        };
        *self.lock_data_cache.borrow_mut() = Some(data.clone());
        Ok(data)
    }

    /// Locks provided data into lockfile.
    #[allow(clippy::too_many_arguments, reason = "to keep PHP signature")]
    pub fn set_lock_data(
        &mut self,
        packages: Vec<PackageInterfaceHandle>,
        dev_packages: Option<Vec<PackageInterfaceHandle>>,
        platform_reqs: IndexMap<String, String>,
        platform_dev_reqs: IndexMap<String, String>,
        aliases: Vec<IndexMap<String, PhpMixed>>,
        minimum_stability: &str,
        stability_flags: IndexMap<String, i64>,
        prefer_stable: bool,
        prefer_lowest: bool,
        platform_overrides: IndexMap<String, PhpMixed>,
        write: bool,
    ) -> Result<bool> {
        // keep old default branch names normalized to DEFAULT_BRANCH_ALIAS for BC as that is how Composer 1 outputs the lock file
        // when loading the lock file the version is anyway ignored in Composer 2, so it has no adverse effect
        let aliases: Vec<IndexMap<String, PhpMixed>> = array_map(
            |alias: &IndexMap<String, PhpMixed>| {
                let mut alias = alias.clone();
                let version = alias
                    .get("version")
                    .and_then(|v| v.as_string())
                    .unwrap_or("")
                    .to_string();
                if in_array(
                    PhpMixed::String(version),
                    &PhpMixed::List(vec![
                        PhpMixed::String("dev-master".to_string()),
                        PhpMixed::String("dev-trunk".to_string()),
                        PhpMixed::String("dev-default".to_string()),
                    ]),
                    true,
                ) {
                    alias.insert(
                        "version".to_string(),
                        PhpMixed::String(VersionParser::DEFAULT_BRANCH_ALIAS.to_string()),
                    );
                }
                alias
            },
            &aliases,
        );

        let mut lock: IndexMap<String, PhpMixed> = IndexMap::new();
        lock.insert(
            "_readme".to_string(),
            PhpMixed::List(vec![
                PhpMixed::String(
                    "This file locks the dependencies of your project to a known state".to_string(),
                ),
                PhpMixed::String(
                    "Read more about it at https://getcomposer.org/doc/01-basic-usage.md#installing-dependencies".to_string(),
                ),
                PhpMixed::String(
                    format!("This file is @{}ated automatically", "gener"),
                ),
            ]),
        );
        lock.insert(
            "content-hash".to_string(),
            PhpMixed::String(self.content_hash.clone()),
        );
        lock.insert("packages".to_string(), self.lock_packages(&packages)?);
        lock.insert("packages-dev".to_string(), PhpMixed::Null);
        lock.insert(
            "aliases".to_string(),
            PhpMixed::List(
                aliases
                    .iter()
                    .map(|m| {
                        PhpMixed::Array(m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                    })
                    .collect(),
            ),
        );
        lock.insert(
            "minimum-stability".to_string(),
            PhpMixed::String(minimum_stability.to_string()),
        );
        lock.insert(
            "stability-flags".to_string(),
            PhpMixed::Array(
                stability_flags
                    .iter()
                    .map(|(k, v)| (k.clone(), PhpMixed::Int(*v)))
                    .collect(),
            ),
        );
        lock.insert("prefer-stable".to_string(), PhpMixed::Bool(prefer_stable));
        lock.insert("prefer-lowest".to_string(), PhpMixed::Bool(prefer_lowest));

        if let Some(dev_packages) = dev_packages {
            lock.insert(
                "packages-dev".to_string(),
                self.lock_packages(&dev_packages)?,
            );
        }

        lock.insert(
            "platform".to_string(),
            PhpMixed::Array(
                platform_reqs
                    .iter()
                    .map(|(k, v)| (k.clone(), PhpMixed::String(v.clone())))
                    .collect(),
            ),
        );
        lock.insert(
            "platform-dev".to_string(),
            PhpMixed::Array(
                platform_dev_reqs
                    .iter()
                    .map(|(k, v)| (k.clone(), PhpMixed::String(v.clone())))
                    .collect(),
            ),
        );
        if !platform_overrides.is_empty() {
            lock.insert(
                "platform-overrides".to_string(),
                PhpMixed::Array(platform_overrides.into_iter().collect()),
            );
        }
        lock.insert(
            "plugin-api-version".to_string(),
            PhpMixed::String(plugin_interface::PLUGIN_API_VERSION.to_string()),
        );

        let lock = self.fixup_json_data_type(lock);

        let is_locked = match self.is_locked_result() {
            Ok(b) => b,
            Err(e) => {
                if e.downcast_ref::<ParsingException>().is_some() {
                    false
                } else {
                    return Err(e);
                }
            }
        };
        let current_data = if is_locked {
            self.get_lock_data().ok()
        } else {
            None
        };
        let differs = current_data
            .as_ref()
            .map(|c| !c.iter().eq(lock.iter()))
            .unwrap_or(true);
        if !is_locked || differs {
            if write {
                self.lock_file
                    .write(PhpMixed::Array(lock.into_iter().collect()))?;
                *self.lock_data_cache.borrow_mut() = None;
                self.virtual_file_written = false;
            } else {
                self.virtual_file_written = true;
                let parsed = JsonFile::parse_json(
                    Some(&JsonFile::encode(&PhpMixed::Array(
                        lock.into_iter().collect(),
                    ))),
                    None,
                )?;
                let parsed_map: IndexMap<String, PhpMixed> = match parsed {
                    PhpMixed::Array(m) => m.into_iter().collect(),
                    _ => IndexMap::new(),
                };
                *self.lock_data_cache.borrow_mut() = Some(parsed_map);
            }

            return Ok(true);
        }

        Ok(false)
    }

    fn is_locked_result(&mut self) -> Result<bool> {
        Ok(self.is_locked())
    }

    /// Updates the lock file's hash in-place from a given composer.json's JsonFile
    pub fn update_hash<F>(
        &mut self,
        composer_json: &JsonFile,
        data_processor: Option<F>,
    ) -> Result<()>
    where
        F: FnOnce(IndexMap<String, PhpMixed>) -> IndexMap<String, PhpMixed>,
    {
        let contents = file_get_contents(composer_json.get_path());
        let contents = match contents {
            Some(s) => s,
            None => {
                return Err(RuntimeException {
                    message: format!(
                        "Unable to read {} contents to update the lock file hash.",
                        composer_json.get_path()
                    ),
                    code: 0,
                }
                .into());
            }
        };

        let lock_mtime = filemtime(self.lock_file.get_path());
        let lock_data_php = self.lock_file.read()?;
        let mut lock_data: IndexMap<String, PhpMixed> = match lock_data_php {
            PhpMixed::Array(m) => m.into_iter().collect(),
            _ => IndexMap::new(),
        };
        lock_data.insert(
            "content-hash".to_string(),
            PhpMixed::String(Self::get_content_hash(&contents)?),
        );
        if let Some(processor) = data_processor {
            lock_data = processor(lock_data);
        }

        self.lock_file.write(PhpMixed::Array(
            self.fixup_json_data_type(lock_data).into_iter().collect(),
        ))?;
        *self.lock_data_cache.borrow_mut() = None;
        self.virtual_file_written = false;
        if let Some(mtime) = lock_mtime
            && is_int(&PhpMixed::Int(mtime))
        {
            let _ = touch2(self.lock_file.get_path(), mtime);
        }
        Ok(())
    }

    /// Ensures correct data types and ordering for the JSON lock format
    fn fixup_json_data_type(
        &self,
        mut lock_data: IndexMap<String, PhpMixed>,
    ) -> IndexMap<String, PhpMixed> {
        for key in ["stability-flags", "platform", "platform-dev"].iter() {
            let should_replace = lock_data
                .get(*key)
                .map(|v| match v {
                    PhpMixed::Array(m) => m.is_empty(),
                    _ => false,
                })
                .unwrap_or(false);
            if should_replace {
                lock_data.insert(key.to_string(), PhpMixed::Array(IndexMap::new()));
            }
        }

        if let Some(PhpMixed::Array(m)) = lock_data.get_mut("stability-flags") {
            let mut as_map: IndexMap<String, PhpMixed> =
                m.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            ksort(&mut as_map);
            *m = as_map.into_iter().collect();
        }

        lock_data
    }

    /// @param PackageInterface[] $packages
    fn lock_packages(&mut self, packages: &[PackageInterfaceHandle]) -> Result<PhpMixed> {
        let mut locked: Vec<IndexMap<String, PhpMixed>> = vec![];

        for package in packages {
            if package.as_alias().is_some() {
                continue;
            }

            let name = package.get_pretty_name();
            let version = package.get_pretty_version();

            if name.is_empty() || version.is_empty() {
                return Err(LogicException {
                    message: format!(
                        "Package \"{}\" has no version or name and can not be locked",
                        package,
                    ),
                    code: 0,
                }
                .into());
            }

            let mut spec = self.dumper.dump(package.clone());
            spec.shift_remove("version_normalized");

            // always move time to the end of the package definition
            let time = spec.get("time").cloned();
            spec.shift_remove("time");
            let time = if package.is_dev()
                && package.get_installation_source() == Some("source".to_string())
            {
                // use the exact commit time of the current reference if it's a dev package
                let pkg_time = self.get_package_time(package.clone())?;
                pkg_time.map(PhpMixed::String).or(time)
            } else {
                time
            };
            if let Some(t) = time {
                spec.insert("time".to_string(), t);
            }

            spec.shift_remove("installation-source");

            locked.push(spec);
        }

        usort(&mut locked, |a, b| {
            let comparison = strcmp(
                a.get("name").and_then(|v| v.as_string()).unwrap_or(""),
                b.get("name").and_then(|v| v.as_string()).unwrap_or(""),
            );

            if 0 != comparison {
                return comparison;
            }

            // If it is the same package, compare the versions to make the order deterministic
            strcmp(
                a.get("version").and_then(|v| v.as_string()).unwrap_or(""),
                b.get("version").and_then(|v| v.as_string()).unwrap_or(""),
            )
        });

        Ok(PhpMixed::List(
            locked
                .into_iter()
                .map(|m| PhpMixed::Array(m.into_iter().collect()))
                .collect(),
        ))
    }

    /// Returns the packages's datetime for its source reference.
    fn get_package_time(&mut self, package: PackageInterfaceHandle) -> Result<Option<String>> {
        if !function_exists("proc_open") {
            return Ok(None);
        }

        let path = self
            .installation_manager
            .borrow_mut()
            .get_install_path(package.clone());
        if path.is_none() {
            return Ok(None);
        }
        let path = realpath(&path.unwrap());
        let source_type = package.get_source_type();
        let mut datetime: Option<chrono::DateTime<chrono::Utc>> = None;

        if path.is_some()
            && in_array(
                PhpMixed::String(source_type.clone().unwrap_or_default()),
                &PhpMixed::List(vec![
                    PhpMixed::String("git".to_string()),
                    PhpMixed::String("hg".to_string()),
                ]),
                false,
            )
        {
            let source_ref = package
                .get_source_reference()
                .or_else(|| package.get_dist_reference())
                .unwrap_or_default();
            match source_type.as_deref().unwrap_or("") {
                "git" => {
                    GitUtil::clean_env(&self.process);

                    let no_show_signature_flags =
                        GitUtil::get_no_show_signature_flags(&self.process);
                    let mut args: Vec<String> = vec![
                        "-n1".to_string(),
                        "--format=%ct".to_string(),
                        source_ref.clone(),
                    ];
                    args.extend(no_show_signature_flags);
                    let command = GitUtil::build_rev_list_command(&self.process, args);
                    let mut output = PhpMixed::Null;
                    if 0 == self.process.borrow_mut().execute(
                        PhpMixed::List(command.into_iter().map(PhpMixed::String).collect()),
                        &mut output,
                        path.as_deref(),
                    )? {
                        let output_str = trim(
                            &GitUtil::parse_rev_list_output(
                                output.as_string().unwrap_or(""),
                                &self.process,
                            ),
                            None,
                        );
                        if Preg::is_match(r"{^\s*\d+\s*$}", &output_str) {
                            let ts = trim(&output_str, None).parse::<i64>().unwrap_or(0);
                            datetime = chrono::DateTime::from_timestamp(ts, 0);
                        }
                    }
                }
                "hg" => {
                    let mut output = PhpMixed::Null;
                    if 0 == self.process.borrow_mut().execute(
                        PhpMixed::List(vec![
                            PhpMixed::String("hg".to_string()),
                            PhpMixed::String("log".to_string()),
                            PhpMixed::String("--template".to_string()),
                            PhpMixed::String("{date|hgdate}".to_string()),
                            PhpMixed::String("-r".to_string()),
                            PhpMixed::String(source_ref.clone()),
                        ]),
                        &mut output,
                        path.as_deref(),
                    )? {
                        let mut m: IndexMap<CaptureKey, String> = IndexMap::new();
                        if Preg::is_match3(
                            r"{^\s*(\d+)\s*}",
                            output.as_string().unwrap_or(""),
                            Some(&mut m),
                        ) {
                            let ts = m
                                .get(&CaptureKey::ByIndex(1))
                                .cloned()
                                .unwrap_or_default()
                                .parse::<i64>()
                                .unwrap_or(0);
                            datetime = chrono::DateTime::from_timestamp(ts, 0);
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(datetime.map(|d| d.format(DATE_RFC3339).to_string()))
    }

    /// @return array<string>
    pub fn get_missing_requirement_info(
        &mut self,
        package: RootPackageInterfaceHandle,
        include_dev: bool,
    ) -> Result<Vec<String>> {
        let mut missing_requirement_info: Vec<String> = vec![];
        let mut missing_requirements = false;
        let mut sets: Vec<SetEntry> = vec![SetEntry {
            repo: self.get_locked_repository(false)?,
            method: "getRequires".to_string(),
            description: "Required".to_string(),
        }];
        if include_dev {
            sets.push(SetEntry {
                repo: self.get_locked_repository(true)?,
                method: "getDevRequires".to_string(),
                description: "Required (in require-dev)".to_string(),
            });
        }
        let root_repo = RootPackageRepository::new(RootPackageInterfaceHandle::dup(&package));

        for set in &sets {
            let installed_repo = InstalledRepository::new(vec![/* set.repo, root_repo */]);

            // PHP: call_user_func([$package, $set['method']])
            let links = match set.method.as_str() {
                "getRequires" => package.get_requires(),
                "getDevRequires" => package.get_dev_requires(),
                _ => unreachable!(),
            };
            for link in links.values() {
                if PlatformRepository::is_platform_package(link.get_target()) {
                    continue;
                }
                if link.get_pretty_constraint() == "self.version" {
                    continue;
                }
                if installed_repo
                    .find_packages_with_replacers_and_providers(
                        link.get_target(),
                        Some(FindPackageConstraint::Constraint(
                            link.get_constraint().clone(),
                        )),
                    )?
                    .is_empty()
                {
                    let results = installed_repo
                        .find_packages_with_replacers_and_providers(link.get_target(), None)?;

                    if !results.is_empty() {
                        // PHP `reset($results)` returns the first shared package; clone the handle.
                        let provider: BasePackageHandle = results.first().unwrap().clone();
                        let _ = &results;
                        let mut description = provider.get_pretty_version();
                        if provider.get_name() != link.get_target() {
                            'outer: for (method, verb) in
                                [("getReplaces", "replaced"), ("getProvides", "provided")].iter()
                            {
                                let provider_links = match *method {
                                    "getReplaces" => provider.get_replaces(),
                                    "getProvides" => provider.get_provides(),
                                    _ => unreachable!(),
                                };
                                for provider_link in provider_links.values() {
                                    if provider_link.get_target() == link.get_target() {
                                        description = format!(
                                            "{} as {} by {} {}",
                                            verb,
                                            provider_link.get_pretty_constraint(),
                                            provider.get_pretty_name(),
                                            provider.get_pretty_version(),
                                        );
                                        break 'outer;
                                    }
                                }
                            }
                        }
                        missing_requirement_info.push(format!(
                            "- {} package \"{}\" is in the lock file as \"{}\" but that does not satisfy your constraint \"{}\".",
                            set.description,
                            link.get_target(),
                            description,
                            link.get_pretty_constraint(),
                        ));
                    } else {
                        missing_requirement_info.push(format!(
                            "- {} package \"{}\" is not present in the lock file.",
                            set.description,
                            link.get_target()
                        ));
                    }
                    missing_requirements = true;
                }
            }
            let _ = root_repo;
            let _ = installed_repo;
        }

        if missing_requirements {
            missing_requirement_info.push("This usually happens when composer files are incorrectly merged or the composer.json file is manually edited.".to_string());
            missing_requirement_info.push("Read more about correctly resolving merge conflicts https://getcomposer.org/doc/articles/resolving-merge-conflicts.md".to_string());
            missing_requirement_info.push("and prefer using the \"require\" command over editing the composer.json file directly https://getcomposer.org/doc/03-cli.md#require-r".to_string());
        }

        Ok(missing_requirement_info)
    }
}

struct SetEntry {
    repo: LockArrayRepositoryHandle,
    method: String,
    description: String,
}
