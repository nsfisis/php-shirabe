//! ref: composer/src/Composer/Repository/FilesystemRepository.php

use std::any::Any;

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{
    InvalidArgumentException, LogicException, PhpMixed, SORT_NATURAL, UnexpectedValueException,
    array_flip, dirname, r#eval, file_get_contents, get_class, get_debug_type, in_array, is_array,
    is_int, is_null, is_string, ksort, php_dir, realpath, sort, sort_with_flags, str_repeat, strtr,
    trim, usort, var_export,
};
use crate::util::silencer::Silencer;

use crate::installed_versions::InstalledVersions;
use crate::installer::installation_manager::InstallationManager;
use crate::json::json_file::JsonFile;
use crate::package::alias_package::AliasPackage;
use crate::package::dumper::array_dumper::ArrayDumper;
use crate::package::loader::array_loader::ArrayLoader;
use crate::package::package_interface::PackageInterface;
use crate::package::root_alias_package::RootAliasPackage;
use crate::package::root_package_interface::RootPackageInterface;
use crate::repository::invalid_repository_exception::InvalidRepositoryException;
use crate::repository::platform_repository::PlatformRepository;
use crate::repository::writable_array_repository::WritableArrayRepository;
use crate::util::filesystem::Filesystem;
use crate::util::platform::Platform;

/// Filesystem repository.
#[derive(Debug)]
pub struct FilesystemRepository {
    pub(crate) inner: WritableArrayRepository,
    /// @var JsonFile
    pub(crate) file: JsonFile,
    /// @var bool
    dump_versions: bool,
    /// @var ?RootPackageInterface
    root_package: Option<Box<dyn RootPackageInterface>>,
    /// @var Filesystem
    filesystem: Filesystem,
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
        root_package: Option<Box<dyn RootPackageInterface>>,
        filesystem: Option<Filesystem>,
    ) -> Result<Self> {
        let filesystem = filesystem.unwrap_or_else(Filesystem::new);
        if dump_versions && root_package.is_none() {
            return Err(InvalidArgumentException {
                message: "Expected a root package instance if $dumpVersions is true".to_string(),
                code: 0,
            }
            .into());
        }
        Ok(Self {
            // TODO(phase-b): WritableArrayRepository::new() needs to be exposed
            inner: todo!("WritableArrayRepository::new()"),
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

    /// Initializes repository (reads file, or remote address).
    pub(crate) fn initialize(&mut self) -> Result<()> {
        self.inner.initialize();

        if !self.file.exists() {
            return Ok(());
        }

        // TODO(phase-b): use anyhow::Result<Result<T, E>> to model PHP try/catch
        let packages: PhpMixed = match (|| -> Result<PhpMixed> {
            let data = self.file.read()?;
            let packages_value = if let PhpMixed::Array(ref m) = data {
                if m.contains_key("packages") {
                    (*m.get("packages").unwrap().clone()).clone()
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
                return Err(InvalidRepositoryException::new(format!(
                    "Invalid repository data in {}, packages could not be loaded: [{}] {}",
                    self.file.get_path(),
                    get_class(&e),
                    e,
                ))
                .into());
            }
        };

        let mut loader = ArrayLoader::new(None, true);
        if let Some(packages_list) = packages.as_list() {
            for package_data in packages_list.iter() {
                let package = loader.load(
                    (**package_data).clone(),
                    "Composer\\Package\\CompletePackage",
                )?;
                self.inner.add_package(package)?;
            }
        } else if let Some(packages_array) = packages.as_array() {
            for (_, package_data) in packages_array.iter() {
                let package = loader.load(
                    (**package_data).clone(),
                    "Composer\\Package\\CompletePackage",
                )?;
                self.inner.add_package(package)?;
            }
        }

        Ok(())
    }

    pub fn reload(&mut self) -> Result<()> {
        // TODO(phase-b): clear inner packages cache (PHP: $this->packages = null)
        self.inner.reload();
        self.initialize()
    }

    /// Writes writable repository.
    pub fn write(
        &mut self,
        dev_mode: bool,
        installation_manager: &InstallationManager,
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
        self.filesystem.ensure_directory_exists(&repo_dir);

        let repo_dir = self
            .filesystem
            .normalize_path(&realpath(&repo_dir).unwrap_or_default());
        let mut install_paths: IndexMap<String, Option<String>> = IndexMap::new();

        for package in self.inner.get_canonical_packages() {
            let mut pkg_array = dumper.dump(&*package);
            let path = installation_manager.get_install_path(&*package);
            let mut install_path: Option<String> = None;
            if let Some(path_str) = &path {
                if !path_str.is_empty() {
                    let normalized_path = self.filesystem.normalize_path(&if self
                        .filesystem
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
                    install_path = Some(self.filesystem.find_shortest_path(
                        &repo_dir,
                        &normalized_path,
                        true,
                    ));
                }
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
                list.push(Box::new(PhpMixed::Array(
                    pkg_array
                        .into_iter()
                        .map(|(k, v)| (k, Box::new(v)))
                        .collect(),
                )));
            }

            // only write to the files the names which are really installed, as we receive the full list
            // of dev package names before they get installed during composer install
            if in_array(
                PhpMixed::String(package.get_name().to_string()),
                &PhpMixed::List(
                    self.inner
                        .dev_package_names
                        .iter()
                        .map(|s| Box::new(PhpMixed::String(s.clone())))
                        .collect(),
                ),
                true,
            ) {
                if let Some(PhpMixed::List(list)) = data.get_mut("dev-package-names") {
                    list.push(Box::new(PhpMixed::String(package.get_name().to_string())));
                }
            }
        }

        // PHP: sort($data['dev-package-names']);
        if let Some(PhpMixed::List(list)) = data.get_mut("dev-package-names") {
            // TODO(phase-b): sort PhpMixed::List in-place using string comparison
            sort(list);
        }
        // PHP: usort($data['packages'], static function ($a, $b): int { return strcmp($a['name'], $b['name']); });
        if let Some(PhpMixed::List(list)) = data.get_mut("packages") {
            usort(list, |a: &Box<PhpMixed>, b: &Box<PhpMixed>| -> i64 {
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

        self.file.write(
            PhpMixed::Array(
                data.clone()
                    .into_iter()
                    .map(|(k, v)| (k, Box::new(v)))
                    .collect(),
            ),
            shirabe_php_shim::JSON_UNESCAPED_SLASHES
                | shirabe_php_shim::JSON_PRETTY_PRINT
                | shirabe_php_shim::JSON_UNESCAPED_UNICODE,
        )?;

        if self.dump_versions {
            let versions = self.generate_installed_versions(
                installation_manager,
                &install_paths,
                dev_mode,
                &repo_dir,
            )?;

            self.filesystem.file_put_contents_if_modified(
                &format!("{}/installed.php", repo_dir),
                &format!("<?php return {};\n", self.dump_to_php_code(&versions, 0),),
            );
            let installed_versions_class =
                file_get_contents(&format!("{}/../InstalledVersions.php", php_dir(),));

            // this normally should not happen but during upgrades of Composer when it is installed in the project it is a possibility
            if let Some(class_content) = installed_versions_class {
                self.filesystem.file_put_contents_if_modified(
                    &format!("{}/InstalledVersions.php", repo_dir),
                    &class_content,
                );

                // make sure the in memory state is up to date with on disk
                InstalledVersions::reload(versions);

                // make sure the selfDir matches the expected data at runtime if the class was loaded from the vendor dir, as it may have been
                // loaded from the Composer sources, causing packages to appear twice in that case if the installed.php is loaded in addition to the
                // in memory loaded data from above
                // TODO(phase-b): Reflection API on static properties — confirm porting approach with user
                let _attempt: Result<()> = (|| -> Result<()> {
                    todo!(
                        "ReflectionProperty(Composer\\InstalledVersions::class, 'selfDir')->setValue(null, strtr($repoDir, '\\\\', '/'))"
                    );
                    // (the second reflection block sets installedIsLocalDir = true)
                })();
                // PHP: catches \ReflectionException and rethrows if not "Property does not exist"
            }
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
                        .map(|m| m.into_iter().map(|(k, v)| (k, *v)).collect())
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
            lines.push_str(&if is_int(&PhpMixed::String(key.clone())) {
                // TODO(phase-b): PHP integer-keyed array entries — IndexMap keys are strings
                format!("{} => ", key)
            } else {
                format!("{} => ", var_export(&PhpMixed::String(key.clone()), true))
            });

            if is_array(value) {
                if let Some(inner_arr) = value.as_array() {
                    if !inner_arr.is_empty() {
                        let inner_map: IndexMap<String, PhpMixed> = inner_arr
                            .iter()
                            .map(|(k, v)| (k.clone(), (**v).clone()))
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
                            .map(|(i, v)| (i.to_string(), (**v).clone()))
                            .collect();
                        lines.push_str(&self.dump_to_php_code(&inner_map, level));
                    } else {
                        lines.push_str("array(),\n");
                    }
                }
            } else if key == "install_path" && is_string(value) {
                let s = value.as_string().unwrap_or("").to_string();
                if self.filesystem.is_absolute_path(&s) {
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
        &self,
        installation_manager: &InstallationManager,
        install_paths: &IndexMap<String, Option<String>>,
        dev_mode: bool,
        repo_dir: &str,
    ) -> Result<IndexMap<String, PhpMixed>> {
        let dev_packages = array_flip(&PhpMixed::List(
            self.inner
                .dev_package_names
                .iter()
                .map(|s| Box::new(PhpMixed::String(s.clone())))
                .collect(),
        ));
        let mut packages: Vec<Box<dyn PackageInterface>> = self
            .inner
            .get_packages()
            .into_iter()
            // TODO(phase-b): Box<BasePackage> -> Box<dyn PackageInterface>
            .map(|p| todo!("Box<BasePackage> to Box<dyn PackageInterface>"))
            .collect();
        let mut root_package = match &self.root_package {
            None => {
                return Err(LogicException {
                    message:
                        "It should not be possible to dump packages if no root package is given"
                            .to_string(),
                    code: 0,
                }
                .into());
            }
            // TODO(phase-b): clone root_package to push into packages list
            Some(_r) => todo!("clone root_package"),
        };
        // packages[] = $rootPackage = $this->rootPackage;
        // TODO(phase-b): track current root_package in mutable variable
        let mut current_root: Box<dyn RootPackageInterface> = root_package;
        // packages.push(current_root.clone_box());

        while let Some(_alias) =
            (current_root.as_any() as &dyn Any).downcast_ref::<RootAliasPackage>()
        {
            current_root =
                todo!("RootAliasPackage::get_alias_of() returning Box<dyn RootPackageInterface>");
            // packages.push(current_root.clone_box());
        }
        let mut versions: IndexMap<String, PhpMixed> = IndexMap::new();
        versions.insert(
            "root".to_string(),
            PhpMixed::Array(
                self.dump_root_package(
                    &*current_root,
                    install_paths,
                    dev_mode,
                    repo_dir,
                    &dev_packages,
                )
                .into_iter()
                .map(|(k, v)| (k, Box::new(v)))
                .collect(),
            ),
        );
        versions.insert("versions".to_string(), PhpMixed::Array(IndexMap::new()));

        // add real installed packages
        for package in &packages {
            if (package.as_any() as &dyn Any)
                .downcast_ref::<AliasPackage>()
                .is_some()
            {
                continue;
            }

            let dumped =
                self.dump_installed_package(&**package, install_paths, repo_dir, &dev_packages);
            if let Some(PhpMixed::Array(versions_map)) = versions.get_mut("versions") {
                versions_map.insert(
                    package.get_name().to_string(),
                    Box::new(PhpMixed::Array(
                        dumped.into_iter().map(|(k, v)| (k, Box::new(v))).collect(),
                    )),
                );
            }
        }

        // add provided/replaced packages
        for package in &packages {
            let is_dev_package = dev_packages
                .as_array()
                .map(|m| m.contains_key(package.get_name()))
                .unwrap_or(false);
            for replace in package.get_replaces() {
                // exclude platform replaces as when they are really there we can not check for their presence
                if PlatformRepository::is_platform_package(replace.get_target()) {
                    continue;
                }
                // PHP: dev_requirement handling
                // TODO(phase-b): mutate nested versions['versions'][$replace->getTarget()]['dev_requirement']
                todo!("mutate nested versions['versions'][target]['dev_requirement']");
                #[allow(unreachable_code)]
                {
                    let mut replaced = replace.get_pretty_constraint().unwrap_or("").to_string();
                    if replaced == "self.version" {
                        replaced = package.get_pretty_version().to_string();
                    }
                    // TODO(phase-b): mutate nested versions['versions'][$replace->getTarget()]['replaced']
                    todo!("append replaced to versions['versions'][target]['replaced']");
                }
            }
            for provide in package.get_provides() {
                // exclude platform provides as when they are really there we can not check for their presence
                if PlatformRepository::is_platform_package(provide.get_target()) {
                    continue;
                }
                // TODO(phase-b): mutate nested versions['versions'][$provide->getTarget()]['dev_requirement']
                todo!("mutate nested versions['versions'][target]['dev_requirement']");
                #[allow(unreachable_code)]
                {
                    let mut provided = provide.get_pretty_constraint().unwrap_or("").to_string();
                    if provided == "self.version" {
                        provided = package.get_pretty_version().to_string();
                    }
                    // TODO(phase-b): mutate nested versions['versions'][$provide->getTarget()]['provided']
                    todo!("append provided to versions['versions'][target]['provided']");
                }
            }
        }

        // add aliases
        for package in &packages {
            let Some(alias) = (package.as_any() as &dyn Any).downcast_ref::<AliasPackage>() else {
                continue;
            };
            // TODO(phase-b): mutate nested versions['versions'][name]['aliases']
            todo!("append alias->getPrettyVersion() to versions['versions'][name]['aliases']");
            if (package.as_any() as &dyn Any)
                .downcast_ref::<dyn RootPackageInterface>()
                .is_some()
            {
                // TODO(phase-b): same mutation on versions['root']['aliases']
                todo!("append alias->getPrettyVersion() to versions['root']['aliases']");
            }
        }

        if let Some(PhpMixed::Array(versions_map)) = versions.get_mut("versions") {
            // TODO(phase-b): ksort signature mismatch on nested IndexMap; cast appropriately
            ksort(versions_map);
        }
        ksort(&mut versions);

        if let Some(PhpMixed::Array(versions_map)) = versions.get_mut("versions") {
            for (_name, version) in versions_map.iter_mut() {
                if let PhpMixed::Array(version_map) = version.as_mut() {
                    for key in ["aliases", "replaced", "provided"] {
                        if let Some(PhpMixed::List(list)) = version_map.get_mut(key) {
                            // PHP: sort($versions['versions'][$name][$key], SORT_NATURAL);
                            sort_with_flags(list, SORT_NATURAL);
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
        package: &dyn PackageInterface,
        install_paths: &IndexMap<String, Option<String>>,
        repo_dir: &str,
        dev_packages: &PhpMixed,
    ) -> IndexMap<String, PhpMixed> {
        let mut reference: Option<String> = None;
        if let Some(install_src) = package.get_installation_source() {
            reference = if install_src == "source" {
                package.get_source_reference().map(String::from)
            } else {
                package.get_dist_reference().map(String::from)
            };
        }
        if reference.is_none() {
            // PHP: ($package->getSourceReference() ?: $package->getDistReference()) ?: null;
            let source = package.get_source_reference().unwrap_or("");
            let dist = package.get_dist_reference().unwrap_or("");
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

        let install_path = if (package.as_any() as &dyn Any)
            .downcast_ref::<dyn RootPackageInterface>()
            .is_some()
        {
            let to = self.filesystem.normalize_path(
                &realpath(&Platform::get_cwd(false).unwrap_or_default()).unwrap_or_default(),
            );
            Some(self.filesystem.find_shortest_path(repo_dir, &to, true))
        } else {
            install_paths.get(package.get_name()).cloned().flatten()
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
                    .map(|m| m.contains_key(package.get_name()))
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
        package: &dyn RootPackageInterface,
        install_paths: &IndexMap<String, Option<String>>,
        dev_mode: bool,
        repo_dir: &str,
        dev_packages: &PhpMixed,
    ) -> IndexMap<String, PhpMixed> {
        let data =
            // TODO(phase-b): RootPackageInterface trait bound — pass as &dyn PackageInterface
            self.dump_installed_package(todo!("package as &dyn PackageInterface"), install_paths, repo_dir, dev_packages);

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
