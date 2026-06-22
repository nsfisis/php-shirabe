//! ref: composer/src/Composer/Repository/FilesystemRepository.php

use std::any::Any;

use crate::util::Silencer;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_php_shim::{
    Exception, InvalidArgumentException, LogicException, PhpMixed, SORT_NATURAL,
    UnexpectedValueException, array_flip, dirname, r#eval, file_get_contents, get_class,
    get_class_err, get_debug_type, in_array, is_array, is_null, is_string, ksort, realpath, sort,
    sort_with_flags, str_repeat, strtr, trim, usort, var_export,
};

use crate::config::is_php_integer_key;
use crate::installed_versions::InstalledVersions;
use crate::installer::InstallationManager;
use crate::json::JsonFile;
use crate::package::BasePackageHandle;
use crate::package::PackageInterfaceHandle;
use crate::package::RootPackageInterfaceHandle;
use crate::package::dumper::ArrayDumper;
use crate::package::loader::ArrayLoader;
use crate::package::loader::LoaderInterface;
use crate::repository::InvalidRepositoryException;
use crate::repository::PlatformRepository;
use crate::repository::RepositoryInterface;
use crate::repository::WritableArrayRepository;
use crate::repository::{FindPackageConstraint, LoadPackagesResult, ProviderInfo, SearchResult};
use crate::util::Filesystem;
use crate::util::Platform;
use shirabe_semver::constraint::AnyConstraint;

/// Filesystem repository.
#[derive(Debug)]
pub struct FilesystemRepository {
    pub(crate) inner: WritableArrayRepository,
    /// @var JsonFile
    pub(crate) file: JsonFile,
    /// @var bool
    dump_versions: bool,
    /// @var ?RootPackageInterface
    root_package: Option<RootPackageInterfaceHandle>,
    /// @var Filesystem
    filesystem: std::rc::Rc<std::cell::RefCell<Filesystem>>,
    /// @var bool|null
    dev_mode: Option<bool>,
}

impl FilesystemRepository {
    /// Initializes filesystem repository.
    ///
    /// @param JsonFile              $repositoryFile repository json file
    /// @param ?RootPackageInterface $rootPackage    Must be provided if $dumpVersions is true
    pub fn new(
        repository_file: JsonFile,
        dump_versions: bool,
        root_package: Option<RootPackageInterfaceHandle>,
        filesystem: Option<std::rc::Rc<std::cell::RefCell<Filesystem>>>,
    ) -> Result<Self> {
        let filesystem = filesystem
            .unwrap_or_else(|| std::rc::Rc::new(std::cell::RefCell::new(Filesystem::new(None))));
        if dump_versions && root_package.is_none() {
            return Err(InvalidArgumentException {
                message: "Expected a root package instance if $dumpVersions is true".to_string(),
                code: 0,
            }
            .into());
        }
        Ok(Self {
            inner: WritableArrayRepository::new(vec![])?,
            file: repository_file,
            dump_versions,
            root_package,
            filesystem,
            dev_mode: None,
        })
    }

    /// @return bool|null true if dev requirements were installed, false if --no-dev was used, null if yet unknown
    pub fn get_dev_mode(&self) -> Option<bool> {
        self.dev_mode
    }

    pub fn set_self_handle(&self, weak: crate::repository::RepositoryInterfaceWeakHandle) {
        self.inner.set_self_handle(weak);
    }

    pub fn get_repo_name(&self) -> String {
        self.inner.get_repo_name()
    }

    fn ensure_initialized(&mut self) -> Result<()> {
        if !self.inner.is_initialized() {
            self.initialize()?;
        }
        Ok(())
    }

    /// Initializes repository (reads file, or remote address).
    pub(crate) fn initialize(&mut self) -> Result<()> {
        self.inner.initialize();

        if !self.file.exists() {
            return Ok(());
        }

        let packages: PhpMixed = match (|| -> Result<PhpMixed> {
            let data = self.file.read()?;
            let packages_value = if let PhpMixed::Array(ref m) = data {
                if m.contains_key("packages") {
                    m.get("packages").unwrap().clone()
                } else {
                    data.clone()
                }
            } else {
                data.clone()
            };

            if let PhpMixed::Array(ref m) = data {
                if let Some(names) = m.get("dev-package-names") {
                    let dev_names: Vec<String> = names
                        .as_list()
                        .map(|l| {
                            l.iter()
                                .filter_map(|v| v.as_string().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();
                    self.inner.set_dev_package_names(dev_names);
                }
                if let Some(dev) = m.get("dev") {
                    self.dev_mode = dev.as_bool();
                }
            }

            if !is_array(&packages_value) {
                return Err(UnexpectedValueException {
                    message: "Could not parse package list from the repository".to_string(),
                    code: 0,
                }
                .into());
            }

            Ok(packages_value)
        })() {
            Ok(p) => p,
            Err(e) => {
                return Err(InvalidRepositoryException(Exception {
                    message: format!(
                        "Invalid repository data in {}, packages could not be loaded: [{}] {}",
                        self.file.get_path(),
                        get_class_err(&e),
                        e,
                    ),
                    code: 0,
                })
                .into());
            }
        };

        let mut loader = ArrayLoader::new(None, true);
        if let Some(packages_list) = packages.as_list() {
            for package_data in packages_list.iter() {
                let cfg = package_data.as_array().cloned().unwrap_or_default();
                let package =
                    loader.load(cfg, Some("Composer\\Package\\CompletePackage".to_string()))?;
                self.inner.add_package(package)?;
            }
        } else if let Some(packages_array) = packages.as_array() {
            for (_, package_data) in packages_array.iter() {
                let cfg = package_data.as_array().cloned().unwrap_or_default();
                let package =
                    loader.load(cfg, Some("Composer\\Package\\CompletePackage".to_string()))?;
                self.inner.add_package(package)?;
            }
        }

        Ok(())
    }

    pub fn reload(&mut self) -> Result<()> {
        self.inner.reset_packages();
        self.initialize()
    }

    pub fn add_package(&mut self, package: PackageInterfaceHandle) -> Result<()> {
        self.inner.add_package(package)
    }

    pub fn remove_package(&mut self, package: PackageInterfaceHandle) -> Result<()> {
        self.inner.remove_package(package)
    }

    pub fn get_canonical_packages(&mut self) -> Result<Vec<PackageInterfaceHandle>> {
        self.ensure_initialized()?;
        Ok(self.inner.get_canonical_packages())
    }

    pub fn set_dev_package_names(&mut self, dev_package_names: Vec<String>) {
        self.inner.set_dev_package_names(dev_package_names);
    }

    pub fn get_dev_package_names(&self) -> &Vec<String> {
        self.inner.get_dev_package_names()
    }

    /// Writes writable repository.
    pub fn write(
        &mut self,
        dev_mode: bool,
        installation_manager: &mut InstallationManager,
    ) -> Result<()> {
        let mut data: IndexMap<String, PhpMixed> = IndexMap::new();
        data.insert("packages".to_string(), PhpMixed::List(vec![]));
        data.insert("dev".to_string(), PhpMixed::Bool(dev_mode));
        data.insert("dev-package-names".to_string(), PhpMixed::List(vec![]));

        let dumper = ArrayDumper::new();

        // make sure the directory is created so we can realpath it
        // as realpath() does some additional normalizations with network paths that normalizePath does not
        // and we need to find shortest path correctly
        let repo_dir = dirname(self.file.get_path());
        self.filesystem
            .borrow_mut()
            .ensure_directory_exists(&repo_dir);

        let repo_dir = self
            .filesystem
            .borrow()
            .normalize_path(&realpath(&repo_dir).unwrap_or_default());
        let mut install_paths: IndexMap<String, Option<String>> = IndexMap::new();

        for package in self.inner.get_canonical_packages() {
            let mut pkg_array = dumper.dump(package.clone());
            let path = installation_manager.get_install_path(package.clone());
            let mut install_path: Option<String> = None;
            if let Some(path_str) = &path
                && !path_str.is_empty()
            {
                let normalized_path = self.filesystem.borrow_mut().normalize_path(&if self
                    .filesystem
                    .borrow()
                    .is_absolute_path(path_str)
                {
                    path_str.clone()
                } else {
                    format!(
                        "{}/{}",
                        Platform::get_cwd(false).unwrap_or_default(),
                        path_str
                    )
                });
                install_path = Some(self.filesystem.borrow_mut().find_shortest_path(
                    &repo_dir,
                    &normalized_path,
                    true,
                    false,
                ));
            }
            install_paths.insert(package.get_name().to_string(), install_path.clone());

            pkg_array.insert(
                "install-path".to_string(),
                match install_path {
                    Some(s) => PhpMixed::String(s),
                    None => PhpMixed::Null,
                },
            );
            if let Some(PhpMixed::List(list)) = data.get_mut("packages") {
                list.push(PhpMixed::Array(pkg_array.into_iter().collect()));
            }

            // only write to the files the names which are really installed, as we receive the full list
            // of dev package names before they get installed during composer install
            if in_array(
                PhpMixed::String(package.get_name().to_string()),
                &PhpMixed::List(
                    self.inner
                        .dev_package_names
                        .iter()
                        .map(|s| PhpMixed::String(s.clone()))
                        .collect(),
                ),
                true,
            ) && let Some(PhpMixed::List(list)) = data.get_mut("dev-package-names")
            {
                list.push(PhpMixed::String(package.get_name().to_string()));
            }
        }

        // PHP: sort($data['dev-package-names']);
        if let Some(PhpMixed::List(list)) = data.get_mut("dev-package-names") {
            usort(list, |a: &PhpMixed, b: &PhpMixed| -> i64 {
                shirabe_php_shim::strcmp(a.as_string().unwrap_or(""), b.as_string().unwrap_or(""))
            });
        }
        // PHP: usort($data['packages'], static function ($a, $b): int { return strcmp($a['name'], $b['name']); });
        if let Some(PhpMixed::List(list)) = data.get_mut("packages") {
            usort(list, |a: &PhpMixed, b: &PhpMixed| -> i64 {
                let a_name = a
                    .as_array()
                    .and_then(|m| m.get("name"))
                    .and_then(|v| v.as_string())
                    .unwrap_or("");
                let b_name = b
                    .as_array()
                    .and_then(|m| m.get("name"))
                    .and_then(|v| v.as_string())
                    .unwrap_or("");
                shirabe_php_shim::strcmp(a_name, b_name)
            });
        }

        self.file
            .write(PhpMixed::Array(data.clone().into_iter().collect()))?;

        if self.dump_versions {
            let versions = self.generate_installed_versions(
                installation_manager,
                &install_paths,
                dev_mode,
                &repo_dir,
            )?;

            self.filesystem.borrow_mut().file_put_contents_if_modified(
                &format!("{}/installed.php", repo_dir),
                &format!("<?php return {};\n", self.dump_to_php_code(&versions, 0),),
            );
            self.filesystem.borrow_mut().file_put_contents_if_modified(
                &format!("{}/InstalledVersions.php", repo_dir),
                include_str!("../../../../composer/src/Composer/InstalledVersions.php"),
            );

            // make sure the in memory state is up to date with on disk
            InstalledVersions::reload(versions);

            // make sure the selfDir matches the expected data at runtime if the class was loaded from the vendor dir, as it may have been
            // loaded from the Composer sources, causing packages to appear twice in that case if the installed.php is loaded in addition to the
            // in memory loaded data from above
            InstalledVersions::set_self_dir(repo_dir.replace('\\', "/"));
            InstalledVersions::set_installed_is_local_dir(true);
        }

        Ok(())
    }

    /// As we load the file from vendor dir during bootstrap, we need to make sure it contains only expected code before executing it
    ///
    /// @internal
    pub fn safely_load_installed_versions(path: &str) -> bool {
        // PHP: @file_get_contents($path)
        let installed_versions_data = Silencer::call(|| Ok(file_get_contents(path)))
            .ok()
            .flatten();
        let pattern = "{(?(DEFINE)\n   (?<number>  -? \\s*+ \\d++ (?:\\.\\d++)? )\n   (?<boolean> true | false | null )\n   (?<strings> (?&string) (?: \\s*+ \\. \\s*+ (?&string))*+ )\n   (?<string>  (?: \" (?:[^\"\\\\$]*+ | \\\\ [\"\\\\0] )* \" | ' (?:[^'\\\\]*+ | \\\\ ['\\\\] )* ' ) )\n   (?<array>   array\\( \\s*+ (?: (?:(?&number)|(?&strings)) \\s*+ => \\s*+ (?: (?:__DIR__ \\s*+ \\. \\s*+)? (?&strings) | (?&value) ) \\s*+, \\s*+ )*+  \\s*+ \\) )\n   (?<value>   (?: (?&number) | (?&boolean) | (?&strings) | (?&array) ) )\n)\n^<\\?php\\s++return\\s++(?&array)\\s*+;$}ix";
        if let Some(data) = installed_versions_data {
            let mixed = PhpMixed::String(data.clone());
            if is_string(&mixed) && Preg::is_match(pattern, &trim(&data, None)) {
                let replaced = Preg::replace(
                    r#"{=>\s*+__DIR__\s*+\.\s*+(['\"])}"#,
                    &format!(
                        "=> {} . $1",
                        var_export(&PhpMixed::String(dirname(path)), true),
                    ),
                    &data,
                );
                let evaluated = r#eval(&format!("?>{}", replaced));
                InstalledVersions::reload(
                    evaluated
                        .as_array()
                        .cloned()
                        .map(|m| m.into_iter().collect())
                        .unwrap_or_default(),
                );

                return true;
            }
        }

        false
    }

    /// @param array<mixed> $array
    fn dump_to_php_code(&self, array: &IndexMap<String, PhpMixed>, level: i64) -> String {
        let mut lines = String::from("array(\n");
        let level = level + 1;

        for (key, value) in array {
            lines.push_str(&str_repeat("    ", level as usize));
            lines.push_str(&if is_php_integer_key(key) {
                format!("{} => ", key)
            } else {
                format!("{} => ", var_export(&PhpMixed::String(key.clone()), true))
            });

            if is_array(value) {
                if let Some(inner_arr) = value.as_array() {
                    if !inner_arr.is_empty() {
                        let inner_map: IndexMap<String, PhpMixed> = inner_arr
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect();
                        lines.push_str(&self.dump_to_php_code(&inner_map, level));
                    } else {
                        lines.push_str("array(),\n");
                    }
                } else if let Some(list) = value.as_list() {
                    if !list.is_empty() {
                        let inner_map: IndexMap<String, PhpMixed> = list
                            .iter()
                            .enumerate()
                            .map(|(i, v)| (i.to_string(), v.clone()))
                            .collect();
                        lines.push_str(&self.dump_to_php_code(&inner_map, level));
                    } else {
                        lines.push_str("array(),\n");
                    }
                }
            } else if key == "install_path" && is_string(value) {
                let s = value.as_string().unwrap_or("").to_string();
                if self.filesystem.borrow_mut().is_absolute_path(&s) {
                    lines.push_str(&format!("{},\n", var_export(&PhpMixed::String(s), true),));
                } else {
                    lines.push_str(&format!(
                        "__DIR__ . {},\n",
                        var_export(&PhpMixed::String(format!("/{}", s)), true),
                    ));
                }
            } else if is_string(value) {
                lines.push_str(&format!("{},\n", var_export(value, true)));
            } else if let PhpMixed::Bool(b) = value {
                lines.push_str(&format!("{},\n", if *b { "true" } else { "false" }));
            } else if is_null(value) {
                lines.push_str("null,\n");
            } else {
                // PHP: throw new \UnexpectedValueException('Unexpected type '.get_debug_type($value));
                panic!("{}", format!("Unexpected type {}", get_debug_type(value)));
            }
        }

        lines.push_str(&format!(
            "{}){}",
            str_repeat("    ", (level - 1) as usize),
            if (level - 1) == 0 { "" } else { ",\n" },
        ));

        lines
    }

    /// @param array<string, string> $installPaths
    fn generate_installed_versions(
        &mut self,
        installation_manager: &InstallationManager,
        install_paths: &IndexMap<String, Option<String>>,
        dev_mode: bool,
        repo_dir: &str,
    ) -> Result<IndexMap<String, PhpMixed>> {
        let dev_packages = array_flip(&PhpMixed::List(
            self.inner
                .dev_package_names
                .iter()
                .map(|s| PhpMixed::String(s.clone()))
                .collect(),
        ));
        let mut packages: Vec<PackageInterfaceHandle> =
            self.inner.get_packages()?.into_iter().collect();
        let mut current_root: RootPackageInterfaceHandle = match &self.root_package {
            None => {
                return Err(LogicException {
                    message:
                        "It should not be possible to dump packages if no root package is given"
                            .to_string(),
                    code: 0,
                }
                .into());
            }
            Some(r) => r.clone(),
        };
        // packages[] = $rootPackage = $this->rootPackage;
        packages.push(current_root.clone().into());

        while let Some(root_alias) =
            PackageInterfaceHandle::from(current_root.clone()).as_root_alias_package()
        {
            current_root = root_alias.get_alias_of().into();
            packages.push(current_root.clone().into());
        }
        let mut versions: IndexMap<String, PhpMixed> = IndexMap::new();
        versions.insert(
            "root".to_string(),
            PhpMixed::Array(
                self.dump_root_package(
                    current_root.clone(),
                    install_paths,
                    dev_mode,
                    repo_dir,
                    &dev_packages,
                )
                .into_iter()
                .collect(),
            ),
        );
        versions.insert("versions".to_string(), PhpMixed::Array(IndexMap::new()));

        // add real installed packages
        for package in &packages {
            if package.as_alias().is_some() {
                continue;
            }

            let dumped = self.dump_installed_package(
                package.clone(),
                install_paths,
                repo_dir,
                &dev_packages,
            );
            if let Some(PhpMixed::Array(versions_map)) = versions.get_mut("versions") {
                versions_map.insert(
                    package.get_name().to_string(),
                    PhpMixed::Array(dumped.into_iter().collect()),
                );
            }
        }

        // add provided/replaced packages
        for package in &packages {
            let is_dev_package = dev_packages
                .as_array()
                .map(|m| m.contains_key(&package.get_name()))
                .unwrap_or(false);
            for (_, replace) in package.get_replaces() {
                // exclude platform replaces as when they are really there we can not check for their presence
                if PlatformRepository::is_platform_package(replace.get_target()) {
                    continue;
                }
                let mut replaced = replace.get_pretty_constraint().to_string();
                if replaced == "self.version" {
                    replaced = package.get_pretty_version().to_string();
                }
                record_replace_or_provide(
                    &mut versions,
                    replace.get_target(),
                    "replaced",
                    replaced,
                    is_dev_package,
                );
            }
            for (_, provide) in package.get_provides() {
                // exclude platform provides as when they are really there we can not check for their presence
                if PlatformRepository::is_platform_package(provide.get_target()) {
                    continue;
                }
                let mut provided = provide.get_pretty_constraint().to_string();
                if provided == "self.version" {
                    provided = package.get_pretty_version().to_string();
                }
                record_replace_or_provide(
                    &mut versions,
                    provide.get_target(),
                    "provided",
                    provided,
                    is_dev_package,
                );
            }
        }

        // add aliases
        for package in &packages {
            if package.as_alias().is_none() {
                continue;
            }
            let pretty = package.get_pretty_version().to_string();
            push_to_list(
                versions_entry(&mut versions, &package.get_name()),
                "aliases",
                pretty.clone(),
            );
            if package.as_root().is_some()
                && let Some(PhpMixed::Array(root_map)) = versions.get_mut("root")
            {
                push_to_list(root_map, "aliases", pretty);
            }
        }

        if let Some(PhpMixed::Array(versions_map)) = versions.get_mut("versions") {
            ksort(versions_map);
        }
        ksort(&mut versions);

        if let Some(PhpMixed::Array(versions_map)) = versions.get_mut("versions") {
            for (_name, version) in versions_map.iter_mut() {
                if let PhpMixed::Array(version_map) = version {
                    for key in ["aliases", "replaced", "provided"] {
                        if let Some(entry) = version_map.get_mut(key)
                            && let PhpMixed::List(list) = entry
                        {
                            // PHP: sort($versions['versions'][$name][$key], SORT_NATURAL);
                            usort(list, |a: &PhpMixed, b: &PhpMixed| -> i64 {
                                shirabe_php_shim::strnatcmp(
                                    a.as_string().unwrap_or(""),
                                    b.as_string().unwrap_or(""),
                                )
                            });
                        }
                    }
                }
            }
        }

        Ok(versions)
    }

    /// @param array<string, string> $installPaths
    /// @param array<string, int> $devPackages
    /// @return array{pretty_version: string, version: string, reference: string|null, type: string, install_path: string, aliases: string[], dev_requirement: bool}
    fn dump_installed_package(
        &self,
        package: PackageInterfaceHandle,
        install_paths: &IndexMap<String, Option<String>>,
        repo_dir: &str,
        dev_packages: &PhpMixed,
    ) -> IndexMap<String, PhpMixed> {
        let mut reference: Option<String> = None;
        if let Some(install_src) = package.get_installation_source() {
            reference = if install_src == "source" {
                package.get_source_reference()
            } else {
                package.get_dist_reference()
            };
        }
        if reference.is_none() {
            // PHP: ($package->getSourceReference() ?: $package->getDistReference()) ?: null;
            let source = package.get_source_reference().unwrap_or_default();
            let dist = package.get_dist_reference().unwrap_or_default();
            let combined = if !source.is_empty() {
                source.to_string()
            } else {
                dist.to_string()
            };
            reference = if combined.is_empty() {
                None
            } else {
                Some(combined)
            };
        }

        let install_path = if package.as_root().is_some() {
            let to = self.filesystem.borrow_mut().normalize_path(
                &realpath(&Platform::get_cwd(false).unwrap_or_default()).unwrap_or_default(),
            );
            Some(
                self.filesystem
                    .borrow_mut()
                    .find_shortest_path(repo_dir, &to, true, false),
            )
        } else {
            install_paths.get(&package.get_name()).cloned().flatten()
        };

        let mut data: IndexMap<String, PhpMixed> = IndexMap::new();
        data.insert(
            "pretty_version".to_string(),
            PhpMixed::String(package.get_pretty_version().to_string()),
        );
        data.insert(
            "version".to_string(),
            PhpMixed::String(package.get_version().to_string()),
        );
        data.insert(
            "reference".to_string(),
            match reference {
                Some(s) => PhpMixed::String(s),
                None => PhpMixed::Null,
            },
        );
        data.insert(
            "type".to_string(),
            PhpMixed::String(package.get_type().to_string()),
        );
        data.insert(
            "install_path".to_string(),
            match install_path {
                Some(s) => PhpMixed::String(s),
                None => PhpMixed::Null,
            },
        );
        data.insert("aliases".to_string(), PhpMixed::List(vec![]));
        data.insert(
            "dev_requirement".to_string(),
            PhpMixed::Bool(
                dev_packages
                    .as_array()
                    .map(|m| m.contains_key(&package.get_name()))
                    .unwrap_or(false),
            ),
        );

        data
    }

    /// @param array<string, string> $installPaths
    /// @param array<string, int> $devPackages
    /// @return array{name: string, pretty_version: string, version: string, reference: string|null, type: string, install_path: string, aliases: string[], dev: bool}
    fn dump_root_package(
        &self,
        package: RootPackageInterfaceHandle,
        install_paths: &IndexMap<String, Option<String>>,
        dev_mode: bool,
        repo_dir: &str,
        dev_packages: &PhpMixed,
    ) -> IndexMap<String, PhpMixed> {
        let data = self.dump_installed_package(
            package.clone().into(),
            install_paths,
            repo_dir,
            dev_packages,
        );

        let mut result: IndexMap<String, PhpMixed> = IndexMap::new();
        result.insert(
            "name".to_string(),
            PhpMixed::String(package.get_name().to_string()),
        );
        result.insert(
            "pretty_version".to_string(),
            data.get("pretty_version")
                .cloned()
                .unwrap_or(PhpMixed::Null),
        );
        result.insert(
            "version".to_string(),
            data.get("version").cloned().unwrap_or(PhpMixed::Null),
        );
        result.insert(
            "reference".to_string(),
            data.get("reference").cloned().unwrap_or(PhpMixed::Null),
        );
        result.insert(
            "type".to_string(),
            data.get("type").cloned().unwrap_or(PhpMixed::Null),
        );
        result.insert(
            "install_path".to_string(),
            data.get("install_path").cloned().unwrap_or(PhpMixed::Null),
        );
        result.insert(
            "aliases".to_string(),
            data.get("aliases")
                .cloned()
                .unwrap_or(PhpMixed::List(vec![])),
        );
        result.insert("dev".to_string(), PhpMixed::Bool(dev_mode));

        result
    }
}

impl RepositoryInterface for FilesystemRepository {
    fn count(&self) -> Result<usize> {
        self.inner.count()
    }

    fn has_package(&self, package: PackageInterfaceHandle) -> bool {
        self.inner.has_package(package)
    }

    fn find_package(
        &mut self,
        name: &str,
        constraint: FindPackageConstraint,
    ) -> Result<Option<BasePackageHandle>> {
        self.ensure_initialized()?;
        self.inner.find_package(name, constraint)
    }

    fn find_packages(
        &mut self,
        name: &str,
        constraint: Option<FindPackageConstraint>,
    ) -> Result<Vec<BasePackageHandle>> {
        self.ensure_initialized()?;
        self.inner.find_packages(name, constraint)
    }

    fn get_packages(&mut self) -> Result<Vec<BasePackageHandle>> {
        self.ensure_initialized()?;
        self.inner.get_packages()
    }

    fn load_packages(
        &mut self,
        package_name_map: IndexMap<String, Option<AnyConstraint>>,
        acceptable_stabilities: IndexMap<String, i64>,
        stability_flags: IndexMap<String, i64>,
        already_loaded: IndexMap<String, IndexMap<String, PackageInterfaceHandle>>,
    ) -> Result<LoadPackagesResult> {
        self.ensure_initialized()?;
        self.inner.load_packages(
            package_name_map,
            acceptable_stabilities,
            stability_flags,
            already_loaded,
        )
    }

    fn search(
        &mut self,
        query: String,
        mode: i64,
        r#type: Option<String>,
    ) -> Result<Vec<SearchResult>> {
        self.ensure_initialized()?;
        self.inner.search(query, mode, r#type)
    }

    fn get_providers(&mut self, package_name: String) -> Result<IndexMap<String, ProviderInfo>> {
        self.ensure_initialized()?;
        self.inner.get_providers(package_name)
    }

    fn get_repo_name(&self) -> String {
        self.inner.get_repo_name()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn set_self_handle(&self, weak: crate::repository::RepositoryInterfaceWeakHandle) {
        self.inner.set_self_handle(weak);
    }
}

fn versions_entry<'a>(
    versions: &'a mut IndexMap<String, PhpMixed>,
    target: &str,
) -> &'a mut IndexMap<String, PhpMixed> {
    let versions_map = match versions.get_mut("versions") {
        Some(PhpMixed::Array(m)) => m,
        _ => unreachable!("versions['versions'] is always an array"),
    };
    match versions_map
        .entry(target.to_string())
        .or_insert_with(|| PhpMixed::Array(IndexMap::new()))
    {
        PhpMixed::Array(m) => m,
        _ => unreachable!("versions['versions'][target] is always an array"),
    }
}

fn push_to_list(entry: &mut IndexMap<String, PhpMixed>, key: &str, value: String) {
    if let PhpMixed::List(list) = entry
        .entry(key.to_string())
        .or_insert_with(|| PhpMixed::List(vec![]))
    {
        list.push(PhpMixed::String(value));
    }
}

fn record_replace_or_provide(
    versions: &mut IndexMap<String, PhpMixed>,
    target: &str,
    key: &str,
    value: String,
    is_dev_package: bool,
) {
    let entry = versions_entry(versions, target);
    if !entry.contains_key("dev_requirement") {
        entry.insert(
            "dev_requirement".to_string(),
            PhpMixed::Bool(is_dev_package),
        );
    } else if !is_dev_package {
        entry.insert("dev_requirement".to_string(), PhpMixed::Bool(false));
    }
    let already_present = match entry.get(key) {
        Some(b) => matches!(
            b,
            PhpMixed::List(list) if list.iter().any(|v| v.as_string() == Some(value.as_str()))
        ),
        None => false,
    };
    if !already_present {
        push_to_list(entry, key, value);
    }
}
