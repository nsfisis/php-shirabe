//! ref: composer/src/Composer/InstalledVersions.php

use std::sync::Mutex;

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_php_shim::{
    array_flip, array_keys, array_merge, call_user_func_array, implode, is_file, method_exists,
    php_dir, require_php_file, strtr_array, substr, trigger_error, OutOfBoundsException, PhpMixed,
    E_USER_DEPRECATED,
};
use shirabe_semver::version_parser::VersionParser;

use crate::autoload::class_loader::ClassLoader;

/// This class is copied in every Composer installed project and available to all
///
/// See also https://getcomposer.org/doc/07-runtime.md#installed-versions
///
/// To require its presence, you can require `composer-runtime-api ^2.0`
///
/// @final
pub struct InstalledVersions;

/// @var string|null if set (by reflection by Composer), this should be set to the path where this class is being copied to
/// @internal
static SELF_DIR: Mutex<Option<String>> = Mutex::new(None);

/// @var mixed[]|null
/// @psalm-var array{root: array{...}, versions: array<string, array{...}>}|array{}|null
static INSTALLED: Mutex<Option<IndexMap<String, PhpMixed>>> = Mutex::new(None);

/// @var bool
static INSTALLED_IS_LOCAL_DIR: Mutex<bool> = Mutex::new(false);

/// @var bool|null
static CAN_GET_VENDORS: Mutex<Option<bool>> = Mutex::new(None);

/// @var array[]
/// @psalm-var array<string, array{...}>
static INSTALLED_BY_VENDOR: Mutex<IndexMap<String, IndexMap<String, PhpMixed>>> =
    Mutex::new(IndexMap::new());

impl InstalledVersions {
    /// Returns a list of all package names which are present, either by being installed, replaced or provided
    ///
    /// @return string[]
    /// @psalm-return list<string>
    pub fn get_installed_packages() -> Vec<String> {
        let mut packages: Vec<Vec<String>> = vec![];
        for installed in Self::get_installed() {
            let versions = installed
                .get("versions")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            // PHP: array_keys($installed['versions'])
            let keys: Vec<String> = array_keys(
                &versions
                    .into_iter()
                    .map(|(k, v)| (k, *v))
                    .collect::<IndexMap<String, PhpMixed>>(),
            );
            packages.push(keys);
        }

        if 1 == packages.len() {
            return packages.into_iter().next().unwrap();
        }

        // PHP: array_keys(array_flip(\call_user_func_array('array_merge', $packages)))
        let merged = call_user_func_array(
            "array_merge",
            &PhpMixed::List(
                packages
                    .into_iter()
                    .map(|p| {
                        Box::new(PhpMixed::List(
                            p.into_iter()
                                .map(|s| Box::new(PhpMixed::String(s)))
                                .collect(),
                        ))
                    })
                    .collect(),
            ),
        );
        let flipped = array_flip(&merged);
        // TODO(phase-b): convert flipped (PhpMixed::Array) to IndexMap<String, V>
        array_keys(
            &flipped
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|(k, v)| (k, *v))
                .collect::<IndexMap<String, PhpMixed>>(),
        )
    }

    /// Returns a list of all package names with a specific type e.g. 'library'
    ///
    /// @param  string   $type
    /// @return string[]
    /// @psalm-return list<string>
    pub fn get_installed_packages_by_type(r#type: &str) -> Vec<String> {
        let mut packages_by_type: Vec<String> = vec![];

        for installed in Self::get_installed() {
            let versions = installed
                .get("versions")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            for (name, package) in versions {
                if let Some(pkg) = package.as_array() {
                    if let Some(pkg_type) = pkg.get("type").and_then(|v| v.as_string()) {
                        if pkg_type == r#type {
                            packages_by_type.push(name);
                        }
                    }
                }
            }
        }

        packages_by_type
    }

    /// Checks whether the given package is installed
    ///
    /// This also returns true if the package name is provided or replaced by another package
    ///
    /// @param  string $packageName
    /// @param  bool   $includeDevRequirements
    /// @return bool
    pub fn is_installed(package_name: &str, include_dev_requirements: bool) -> bool {
        for installed in Self::get_installed() {
            let Some(versions) = installed.get("versions").and_then(|v| v.as_array()) else {
                continue;
            };
            if let Some(package) = versions.get(package_name) {
                let dev_requirement = package
                    .as_array()
                    .and_then(|a| a.get("dev_requirement"))
                    .map(|v| v.as_ref().clone())
                    .unwrap_or(PhpMixed::Null);
                return include_dev_requirements
                    || matches!(dev_requirement, PhpMixed::Null)
                    || matches!(dev_requirement, PhpMixed::Bool(false));
            }
        }

        false
    }

    /// Checks whether the given package satisfies a version constraint
    ///
    /// e.g. If you want to know whether version 2.3+ of package foo/bar is installed, you would call:
    ///
    ///   Composer\InstalledVersions::satisfies(new VersionParser, 'foo/bar', '^2.3')
    ///
    /// @param  VersionParser $parser      Install composer/semver to have access to this class and functionality
    /// @param  string        $packageName
    /// @param  string|null   $constraint  A version constraint to check for, if you pass one you have to make sure composer/semver is required by your package
    /// @return bool
    pub fn satisfies(
        parser: &VersionParser,
        package_name: &str,
        constraint: Option<&str>,
    ) -> Result<bool> {
        let constraint = parser.parse_constraints(constraint.unwrap_or(""))?;
        let provided = parser.parse_constraints(&Self::get_version_ranges(package_name)?)?;

        Ok(provided.matches(&*constraint))
    }

    /// Returns a version constraint representing all the range(s) which are installed for a given package
    ///
    /// It is easier to use this via isInstalled() with the $constraint argument if you need to check
    /// whether a given version of a package is installed, and not just whether it exists
    ///
    /// @param  string $packageName
    /// @return string Version constraint usable with composer/semver
    pub fn get_version_ranges(package_name: &str) -> Result<String> {
        for installed in Self::get_installed() {
            let Some(versions) = installed.get("versions").and_then(|v| v.as_array()) else {
                continue;
            };
            let Some(pkg) = versions.get(package_name).and_then(|v| v.as_array()).cloned()
            else {
                continue;
            };

            let mut ranges: Vec<String> = vec![];
            if let Some(pretty_version) = pkg.get("pretty_version").and_then(|v| v.as_string()) {
                ranges.push(pretty_version.to_string());
            }
            if pkg.contains_key("aliases") {
                ranges = array_merge(
                    PhpMixed::List(
                        ranges
                            .iter()
                            .map(|s| Box::new(PhpMixed::String(s.clone())))
                            .collect(),
                    ),
                    pkg.get("aliases")
                        .map(|v| (**v).clone())
                        .unwrap_or(PhpMixed::Null),
                )
                .as_list()
                .map(|l| {
                    l.iter()
                        .filter_map(|v| v.as_string().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            }
            if pkg.contains_key("replaced") {
                ranges = array_merge(
                    PhpMixed::List(
                        ranges
                            .iter()
                            .map(|s| Box::new(PhpMixed::String(s.clone())))
                            .collect(),
                    ),
                    pkg.get("replaced")
                        .map(|v| (**v).clone())
                        .unwrap_or(PhpMixed::Null),
                )
                .as_list()
                .map(|l| {
                    l.iter()
                        .filter_map(|v| v.as_string().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            }
            if pkg.contains_key("provided") {
                ranges = array_merge(
                    PhpMixed::List(
                        ranges
                            .iter()
                            .map(|s| Box::new(PhpMixed::String(s.clone())))
                            .collect(),
                    ),
                    pkg.get("provided")
                        .map(|v| (**v).clone())
                        .unwrap_or(PhpMixed::Null),
                )
                .as_list()
                .map(|l| {
                    l.iter()
                        .filter_map(|v| v.as_string().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            }

            return Ok(implode(" || ", &ranges));
        }

        Err(OutOfBoundsException {
            message: format!("Package \"{}\" is not installed", package_name),
            code: 0,
        }
        .into())
    }

    /// @param  string      $packageName
    /// @return string|null If the package is being replaced or provided but is not really installed, null will be returned as version, use satisfies or getVersionRanges if you need to know if a given version is present
    pub fn get_version(package_name: &str) -> Result<Option<String>> {
        for installed in Self::get_installed() {
            let Some(versions) = installed.get("versions").and_then(|v| v.as_array()) else {
                continue;
            };
            let Some(pkg) = versions.get(package_name).and_then(|v| v.as_array()) else {
                continue;
            };

            if !pkg.contains_key("version") {
                return Ok(None);
            }

            return Ok(pkg
                .get("version")
                .and_then(|v| v.as_string())
                .map(|s| s.to_string()));
        }

        Err(OutOfBoundsException {
            message: format!("Package \"{}\" is not installed", package_name),
            code: 0,
        }
        .into())
    }

    /// @param  string      $packageName
    /// @return string|null If the package is being replaced or provided but is not really installed, null will be returned as version, use satisfies or getVersionRanges if you need to know if a given version is present
    pub fn get_pretty_version(package_name: &str) -> Result<Option<String>> {
        for installed in Self::get_installed() {
            let Some(versions) = installed.get("versions").and_then(|v| v.as_array()) else {
                continue;
            };
            let Some(pkg) = versions.get(package_name).and_then(|v| v.as_array()) else {
                continue;
            };

            if !pkg.contains_key("pretty_version") {
                return Ok(None);
            }

            return Ok(pkg
                .get("pretty_version")
                .and_then(|v| v.as_string())
                .map(|s| s.to_string()));
        }

        Err(OutOfBoundsException {
            message: format!("Package \"{}\" is not installed", package_name),
            code: 0,
        }
        .into())
    }

    /// @param  string      $packageName
    /// @return string|null If the package is being replaced or provided but is not really installed, null will be returned as reference
    pub fn get_reference(package_name: &str) -> Result<Option<String>> {
        for installed in Self::get_installed() {
            let Some(versions) = installed.get("versions").and_then(|v| v.as_array()) else {
                continue;
            };
            let Some(pkg) = versions.get(package_name).and_then(|v| v.as_array()) else {
                continue;
            };

            if !pkg.contains_key("reference") {
                return Ok(None);
            }

            return Ok(pkg
                .get("reference")
                .and_then(|v| v.as_string())
                .map(|s| s.to_string()));
        }

        Err(OutOfBoundsException {
            message: format!("Package \"{}\" is not installed", package_name),
            code: 0,
        }
        .into())
    }

    /// @param  string      $packageName
    /// @return string|null If the package is being replaced or provided but is not really installed, null will be returned as install path. Packages of type metapackages also have a null install path.
    pub fn get_install_path(package_name: &str) -> Result<Option<String>> {
        for installed in Self::get_installed() {
            let Some(versions) = installed.get("versions").and_then(|v| v.as_array()) else {
                continue;
            };
            let Some(pkg) = versions.get(package_name).and_then(|v| v.as_array()) else {
                continue;
            };

            return Ok(if pkg.contains_key("install_path") {
                pkg.get("install_path")
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_string())
            } else {
                None
            });
        }

        Err(OutOfBoundsException {
            message: format!("Package \"{}\" is not installed", package_name),
            code: 0,
        }
        .into())
    }

    /// @return array
    /// @psalm-return array{name: string, pretty_version: string, version: string, reference: string|null, type: string, install_path: string, aliases: string[], dev: bool}
    pub fn get_root_package() -> IndexMap<String, PhpMixed> {
        let installed = Self::get_installed();

        installed
            .into_iter()
            .next()
            .and_then(|d| d.get("root").and_then(|v| v.as_array()).cloned())
            .map(|m| m.into_iter().map(|(k, v)| (k, *v)).collect())
            .unwrap_or_default()
    }

    /// Returns the raw installed.php data for custom implementations
    ///
    /// @deprecated Use getAllRawData() instead which returns all datasets for all autoloaders present in the process. getRawData only returns the first dataset loaded, which may not be what you expect.
    /// @return array[]
    pub fn get_raw_data() -> IndexMap<String, PhpMixed> {
        // PHP: @trigger_error(...)
        // TODO(phase-b): Silencer::call wraps trigger_error
        trigger_error(
            "getRawData only returns the first dataset loaded, which may not be what you expect. Use getAllRawData() instead which returns all datasets for all autoloaders present in the process.",
            E_USER_DEPRECATED,
        );

        let mut installed = INSTALLED.lock().unwrap();
        if installed.is_none() {
            // only require the installed.php file if this file is loaded from its dumped location,
            // and not from its source location in the composer/composer package, see https://github.com/composer/composer/issues/9937
            if substr(&php_dir(), -8, Some(1)) != "C" {
                let required = require_php_file(&format!("{}/installed.php", php_dir()));
                *installed = required
                    .as_array()
                    .cloned()
                    .map(|m| m.into_iter().map(|(k, v)| (k, *v)).collect());
            } else {
                *installed = Some(IndexMap::new());
            }
        }

        installed.clone().unwrap_or_default()
    }

    /// Returns the raw data of all installed.php which are currently loaded for custom implementations
    ///
    /// @return array[]
    pub fn get_all_raw_data() -> Vec<IndexMap<String, PhpMixed>> {
        Self::get_installed()
    }

    /// Lets you reload the static array from another file
    ///
    /// This is only useful for complex integrations in which a project needs to use
    /// this class but then also needs to execute another project's autoloader in process,
    /// and wants to ensure both projects have access to their version of installed.php.
    ///
    /// A typical case would be PHPUnit, where it would need to make sure it reads all
    /// the data it needs from this class, then call reload() with
    /// `require $CWD/vendor/composer/installed.php` (or similar) as input to make sure
    /// the project in which it runs can then also use this class safely, without
    /// interference between PHPUnit's dependencies and the project's dependencies.
    ///
    /// @param  array[] $data A vendor/composer/installed.php data set
    /// @return void
    pub fn reload(data: IndexMap<String, PhpMixed>) {
        *INSTALLED.lock().unwrap() = Some(data);
        *INSTALLED_BY_VENDOR.lock().unwrap() = IndexMap::new();

        // when using reload, we disable the duplicate protection to ensure that self::$installed data is
        // always returned, but we cannot know whether it comes from the installed.php in __DIR__ or not,
        // so we have to assume it does not, and that may result in duplicate data being returned when listing
        // all installed packages for example
        *INSTALLED_IS_LOCAL_DIR.lock().unwrap() = false;
    }

    /// @return string
    fn get_self_dir() -> String {
        let mut self_dir = SELF_DIR.lock().unwrap();
        if self_dir.is_none() {
            *self_dir = Some(strtr_array(
                &php_dir(),
                &{
                    let mut m = IndexMap::new();
                    m.insert("\\".to_string(), "/".to_string());
                    m
                },
            ));
        }

        self_dir.clone().unwrap()
    }

    /// @return array[]
    /// @psalm-return list<array{root: ..., versions: ...}>
    fn get_installed() -> Vec<IndexMap<String, PhpMixed>> {
        {
            let mut can_get_vendors = CAN_GET_VENDORS.lock().unwrap();
            if can_get_vendors.is_none() {
                *can_get_vendors = Some(method_exists(
                    &PhpMixed::String("Composer\\Autoload\\ClassLoader".to_string()),
                    "getRegisteredLoaders",
                ));
            }
        }

        let mut installed: Vec<IndexMap<String, PhpMixed>> = vec![];
        let mut copied_local_dir = false;

        if CAN_GET_VENDORS.lock().unwrap().unwrap_or(false) {
            let self_dir = Self::get_self_dir();
            for (vendor_dir, _loader) in ClassLoader::get_registered_loaders() {
                let vendor_dir = strtr_array(&vendor_dir, &{
                    let mut m = IndexMap::new();
                    m.insert("\\".to_string(), "/".to_string());
                    m
                });
                let cached = INSTALLED_BY_VENDOR.lock().unwrap().get(&vendor_dir).cloned();
                if let Some(cached) = cached {
                    installed.push(cached);
                } else if is_file(&format!("{}/composer/installed.php", vendor_dir)) {
                    let required = require_php_file(&format!(
                        "{}/composer/installed.php",
                        vendor_dir,
                    ));
                    let required_map: IndexMap<String, PhpMixed> = required
                        .as_array()
                        .cloned()
                        .map(|m| m.into_iter().map(|(k, v)| (k, *v)).collect())
                        .unwrap_or_default();
                    INSTALLED_BY_VENDOR
                        .lock()
                        .unwrap()
                        .insert(vendor_dir.clone(), required_map.clone());
                    installed.push(required_map.clone());
                    let mut installed_static = INSTALLED.lock().unwrap();
                    if installed_static.is_none() && format!("{}/composer", vendor_dir) == self_dir
                    {
                        *installed_static = Some(required_map);
                        *INSTALLED_IS_LOCAL_DIR.lock().unwrap() = true;
                    }
                }
                if *INSTALLED_IS_LOCAL_DIR.lock().unwrap()
                    && format!("{}/composer", vendor_dir) == self_dir
                {
                    copied_local_dir = true;
                }
            }
        }

        {
            let mut installed_static = INSTALLED.lock().unwrap();
            if installed_static.is_none() {
                // only require the installed.php file if this file is loaded from its dumped location,
                // and not from its source location in the composer/composer package, see https://github.com/composer/composer/issues/9937
                if substr(&php_dir(), -8, Some(1)) != "C" {
                    let required = require_php_file(&format!("{}/installed.php", php_dir()));
                    *installed_static = required
                        .as_array()
                        .cloned()
                        .map(|m| m.into_iter().map(|(k, v)| (k, *v)).collect());
                } else {
                    *installed_static = Some(IndexMap::new());
                }
            }
        }

        let installed_static_data = INSTALLED.lock().unwrap().clone().unwrap_or_default();
        if !installed_static_data.is_empty() && !copied_local_dir {
            installed.push(installed_static_data);
        }

        installed
    }
}
