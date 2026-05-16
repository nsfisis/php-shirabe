//! ref: composer/src/Composer/Autoload/AutoloadGenerator.php

use indexmap::IndexMap;

use shirabe_class_map_generator::class_map::ClassMap;
use shirabe_class_map_generator::class_map_generator::ClassMapGenerator;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::symfony::component::console::formatter::output_formatter::OutputFormatter;
use shirabe_php_shim::{
    array_filter, array_keys, array_map, array_merge, array_merge_recursive, array_reverse,
    array_shift, array_slice, array_unique, bin2hex, explode, file_exists, file_get_contents,
    hash, implode, in_array, is_array, krsort, ksort, ltrim, preg_quote, random_bytes, realpath,
    sprintf, str_replace, str_starts_with, str_contains, strlen, strpos, strtr, substr,
    substr_count, trim, trigger_error, unlink, var_export, E_USER_DEPRECATED,
    InvalidArgumentException, PhpMixed, RuntimeException,
};
use shirabe_semver::constraint::bound::Bound;

use crate::config::Config;
use crate::event_dispatcher::event_dispatcher::EventDispatcher;
use crate::filter::platform_requirement_filter::ignore_all_platform_requirement_filter::IgnoreAllPlatformRequirementFilter;
use crate::filter::platform_requirement_filter::platform_requirement_filter_factory::PlatformRequirementFilterFactory;
use crate::filter::platform_requirement_filter::platform_requirement_filter_interface::PlatformRequirementFilterInterface;
use crate::installer::installation_manager::InstallationManager;
use crate::io::io_interface::IOInterface;
use crate::io::null_io::NullIO;
use crate::json::json_file::JsonFile;
use crate::package::alias_package::AliasPackage;
use crate::package::locker::Locker;
use crate::package::package_interface::PackageInterface;
use crate::package::root_package_interface::RootPackageInterface;
use crate::repository::installed_repository_interface::InstalledRepositoryInterface;
use crate::script::script_events::ScriptEvents;
use crate::util::filesystem::Filesystem;
use crate::util::package_sorter::PackageSorter;
use crate::util::platform::Platform;
use crate::autoload::class_loader::ClassLoader;

#[derive(Debug)]
pub struct AutoloadGenerator {
    event_dispatcher: EventDispatcher,
    io: Box<dyn IOInterface>,
    dev_mode: Option<bool>,
    class_map_authoritative: bool,
    apcu: bool,
    apcu_prefix: Option<String>,
    dry_run: bool,
    run_scripts: bool,
    platform_requirement_filter: Box<dyn PlatformRequirementFilterInterface>,
}

impl AutoloadGenerator {
    pub fn new(event_dispatcher: EventDispatcher, io: Option<Box<dyn IOInterface>>) -> Self {
        let io: Box<dyn IOInterface> = io.unwrap_or_else(|| Box::new(NullIO::new()));

        Self {
            event_dispatcher,
            io,
            dev_mode: None,
            class_map_authoritative: false,
            apcu: false,
            apcu_prefix: None,
            dry_run: false,
            run_scripts: false,
            platform_requirement_filter: PlatformRequirementFilterFactory::ignore_nothing(),
        }
    }

    pub fn set_dev_mode(&mut self, dev_mode: bool) {
        self.dev_mode = Some(dev_mode);
    }

    /// Whether generated autoloader considers the class map authoritative.
    pub fn set_class_map_authoritative(&mut self, class_map_authoritative: bool) {
        self.class_map_authoritative = class_map_authoritative;
    }

    /// Whether generated autoloader considers APCu caching.
    pub fn set_apcu(&mut self, apcu: bool, apcu_prefix: Option<String>) {
        self.apcu = apcu;
        self.apcu_prefix = apcu_prefix;
    }

    /// Whether to run scripts or not
    pub fn set_run_scripts(&mut self, run_scripts: bool) {
        self.run_scripts = run_scripts;
    }

    /// Whether to run in drymode or not
    pub fn set_dry_run(&mut self, dry_run: bool) {
        self.dry_run = dry_run;
    }

    /// Whether platform requirements should be ignored.
    ///
    /// If this is set to true, the platform check file will not be generated
    /// If this is set to false, the platform check file will be generated with all requirements
    /// If this is set to string[], those packages will be ignored from the platform check file
    ///
    /// Deprecated: use setPlatformRequirementFilter instead
    pub fn set_ignore_platform_requirements(&mut self, ignore_platform_reqs: PhpMixed) {
        trigger_error(
            "AutoloadGenerator::setIgnorePlatformRequirements is deprecated since Composer 2.2, use setPlatformRequirementFilter instead.",
            E_USER_DEPRECATED,
        );

        self.set_platform_requirement_filter(PlatformRequirementFilterFactory::from_bool_or_list(ignore_platform_reqs));
    }

    pub fn set_platform_requirement_filter(
        &mut self,
        platform_requirement_filter: Box<dyn PlatformRequirementFilterInterface>,
    ) {
        self.platform_requirement_filter = platform_requirement_filter;
    }

    pub fn dump(
        &mut self,
        config: &Config,
        local_repo: &dyn InstalledRepositoryInterface,
        root_package: &dyn RootPackageInterface,
        installation_manager: &InstallationManager,
        target_dir: &str,
        scan_psr_packages: bool,
        suffix: Option<String>,
        locker: Option<&Locker>,
        strict_ambiguous: bool,
    ) -> anyhow::Result<ClassMap> {
        let mut scan_psr_packages = scan_psr_packages;
        if self.class_map_authoritative {
            // Force scanPsrPackages when classmap is authoritative
            scan_psr_packages = true;
        }

        // auto-set devMode based on whether dev dependencies are installed or not
        if self.dev_mode.is_none() {
            // we assume no-dev mode if no vendor dir is present or it is too old to contain dev information
            self.dev_mode = Some(false);

            let installed_json = JsonFile::new(
                format!(
                    "{}/composer/installed.json",
                    config.get("vendor-dir").as_string().unwrap_or("")
                ),
                None,
                None,
            );
            if installed_json.exists() {
                let installed_json_data = installed_json.read()?;
                if let Some(arr) = installed_json_data.as_array() {
                    if let Some(dev) = arr.get("dev") {
                        self.dev_mode = dev.as_bool();
                    }
                }
            }
        }

        if self.run_scripts {
            // set COMPOSER_DEV_MODE in case not set yet so it is available in the dump-autoload event listeners
            if shirabe_php_shim::server_get("COMPOSER_DEV_MODE").is_none() {
                Platform::put_env(
                    "COMPOSER_DEV_MODE",
                    if self.dev_mode.unwrap_or(false) { "1" } else { "0" },
                );
            }

            let mut additional_args: IndexMap<String, PhpMixed> = IndexMap::new();
            additional_args.insert("optimize".to_string(), PhpMixed::Bool(scan_psr_packages));
            self.event_dispatcher.dispatch_script_with_args(
                ScriptEvents::PRE_AUTOLOAD_DUMP,
                self.dev_mode.unwrap_or(false),
                vec![],
                additional_args,
            );
        }

        let mut class_map_generator = ClassMapGenerator::new(vec!["php".to_string(), "inc".to_string(), "hh".to_string()]);
        class_map_generator.avoid_duplicate_scans();

        let filesystem = Filesystem::new(None);
        filesystem.ensure_directory_exists(config.get("vendor-dir").as_string().unwrap_or(""))?;
        // Do not remove double realpath() calls.
        // Fixes failing Windows realpath() implementation.
        // See https://bugs.php.net/bug.php?id=72738
        let base_path = filesystem.normalize_path(&realpath(&realpath(&Platform::get_cwd()).unwrap_or_default()).unwrap_or_default());
        let vendor_path = filesystem.normalize_path(&realpath(&realpath(config.get("vendor-dir").as_string().unwrap_or("")).unwrap_or_default()).unwrap_or_default());
        let use_global_include_path = config.get("use-include-path").as_bool().unwrap_or(false);
        let prepend_autoloader = if config.get("prepend-autoloader").as_bool() == Some(false) {
            "false"
        } else {
            "true"
        };
        let target_dir = format!("{}/{}", vendor_path, target_dir);
        filesystem.ensure_directory_exists(&target_dir)?;

        let vendor_path_code = filesystem.find_shortest_path_code(&realpath(&target_dir).unwrap_or_default(), &vendor_path, true, false);
        let vendor_path_to_target_dir_code = filesystem.find_shortest_path_code(&vendor_path, &realpath(&target_dir).unwrap_or_default(), true, false);

        let app_base_dir_code = filesystem.find_shortest_path_code(&vendor_path, &base_path, true, false);
        let app_base_dir_code = str_replace("__DIR__", "$vendorDir", &app_base_dir_code);

        let mut namespaces_file = format!(
            "<?php\n\n// autoload_namespaces.php @generated by Composer\n\n$vendorDir = {};\n$baseDir = {};\n\nreturn array(\n",
            vendor_path_code, app_base_dir_code
        );

        let mut psr4_file = format!(
            "<?php\n\n// autoload_psr4.php @generated by Composer\n\n$vendorDir = {};\n$baseDir = {};\n\nreturn array(\n",
            vendor_path_code, app_base_dir_code
        );

        // Collect information from all packages.
        let dev_package_names = local_repo.get_dev_package_names();
        let package_map = self.build_package_map(installation_manager, root_package, local_repo.get_canonical_packages())?;
        let filtered_dev_packages: PhpMixed = if self.dev_mode.unwrap_or(false) {
            // if dev mode is enabled, then we do not filter any dev packages out so disable this entirely
            PhpMixed::Bool(false)
        } else {
            // if the list of dev package names is available we use that straight, otherwise pass true which means use legacy algo to figure them out
            if !dev_package_names.is_empty() {
                PhpMixed::List(dev_package_names.iter().map(|s| Box::new(PhpMixed::String(s.clone()))).collect())
            } else {
                PhpMixed::Bool(true)
            }
        };
        let autoloads = self.parse_autoloads(&package_map, root_package, filtered_dev_packages);

        // Process the 'psr-0' base directories.
        let psr0_map = autoloads.get("psr-0").and_then(|v| v.as_array().cloned()).unwrap_or_default();
        for (namespace, paths) in &psr0_map {
            let mut exported_paths: Vec<String> = vec![];
            if let Some(p_list) = paths.as_list() {
                for path in p_list {
                    exported_paths.push(self.get_path_code(&filesystem, &base_path, &vendor_path, path.as_string().unwrap_or("")));
                }
            }
            let exported_prefix = var_export(&PhpMixed::String(namespace.clone()), true);
            namespaces_file.push_str(&format!("    {} => ", exported_prefix));
            namespaces_file.push_str(&format!("array({}),\n", implode(", ", &exported_paths)));
        }
        namespaces_file.push_str(");\n");

        // Process the 'psr-4' base directories.
        let psr4_map = autoloads.get("psr-4").and_then(|v| v.as_array().cloned()).unwrap_or_default();
        for (namespace, paths) in &psr4_map {
            let mut exported_paths: Vec<String> = vec![];
            if let Some(p_list) = paths.as_list() {
                for path in p_list {
                    exported_paths.push(self.get_path_code(&filesystem, &base_path, &vendor_path, path.as_string().unwrap_or("")));
                }
            }
            let exported_prefix = var_export(&PhpMixed::String(namespace.clone()), true);
            psr4_file.push_str(&format!("    {} => ", exported_prefix));
            psr4_file.push_str(&format!("array({}),\n", implode(", ", &exported_paths)));
        }
        psr4_file.push_str(");\n");

        // add custom psr-0 autoloading if the root package has a target dir
        let mut target_dir_loader: Option<String> = None;
        let main_autoload = root_package.get_autoload();
        if root_package.get_target_dir().is_some()
            && main_autoload.get("psr-0").map_or(false, |v| !v.is_empty())
        {
            let levels = substr_count(&filesystem.normalize_path(&root_package.get_target_dir().unwrap_or_default()), "/") + 1;
            let psr0_keys = main_autoload.get("psr-0").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let prefixes = implode(
                ", ",
                &array_map(
                    |prefix: &String| var_export(&PhpMixed::String(prefix.clone()), true),
                    &array_keys(&psr0_keys),
                ),
            );
            let base_dir_from_target_dir_code =
                filesystem.find_shortest_path_code(&target_dir, &base_path, true, false);

            target_dir_loader = Some(format!(
                "\n    public static function autoload($class)\n    {{\n        $dir = {} . '/';\n        $prefixes = array({});\n        foreach ($prefixes as $prefix) {{\n            if (0 !== strpos($class, $prefix)) {{\n                continue;\n            }}\n            $path = $dir . implode('/', array_slice(explode('\\\\', $class), {})).'.php';\n            if (!$path = stream_resolve_include_path($path)) {{\n                return false;\n            }}\n            require $path;\n\n            return true;\n        }}\n    }}\n",
                base_dir_from_target_dir_code, prefixes, levels
            ));
        }

        let mut excluded: Vec<String> = vec![];
        if let Some(ex) = autoloads.get("exclude-from-classmap").and_then(|v| v.as_list()) {
            if !ex.is_empty() {
                excluded = ex.iter().filter_map(|v| v.as_string().map(|s| s.to_string())).collect();
            }
        }

        let classmap_list = autoloads.get("classmap").and_then(|v| v.as_list()).cloned().unwrap_or_default();
        for dir in &classmap_list {
            let dir_str = dir.as_string().unwrap_or("");
            class_map_generator.scan_paths(dir_str, self.build_exclusion_regex(dir_str, excluded.clone()), "classmap", "");
        }

        if scan_psr_packages {
            let mut namespaces_to_scan: IndexMap<String, Vec<IndexMap<String, PhpMixed>>> = IndexMap::new();

            // Scan the PSR-0/4 directories for class files, and add them to the class map
            for psr_type in &["psr-4", "psr-0"] {
                let map = autoloads.get(*psr_type).and_then(|v| v.as_array()).cloned().unwrap_or_default();
                for (namespace, paths) in &map {
                    let mut entry: IndexMap<String, PhpMixed> = IndexMap::new();
                    entry.insert("paths".to_string(), (**paths).clone());
                    entry.insert("type".to_string(), PhpMixed::String(psr_type.to_string()));
                    namespaces_to_scan.entry(namespace.clone()).or_insert_with(Vec::new).push(entry);
                }
            }

            krsort(&mut namespaces_to_scan);

            for (namespace, groups) in &namespaces_to_scan {
                for group in groups {
                    let paths = group.get("paths").and_then(|v| v.as_list()).cloned().unwrap_or_default();
                    let group_type = group.get("type").and_then(|v| v.as_string()).unwrap_or("").to_string();
                    for dir in &paths {
                        let dir_str = dir.as_string().unwrap_or("").to_string();
                        let dir_str = filesystem.normalize_path(if filesystem.is_absolute_path(&dir_str) {
                            &dir_str
                        } else {
                            &format!("{}/{}", base_path, dir_str)
                        });
                        if !shirabe_php_shim::is_dir(&dir_str) {
                            continue;
                        }

                        // if the vendor dir is contained within a psr-0/psr-4 dir being scanned we exclude it
                        let exclusion_regex = if str_contains(&vendor_path, &format!("{}/", dir_str)) {
                            self.build_exclusion_regex(
                                &dir_str,
                                array_merge(excluded.clone(), vec![format!("{}/", vendor_path)]),
                            )
                        } else {
                            self.build_exclusion_regex(&dir_str, excluded.clone())
                        };

                        class_map_generator.scan_paths(&dir_str, exclusion_regex, &group_type, namespace);
                    }
                }
            }
        }

        let class_map = class_map_generator.get_class_map();
        let ambiguous_classes = if strict_ambiguous {
            class_map.get_ambiguous_classes(false)
        } else {
            class_map.get_ambiguous_classes(true)
        };
        for (class_name, ambiguous_paths) in &ambiguous_classes {
            if ambiguous_paths.len() > 1 {
                self.io.write_error(&format!(
                    "<warning>Warning: Ambiguous class resolution, \"{}\" was found {}x: in \"{}\" and \"{}\", the first will be used.</warning>",
                    class_name,
                    ambiguous_paths.len() + 1,
                    class_map.get_class_path(class_name),
                    implode("\", \"", ambiguous_paths)
                ));
            } else {
                self.io.write_error(&format!(
                    "<warning>Warning: Ambiguous class resolution, \"{}\" was found in both \"{}\" and \"{}\", the first will be used.</warning>",
                    class_name,
                    class_map.get_class_path(class_name),
                    implode("\", \"", ambiguous_paths)
                ));
            }
        }
        if !ambiguous_classes.is_empty() {
            self.io.write_error(&format!(
                "<info>To resolve ambiguity in classes not under your control you can ignore them by path using <href={}>exclude-from-classmap</>",
                OutputFormatter::escape("https://getcomposer.org/doc/04-schema.md#exclude-files-from-classmaps")
            ));
        }

        // output PSR violations which are not coming from the vendor dir
        class_map.clear_psr_violations_by_path(&vendor_path);
        for msg in class_map.get_psr_violations() {
            self.io.write_error(&format!("<warning>{}</warning>", msg));
        }

        class_map.add_class("Composer\\InstalledVersions".to_string(), format!("{}/composer/InstalledVersions.php", vendor_path));
        class_map.sort();

        let mut classmap_file = format!(
            "<?php\n\n// autoload_classmap.php @generated by Composer\n\n$vendorDir = {};\n$baseDir = {};\n\nreturn array(\n",
            vendor_path_code, app_base_dir_code
        );
        for (class_name, path) in class_map.get_map() {
            let path_code = format!("{},\n", self.get_path_code(&filesystem, &base_path, &vendor_path, &path));
            classmap_file.push_str(&format!("    {} => {}", var_export(&PhpMixed::String(class_name.clone()), true), path_code));
        }
        classmap_file.push_str(");\n");

        let mut suffix = suffix;
        if suffix.as_deref() == Some("") {
            suffix = None;
        }
        if suffix.is_none() {
            suffix = config.get("autoloader-suffix").as_string().map(|s| s.to_string());

            // carry over existing autoload.php's suffix if possible and none is configured
            if suffix.is_none() && Filesystem::is_readable(&format!("{}/autoload.php", vendor_path)) {
                let content = file_get_contents(&format!("{}/autoload.php", vendor_path)).unwrap_or_default();
                let mut matches: Vec<String> = vec![];
                if Preg::is_match("{ComposerAutoloaderInit([^:\\s]+)::}", &content, Some(&mut matches)).unwrap_or(false) {
                    suffix = matches.get(1).cloned();
                }
            }

            if suffix.is_none() {
                suffix = Some(if let Some(l) = locker {
                    if l.is_locked() {
                        l.get_lock_data().get("content-hash").and_then(|v| v.as_string()).unwrap_or("").to_string()
                    } else {
                        bin2hex(&random_bytes(16))
                    }
                } else {
                    bin2hex(&random_bytes(16))
                });
            }
        }
        let suffix = suffix.unwrap_or_default();

        if self.dry_run {
            return Ok(class_map);
        }

        filesystem.file_put_contents_if_modified(&format!("{}/autoload_namespaces.php", target_dir), &namespaces_file)?;
        filesystem.file_put_contents_if_modified(&format!("{}/autoload_psr4.php", target_dir), &psr4_file)?;
        filesystem.file_put_contents_if_modified(&format!("{}/autoload_classmap.php", target_dir), &classmap_file)?;
        let include_path_file_path = format!("{}/include_paths.php", target_dir);
        let include_path_file_contents = self.get_include_paths_file(&package_map, &filesystem, &base_path, &vendor_path, &vendor_path_code, &app_base_dir_code);
        if let Some(ref c) = include_path_file_contents {
            filesystem.file_put_contents_if_modified(&include_path_file_path, c)?;
        } else if file_exists(&include_path_file_path) {
            unlink(&include_path_file_path);
        }
        let include_files_file_path = format!("{}/autoload_files.php", target_dir);
        let files_map = autoloads.get("files").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let mut files_str_map: IndexMap<String, String> = IndexMap::new();
        for (k, v) in &files_map {
            files_str_map.insert(k.clone(), v.as_string().unwrap_or("").to_string());
        }
        let include_files_file_contents = self.get_include_files_file(&files_str_map, &filesystem, &base_path, &vendor_path, &vendor_path_code, &app_base_dir_code);
        if let Some(ref c) = include_files_file_contents {
            filesystem.file_put_contents_if_modified(&include_files_file_path, c)?;
        } else if file_exists(&include_files_file_path) {
            unlink(&include_files_file_path);
        }
        filesystem.file_put_contents_if_modified(
            &format!("{}/autoload_static.php", target_dir),
            &self.get_static_file(&suffix, &target_dir, &vendor_path, &base_path),
        )?;
        let mut check_platform = config.get("platform-check").as_bool() != Some(false)
            && self
                .platform_requirement_filter
                .as_any()
                .downcast_ref::<IgnoreAllPlatformRequirementFilter>()
                .is_none();
        let mut platform_check_content: Option<String> = None;
        if check_platform {
            platform_check_content = self.get_platform_check(
                &package_map,
                config.get("platform-check").clone(),
                &dev_package_names,
            );
            if platform_check_content.is_none() {
                check_platform = false;
            }
        }
        if check_platform {
            filesystem.file_put_contents_if_modified(
                &format!("{}/platform_check.php", target_dir),
                platform_check_content.as_ref().unwrap(),
            )?;
        } else if file_exists(&format!("{}/platform_check.php", target_dir)) {
            unlink(&format!("{}/platform_check.php", target_dir));
        }
        filesystem.file_put_contents_if_modified(
            &format!("{}/autoload.php", vendor_path),
            &self.get_autoload_file(&vendor_path_to_target_dir_code, &suffix),
        )?;
        filesystem.file_put_contents_if_modified(
            &format!("{}/autoload_real.php", target_dir),
            &self.get_autoload_real_file(
                true,
                include_path_file_contents.is_some(),
                target_dir_loader.clone(),
                include_files_file_contents.is_some(),
                &vendor_path_code,
                &app_base_dir_code,
                &suffix,
                use_global_include_path,
                prepend_autoloader,
                check_platform,
            ),
        )?;

        // PHP: __DIR__ refers to the directory of AutoloadGenerator.php
        filesystem.safe_copy(
            &format!("{}/ClassLoader.php", "composer/src/Composer/Autoload"),
            &format!("{}/ClassLoader.php", target_dir),
        )?;
        filesystem.safe_copy(
            &format!("{}/../../../LICENSE", "composer/src/Composer/Autoload"),
            &format!("{}/LICENSE", target_dir),
        )?;

        if self.run_scripts {
            let mut additional_args: IndexMap<String, PhpMixed> = IndexMap::new();
            additional_args.insert("optimize".to_string(), PhpMixed::Bool(scan_psr_packages));
            self.event_dispatcher.dispatch_script_with_args(
                ScriptEvents::POST_AUTOLOAD_DUMP,
                self.dev_mode.unwrap_or(false),
                vec![],
                additional_args,
            );
        }

        Ok(class_map)
    }

    fn build_exclusion_regex(&self, dir: &str, excluded: Vec<String>) -> Option<String> {
        let mut excluded = excluded;
        if excluded.is_empty() {
            return None;
        }

        // filter excluded patterns here to only use those matching $dir
        // exclude-from-classmap patterns are all realpath'd so we can only filter them if $dir exists so that realpath($dir) will work
        // if $dir does not exist, it should anyway not find anything there so no trouble
        if file_exists(dir) {
            // transform $dir in the same way that exclude-from-classmap patterns are transformed so we can match them against each other
            let dir_match = preg_quote(&strtr(&realpath(dir).unwrap_or_default(), "\\", "/"), None);
            // also match against the non-realpath version for symlinks
            let fs = Filesystem::new(None);
            let abs_dir = if fs.is_absolute_path(dir) {
                dir.to_string()
            } else {
                format!("{}/{}", realpath(&Platform::get_cwd()).unwrap_or_default(), dir)
            };
            let dir_match_normalized = preg_quote(&strtr(&fs.normalize_path(&abs_dir), "\\", "/"), None);
            let is_symlink = dir_match != dir_match_normalized;

            let mut new_excluded: Vec<String> = vec![];
            for pattern in &excluded {
                // extract the constant string prefix of the pattern here, until we reach a non-escaped regex special character
                let pattern_processed = Preg::replace(
                    "{^(([^.+*?\\[^\\]$(){}=!<>|:\\\\#-]+|\\\\[.+*?\\[^\\]$(){}=!<>|:#-])*).*}",
                    "$1",
                    pattern,
                );
                // if the pattern is not a subset or superset of $dir, it is unrelated and we skip it
                let unrelated = (!str_starts_with(&pattern_processed, &dir_match)
                    && !str_starts_with(&dir_match, &pattern_processed))
                    && (!is_symlink
                        || (!str_starts_with(&pattern_processed, &dir_match_normalized)
                            && !str_starts_with(&dir_match_normalized, &pattern_processed)));
                if !unrelated {
                    new_excluded.push(pattern.clone());
                }
            }
            excluded = new_excluded;
        }

        if !excluded.is_empty() {
            Some(format!("{{({})}}", implode("|", &excluded)))
        } else {
            None
        }
    }

    pub fn build_package_map(
        &self,
        installation_manager: &InstallationManager,
        root_package: &dyn RootPackageInterface,
        packages: Vec<Box<dyn PackageInterface>>,
    ) -> anyhow::Result<Vec<(Box<dyn PackageInterface>, Option<String>)>> {
        // build package => install path map
        let mut package_map: Vec<(Box<dyn PackageInterface>, Option<String>)> =
            vec![(root_package.clone_as_package_interface(), Some(String::new()))];

        for package in packages {
            if package.as_alias_package().is_some() {
                continue;
            }
            self.validate_package(&*package)?;
            let install_path = installation_manager.get_install_path(&*package);
            package_map.push((package, install_path));
        }

        Ok(package_map)
    }

    /// Throws InvalidArgumentException if the package has illegal settings.
    pub(crate) fn validate_package(&self, package: &dyn PackageInterface) -> anyhow::Result<()> {
        let autoload = package.get_autoload();
        if autoload.get("psr-4").map_or(false, |v| !v.is_empty()) && package.get_target_dir().is_some() {
            let name = package.get_name();
            let _ = package.get_target_dir();
            return Err(InvalidArgumentException {
                message: format!("PSR-4 autoloading is incompatible with the target-dir property, remove the target-dir in package '{}'.", name),
                code: 0,
            }
            .into());
        }
        if let Some(psr4) = autoload.get("psr-4").and_then(|v| v.as_array()) {
            for (namespace, _dirs) in psr4 {
                if !namespace.is_empty() && !namespace.ends_with('\\') {
                    return Err(InvalidArgumentException {
                        message: format!("psr-4 namespaces must end with a namespace separator, '{}' does not, use '{}\\'.", namespace, namespace),
                        code: 0,
                    }
                    .into());
                }
            }
        }
        Ok(())
    }

    /// Compiles an ordered list of namespace => path mappings
    pub fn parse_autoloads(
        &self,
        package_map: &Vec<(Box<dyn PackageInterface>, Option<String>)>,
        root_package: &dyn RootPackageInterface,
        filtered_dev_packages: PhpMixed,
    ) -> IndexMap<String, PhpMixed> {
        let mut package_map = package_map.clone();
        let root_package_map = array_shift(&mut package_map).unwrap();
        let package_map = if is_array(&filtered_dev_packages) {
            let dev_list = filtered_dev_packages
                .as_list()
                .map(|l| l.iter().filter_map(|v| v.as_string().map(|s| s.to_string())).collect::<Vec<_>>())
                .unwrap_or_default();
            array_filter(package_map, |item: &(Box<dyn PackageInterface>, Option<String>)| -> bool {
                !in_array(item.0.get_name(), &dev_list, true)
            })
        } else if filtered_dev_packages.as_bool() == Some(true) {
            self.filter_package_map(package_map, root_package)
        } else {
            package_map
        };
        let mut sorted_package_map = self.sort_package_map(package_map);
        sorted_package_map.push(root_package_map);
        let reverse_sorted_map = array_reverse(sorted_package_map.clone());

        // reverse-sorted means root first, then dependents, then their dependents, etc.
        // which makes sense to allow root to override classmap or psr-0/4 entries with higher precedence rules
        let mut psr0 = self.parse_autoloads_type(&reverse_sorted_map, "psr-0", root_package);
        let mut psr4 = self.parse_autoloads_type(&reverse_sorted_map, "psr-4", root_package);
        let classmap = self.parse_autoloads_type(&reverse_sorted_map, "classmap", root_package);

        // sorted (i.e. dependents first) for files to ensure that dependencies are loaded/available once a file is included
        let files = self.parse_autoloads_type(&sorted_package_map, "files", root_package);
        // using sorted here but it does not really matter as all are excluded equally
        let exclude = self.parse_autoloads_type(&sorted_package_map, "exclude-from-classmap", root_package);

        krsort(&mut psr0);
        krsort(&mut psr4);

        let mut result: IndexMap<String, PhpMixed> = IndexMap::new();
        result.insert("psr-0".to_string(), PhpMixed::Array(psr0));
        result.insert("psr-4".to_string(), PhpMixed::Array(psr4));
        result.insert("classmap".to_string(), PhpMixed::Array(classmap));
        result.insert("files".to_string(), PhpMixed::Array(files));
        result.insert("exclude-from-classmap".to_string(), PhpMixed::Array(exclude));
        result
    }

    /// Registers an autoloader based on an autoload-map returned by parseAutoloads
    pub fn create_loader(&self, autoloads: &IndexMap<String, PhpMixed>, vendor_dir: Option<String>) -> ClassLoader {
        let mut loader = ClassLoader::new(vendor_dir);

        if let Some(psr0) = autoloads.get("psr-0").and_then(|v| v.as_array()) {
            for (namespace, path) in psr0 {
                loader.add(namespace.clone(), (**path).clone());
            }
        }

        if let Some(psr4) = autoloads.get("psr-4").and_then(|v| v.as_array()) {
            for (namespace, path) in psr4 {
                loader.add_psr4(namespace.clone(), (**path).clone());
            }
        }

        if let Some(classmap) = autoloads.get("classmap").and_then(|v| v.as_list()) {
            let mut excluded: Vec<String> = vec![];
            if let Some(ex) = autoloads.get("exclude-from-classmap").and_then(|v| v.as_list()) {
                if !ex.is_empty() {
                    excluded = ex.iter().filter_map(|v| v.as_string().map(|s| s.to_string())).collect();
                }
            }

            let mut class_map_generator = ClassMapGenerator::new(vec!["php".to_string(), "inc".to_string(), "hh".to_string()]);
            class_map_generator.avoid_duplicate_scans();

            for dir in classmap {
                let dir_str = dir.as_string().unwrap_or("");
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    class_map_generator.scan_paths(dir_str, self.build_exclusion_regex(dir_str, excluded.clone()), "classmap", "");
                }));
                if let Err(_e) = res {
                    self.io.write_error(&format!("<warning>{}</warning>", "scan failure"));
                }
            }

            loader.add_class_map(class_map_generator.get_class_map().get_map());
        }

        loader
    }

    pub(crate) fn get_include_paths_file(
        &self,
        package_map: &Vec<(Box<dyn PackageInterface>, Option<String>)>,
        filesystem: &Filesystem,
        base_path: &str,
        vendor_path: &str,
        vendor_path_code: &str,
        app_base_dir_code: &str,
    ) -> Option<String> {
        let mut include_paths: Vec<String> = vec![];

        for item in package_map {
            let (package, install_path) = item;

            // packages that are not installed cannot autoload anything
            let install_path = match install_path {
                Some(p) => p.clone(),
                None => continue,
            };

            let mut install_path = install_path;
            if let Some(target_dir) = package.get_target_dir() {
                if !target_dir.is_empty() {
                    let suffix_to_remove = format!("/{}", target_dir);
                    install_path = substr(&install_path, 0, Some(-(suffix_to_remove.len() as isize)));
                }
            }

            for include_path in package.get_include_paths() {
                let include_path = trim(&include_path, "/");
                include_paths.push(if install_path.is_empty() {
                    include_path
                } else {
                    format!("{}/{}", install_path, include_path)
                });
            }
        }

        if include_paths.is_empty() {
            return None;
        }

        let mut include_paths_code = String::new();
        for path in &include_paths {
            include_paths_code.push_str(&format!(
                "    {},\n",
                self.get_path_code(filesystem, base_path, vendor_path, path)
            ));
        }

        Some(format!(
            "<?php\n\n// include_paths.php @generated by Composer\n\n$vendorDir = {};\n$baseDir = {};\n\nreturn array(\n{});\n",
            vendor_path_code, app_base_dir_code, include_paths_code
        ))
    }

    pub(crate) fn get_include_files_file(
        &self,
        files: &IndexMap<String, String>,
        filesystem: &Filesystem,
        base_path: &str,
        vendor_path: &str,
        vendor_path_code: &str,
        app_base_dir_code: &str,
    ) -> Option<String> {
        // Get the path to each file, and make sure these paths are unique.
        let mut files: IndexMap<String, String> = files
            .iter()
            .map(|(k, function_file)| {
                (k.clone(), self.get_path_code(filesystem, base_path, vendor_path, function_file))
            })
            .collect();
        let unique_files: Vec<String> = array_unique(files.values().cloned().collect());
        if unique_files.len() < files.len() {
            self.io.write_error("<warning>The following \"files\" autoload rules are included multiple times, this may cause issues and should be resolved:</warning>");
            // duplicates: array_diff_assoc(files, unique_files)
            let mut seen: indexmap::IndexSet<String> = indexmap::IndexSet::new();
            let mut duplicates: Vec<String> = vec![];
            for v in files.values() {
                if !seen.insert(v.clone()) {
                    duplicates.push(v.clone());
                }
            }
            for duplicate_file in array_unique(duplicates) {
                self.io.write_error(&format!("<warning> - {}</warning>", duplicate_file));
            }
        }
        let _ = unique_files;

        let mut files_code = String::new();

        for (file_identifier, function_file) in &files {
            files_code.push_str(&format!(
                "    {} => {},\n",
                var_export(&PhpMixed::String(file_identifier.clone()), true),
                function_file
            ));
        }

        if files_code.is_empty() {
            return None;
        }

        // pre-mutate to avoid borrow conflict
        let _ = &mut files;

        Some(format!(
            "<?php\n\n// autoload_files.php @generated by Composer\n\n$vendorDir = {};\n$baseDir = {};\n\nreturn array(\n{});\n",
            vendor_path_code, app_base_dir_code, files_code
        ))
    }

    pub(crate) fn get_path_code(
        &self,
        filesystem: &Filesystem,
        base_path: &str,
        vendor_path: &str,
        path: &str,
    ) -> String {
        let mut path = if !filesystem.is_absolute_path(path) {
            format!("{}/{}", base_path, path)
        } else {
            path.to_string()
        };
        path = filesystem.normalize_path(&path);

        let mut base_dir = String::new();
        if strpos(&format!("{}/", path), &format!("{}/", vendor_path)) == Some(0) {
            path = substr(&path, vendor_path.len() as isize, None);
            base_dir = "$vendorDir . ".to_string();
        } else {
            path = filesystem.normalize_path(&filesystem.find_shortest_path(base_path, &path, true));
            if !filesystem.is_absolute_path(&path) {
                base_dir = "$baseDir . ".to_string();
                path = format!("/{}", path);
            }
        }

        if Preg::is_match("{\\.phar([\\\\/]|$)}", &path, None).unwrap_or(false) {
            base_dir = format!("'phar://' . {}", base_dir);
        }

        format!("{}{}", base_dir, var_export(&PhpMixed::String(path), true))
    }

    pub(crate) fn get_platform_check(
        &self,
        package_map: &Vec<(Box<dyn PackageInterface>, Option<String>)>,
        check_platform: PhpMixed,
        dev_package_names: &Vec<String>,
    ) -> Option<String> {
        let mut lowest_php_version = Bound::zero();
        let mut required_php_64bit = false;
        let mut required_extensions: IndexMap<String, String> = IndexMap::new();
        let mut extension_providers: IndexMap<String, Vec<Box<dyn shirabe_semver::constraint::constraint_interface::ConstraintInterface>>> = IndexMap::new();

        for item in package_map {
            let package = &item.0;
            let mut links = package.get_replaces();
            for (k, v) in package.get_provides() {
                links.insert(k, v);
            }
            for (_k, link) in &links {
                let mut matches: Vec<String> = vec![];
                if Preg::is_match("{^ext-(.+)$}iD", link.get_target(), Some(&mut matches)).unwrap_or(false) {
                    extension_providers
                        .entry(matches[1].clone())
                        .or_insert_with(Vec::new)
                        .push(link.get_constraint());
                }
            }
        }

        'outer: for item in package_map {
            let package = &item.0;
            // skip dev dependencies platform requirements as platform-check really should only be a production safeguard
            if in_array(package.get_name(), dev_package_names, true) {
                continue;
            }

            for (_k, link) in &package.get_requires() {
                if self.platform_requirement_filter.is_ignored(link.get_target()) {
                    continue;
                }

                if in_array(link.get_target(), &vec!["php".to_string(), "php-64bit".to_string()], true) {
                    let constraint = link.get_constraint();
                    if constraint.get_lower_bound().compare_to(&lowest_php_version, ">") {
                        lowest_php_version = constraint.get_lower_bound();
                    }
                }

                if "php-64bit" == link.get_target() {
                    required_php_64bit = true;
                }

                let mut matches: Vec<String> = vec![];
                if check_platform.as_bool() == Some(true)
                    && Preg::is_match("{^ext-(.+)$}iD", link.get_target(), Some(&mut matches)).unwrap_or(false)
                {
                    // skip extension checks if they have a valid provider/replacer
                    if let Some(provided_list) = extension_providers.get(&matches[1]) {
                        for provided in provided_list {
                            if provided.matches(&*link.get_constraint()) {
                                continue 'outer;
                            }
                        }
                    }

                    let ext_name = if matches[1] == "zend-opcache" {
                        "zend opcache".to_string()
                    } else {
                        matches[1].clone()
                    };

                    let extension = var_export(&PhpMixed::String(ext_name.clone()), true);
                    if ext_name == "pcntl" || ext_name == "readline" {
                        required_extensions.insert(
                            extension.clone(),
                            format!(
                                "PHP_SAPI !== 'cli' || extension_loaded({}) || $missingExtensions[] = {};\n",
                                extension, extension
                            ),
                        );
                    } else {
                        required_extensions.insert(
                            extension.clone(),
                            format!(
                                "extension_loaded({}) || $missingExtensions[] = {};\n",
                                extension, extension
                            ),
                        );
                    }
                }
            }
        }

        ksort(&mut required_extensions);

        let format_to_php_version_id = |bound: &Bound| -> i64 {
            if bound.is_zero() {
                return 0;
            }

            if bound.is_positive_infinity() {
                return 99999;
            }

            let version = str_replace("-", ".", bound.get_version());
            let chunks: Vec<i64> = explode(".", &version)
                .into_iter()
                .map(|s| shirabe_php_shim::intval(&s))
                .collect();

            chunks[0] * 10000 + chunks[1] * 100 + chunks[2]
        };

        let format_to_human_readable = |bound: &Bound| -> PhpMixed {
            if bound.is_zero() {
                return PhpMixed::Int(0);
            }

            if bound.is_positive_infinity() {
                return PhpMixed::Int(99999);
            }

            let version = str_replace("-", ".", bound.get_version());
            let chunks = explode(".", &version);
            let chunks = array_slice(&chunks, 0, Some(3), false);

            PhpMixed::String(implode(".", &chunks))
        };

        let mut required_php = String::new();
        let mut required_php_error = String::new();
        if !lowest_php_version.is_zero() {
            let operator = if lowest_php_version.is_inclusive() { ">=" } else { ">" };
            required_php = format!("PHP_VERSION_ID {} {}", operator, format_to_php_version_id(&lowest_php_version));
            let human_readable = format_to_human_readable(&lowest_php_version);
            required_php_error = format!(
                "\"{} {}\"",
                operator,
                match &human_readable {
                    PhpMixed::String(s) => s.clone(),
                    PhpMixed::Int(i) => i.to_string(),
                    _ => String::new(),
                }
            );
        }

        if !required_php.is_empty() {
            required_php = format!(
                "\nif (!({})) {{\n    $issues[] = 'Your Composer dependencies require a PHP version {}. You are running ' . PHP_VERSION . '.';\n}}\n",
                required_php, required_php_error
            );
        }

        if required_php_64bit {
            required_php.push_str("\nif (PHP_INT_SIZE !== 8) {\n    $issues[] = 'Your Composer dependencies require a 64-bit build of PHP.';\n}\n");
        }

        let required_extensions_str = implode("", &required_extensions.values().cloned().collect::<Vec<_>>());
        let required_extensions_block = if !required_extensions_str.is_empty() {
            format!(
                "\n$missingExtensions = array();\n{}\nif ($missingExtensions) {{\n    $issues[] = 'Your Composer dependencies require the following PHP extensions to be installed: ' . implode(', ', $missingExtensions) . '.';\n}}\n",
                required_extensions_str
            )
        } else {
            String::new()
        };

        if required_php.is_empty() && required_extensions_block.is_empty() {
            return None;
        }

        Some(format!(
            "<?php\n\n// platform_check.php @generated by Composer\n\n$issues = array();\n{}{}\nif ($issues) {{\n    if (!headers_sent()) {{\n        header('HTTP/1.1 500 Internal Server Error');\n    }}\n    if (!ini_get('display_errors')) {{\n        if (PHP_SAPI === 'cli' || PHP_SAPI === 'phpdbg') {{\n            fwrite(STDERR, 'Composer detected issues in your platform:' . PHP_EOL.PHP_EOL . implode(PHP_EOL, $issues) . PHP_EOL.PHP_EOL);\n        }} elseif (!headers_sent()) {{\n            echo 'Composer detected issues in your platform:' . PHP_EOL.PHP_EOL . str_replace('You are running '.PHP_VERSION.'.', '', implode(PHP_EOL, $issues)) . PHP_EOL.PHP_EOL;\n        }}\n    }}\n    throw new \\RuntimeException(\n        'Composer detected issues in your platform: ' . implode(' ', $issues)\n    );\n}}\n",
            required_php, required_extensions_block
        ))
    }

    pub(crate) fn get_autoload_file(&self, vendor_path_to_target_dir_code: &str, suffix: &str) -> String {
        let last_char = vendor_path_to_target_dir_code
            .chars()
            .nth(vendor_path_to_target_dir_code.len() - 1)
            .unwrap_or(' ');
        let vendor_path_to_target_dir_code = if last_char == '\'' || last_char == '"' {
            format!(
                "{}/autoload_real.php{}",
                substr(vendor_path_to_target_dir_code, 0, Some(-1)),
                last_char
            )
        } else {
            format!("{} . '/autoload_real.php'", vendor_path_to_target_dir_code)
        };

        format!(
            "<?php\n\n// autoload.php @generated by Composer\n\nif (PHP_VERSION_ID < 50600) {{\n    if (!headers_sent()) {{\n        header('HTTP/1.1 500 Internal Server Error');\n    }}\n    $err = 'Composer 2.3.0 dropped support for autoloading on PHP <5.6 and you are running '.PHP_VERSION.', please upgrade PHP or use Composer 2.2 LTS via \"composer self-update --2.2\". Aborting.'.PHP_EOL;\n    if (!ini_get('display_errors')) {{\n        if (PHP_SAPI === 'cli' || PHP_SAPI === 'phpdbg') {{\n            fwrite(STDERR, $err);\n        }} elseif (!headers_sent()) {{\n            echo $err;\n        }}\n    }}\n    throw new RuntimeException($err);\n}}\n\nrequire_once {};\n\nreturn ComposerAutoloaderInit{}::getLoader();\n",
            vendor_path_to_target_dir_code, suffix
        )
    }

    /// Note: vendor_path_code and app_base_dir_code are unused in this method
    pub(crate) fn get_autoload_real_file(
        &self,
        _use_class_map: bool,
        use_include_path: bool,
        target_dir_loader: Option<String>,
        use_include_files: bool,
        _vendor_path_code: &str,
        _app_base_dir_code: &str,
        suffix: &str,
        use_global_include_path: bool,
        prepend_autoloader: &str,
        check_platform: bool,
    ) -> String {
        let mut file = format!(
            "<?php\n\n// autoload_real.php @generated by Composer\n\nclass ComposerAutoloaderInit{}\n{{\n    private static $loader;\n\n    public static function loadClassLoader($class)\n    {{\n        if ('Composer\\Autoload\\ClassLoader' === $class) {{\n            require __DIR__ . '/ClassLoader.php';\n        }}\n    }}\n\n    /**\n     * @return \\Composer\\Autoload\\ClassLoader\n     */\n    public static function getLoader()\n    {{\n        if (null !== self::$loader) {{\n            return self::$loader;\n        }}\n\n\n",
            suffix
        );

        if check_platform {
            file.push_str("        require __DIR__ . '/platform_check.php';\n\n\n");
        }

        file.push_str(&format!(
            "        spl_autoload_register(array('ComposerAutoloaderInit{}', 'loadClassLoader'), true, {});\n        self::$loader = $loader = new \\Composer\\Autoload\\ClassLoader(\\dirname(__DIR__));\n        spl_autoload_unregister(array('ComposerAutoloaderInit{}', 'loadClassLoader'));\n\n",
            suffix, prepend_autoloader, suffix
        ));

        if use_include_path {
            file.push_str("        $includePaths = require __DIR__ . '/include_paths.php';\n        $includePaths[] = get_include_path();\n        set_include_path(implode(PATH_SEPARATOR, $includePaths));\n\n\n");
        }

        // keeping PHP 5.6+ compatibility for the autoloader here by using call_user_func vs getInitializer()()
        file.push_str(&format!(
            "        require __DIR__ . '/autoload_static.php';\n        call_user_func(\\Composer\\Autoload\\ComposerStaticInit{}::getInitializer($loader));\n\n\n",
            suffix
        ));

        if self.class_map_authoritative {
            file.push_str("        $loader->setClassMapAuthoritative(true);\n");
        }

        if self.apcu {
            let apcu_prefix = var_export(
                &PhpMixed::String(if let Some(ref prefix) = self.apcu_prefix {
                    prefix.clone()
                } else {
                    bin2hex(&random_bytes(10))
                }),
                true,
            );
            file.push_str(&format!("        $loader->setApcuPrefix({});\n", apcu_prefix));
        }

        if use_global_include_path {
            file.push_str("        $loader->setUseIncludePath(true);\n");
        }

        if target_dir_loader.is_some() {
            file.push_str(&format!(
                "        spl_autoload_register(array('ComposerAutoloaderInit{}', 'autoload'), true, true);\n\n\n",
                suffix
            ));
        }

        file.push_str(&format!(
            "        $loader->register({});\n\n\n",
            prepend_autoloader
        ));

        if use_include_files {
            file.push_str(&format!(
                "        $filesToLoad = \\Composer\\Autoload\\ComposerStaticInit{}::$files;\n        $requireFile = \\Closure::bind(static function ($fileIdentifier, $file) {{\n            if (empty($GLOBALS['__composer_autoload_files'][$fileIdentifier])) {{\n                $GLOBALS['__composer_autoload_files'][$fileIdentifier] = true;\n\n                require $file;\n            }}\n        }}, null, null);\n        foreach ($filesToLoad as $fileIdentifier => $file) {{\n            $requireFile($fileIdentifier, $file);\n        }}\n\n\n",
                suffix
            ));
        }

        file.push_str("        return $loader;\n    }\n\n");

        if let Some(target_dir_loader_str) = target_dir_loader {
            file.push_str(&target_dir_loader_str);
        }

        format!("{}}}\n", file)
    }

    pub(crate) fn get_static_file(
        &self,
        suffix: &str,
        target_dir: &str,
        vendor_path: &str,
        base_path: &str,
    ) -> String {
        let mut file = format!(
            "<?php\n\n// autoload_static.php @generated by Composer\n\nnamespace Composer\\Autoload;\n\nclass ComposerStaticInit{}\n{{\n",
            suffix
        );

        let mut loader = ClassLoader::new(None);

        // PHP: $map = require $targetDir . '/autoload_namespaces.php';
        let map = shirabe_php_shim::php_require(&format!("{}/autoload_namespaces.php", target_dir));
        if let Some(map_arr) = map.as_array() {
            for (namespace, path) in map_arr {
                loader.set(namespace.clone(), (**path).clone());
            }
        }

        let map = shirabe_php_shim::php_require(&format!("{}/autoload_psr4.php", target_dir));
        if let Some(map_arr) = map.as_array() {
            for (namespace, path) in map_arr {
                loader.set_psr4(namespace.clone(), (**path).clone());
            }
        }

        let class_map = shirabe_php_shim::php_require(&format!("{}/autoload_classmap.php", target_dir));
        if class_map.as_bool() != Some(false) && !class_map.is_null() {
            if let Some(cm) = class_map.as_array() {
                let cm_str: IndexMap<String, String> = cm
                    .iter()
                    .map(|(k, v)| (k.clone(), v.as_string().unwrap_or("").to_string()))
                    .collect();
                loader.add_class_map(cm_str);
            }
        }

        let filesystem = Filesystem::new(None);

        let vendor_path_code = format!(
            " => {} . '/",
            filesystem.find_shortest_path_code(&realpath(target_dir).unwrap_or_default(), vendor_path, true, true)
        );
        let vendor_phar_path_code = format!(
            " => 'phar://' . {} . '/",
            filesystem.find_shortest_path_code(&realpath(target_dir).unwrap_or_default(), vendor_path, true, true)
        );
        let app_base_dir_code = format!(
            " => {} . '/",
            filesystem.find_shortest_path_code(&realpath(target_dir).unwrap_or_default(), base_path, true, true)
        );
        let app_base_dir_phar_code = format!(
            " => 'phar://' . {} . '/",
            filesystem.find_shortest_path_code(&realpath(target_dir).unwrap_or_default(), base_path, true, true)
        );

        // PHP: ' => ' . substr(var_export(rtrim($vendorDir, '\\/') . '/', true), 0, -1)
        let absolute_vendor_path_code = format!(
            " => {}",
            substr(
                &var_export(&PhpMixed::String(format!("{}/", shirabe_php_shim::rtrim(vendor_path, "\\/"))), true),
                0,
                Some(-1),
            )
        );
        let absolute_vendor_phar_path_code = format!(
            " => {}",
            substr(
                &var_export(&PhpMixed::String(format!("{}/", shirabe_php_shim::rtrim(&format!("phar://{}", vendor_path), "\\/"))), true),
                0,
                Some(-1),
            )
        );
        let absolute_app_base_dir_code = format!(
            " => {}",
            substr(
                &var_export(&PhpMixed::String(format!("{}/", shirabe_php_shim::rtrim(base_path, "\\/"))), true),
                0,
                Some(-1),
            )
        );
        let absolute_app_base_dir_phar_code = format!(
            " => {}",
            substr(
                &var_export(&PhpMixed::String(format!("{}/", shirabe_php_shim::rtrim(&format!("phar://{}", base_path), "\\/"))), true),
                0,
                Some(-1),
            )
        );

        let mut initializer = String::new();
        let prefix = "\0Composer\\Autoload\\ClassLoader\0";
        let prefix_len = strlen(prefix);
        let mut maps: IndexMap<String, PhpMixed> = IndexMap::new();
        if file_exists(&format!("{}/autoload_files.php", target_dir)) {
            maps.insert(
                "files".to_string(),
                shirabe_php_shim::php_require(&format!("{}/autoload_files.php", target_dir)),
            );
        }

        // PHP: foreach ((array) $loader as $prop => $value) — iterate over the loader's properties
        for (prop, value) in loader.as_array_iter() {
            if !is_array(&value) || value.as_array().map_or(0, |a| a.len()) == 0
                || !str_starts_with(&prop, prefix)
            {
                continue;
            }
            maps.insert(substr(&prop, prefix_len as isize, None), value);
        }

        for (prop, value) in &maps {
            let value = strtr(
                &var_export(value, true),
                &{
                    let mut m: IndexMap<String, String> = IndexMap::new();
                    m.insert(absolute_vendor_path_code.clone(), vendor_path_code.clone());
                    m.insert(absolute_vendor_phar_path_code.clone(), vendor_phar_path_code.clone());
                    m.insert(absolute_app_base_dir_code.clone(), app_base_dir_code.clone());
                    m.insert(absolute_app_base_dir_phar_code.clone(), app_base_dir_phar_code.clone());
                    m
                },
            );
            let value = shirabe_php_shim::ltrim(&Preg::replace("/^ */m", "    $0$0", &value), None);
            let value = Preg::replace("/ +$/m", "", &value);

            file.push_str(&sprintf(
                "    public static $%s = %s;\n\n",
                &[prop.clone().into(), value.clone().into()],
            ));
            if "files" != prop.as_str() {
                initializer.push_str(&format!(
                    "            $loader->{} = ComposerStaticInit{}::${};\n",
                    prop, suffix, prop
                ));
            }
        }

        format!(
            "{}    public static function getInitializer(ClassLoader $loader)\n    {{\n        return \\Closure::bind(function () use ($loader) {{\n{}        }}, null, ClassLoader::class);\n    }}\n}}\n",
            file, initializer
        )
    }

    pub(crate) fn parse_autoloads_type(
        &self,
        package_map: &Vec<(Box<dyn PackageInterface>, Option<String>)>,
        r#type: &str,
        root_package: &dyn RootPackageInterface,
    ) -> IndexMap<String, Box<PhpMixed>> {
        let mut autoloads: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
        let mut numeric_index: i64 = 0;

        for item in package_map {
            let (package, install_path) = item;

            // packages that are not installed cannot autoload anything
            let install_path = match install_path {
                Some(p) => p.clone(),
                None => continue,
            };

            let mut autoload = package.get_autoload();
            // PHP comparison: $package === $rootPackage (object identity). We compare by name as best-effort.
            let is_root = package.get_name() == root_package.get_name();
            if self.dev_mode.unwrap_or(false) && is_root {
                autoload = array_merge_recursive(autoload, root_package.get_dev_autoload());
            }

            // skip misconfigured packages
            let type_value = match autoload.get(r#type) {
                Some(v) => v,
                None => continue,
            };
            if !is_array(type_value) {
                continue;
            }
            let mut install_path = install_path;
            if package.get_target_dir().is_some() && !is_root {
                let suffix_to_remove = format!("/{}", package.get_target_dir().unwrap_or_default());
                install_path = substr(&install_path, 0, Some(-(suffix_to_remove.len() as isize)));
            }

            let type_arr = type_value.as_array().cloned().unwrap_or_default();
            for (namespace, paths) in type_arr {
                let namespace = if in_array(r#type, &vec!["psr-4".to_string(), "psr-0".to_string()], true) {
                    // normalize namespaces to ensure "\" becomes "" and others do not have leading separators as they are not needed
                    ltrim(&namespace, "\\")
                } else {
                    namespace
                };
                // PHP: foreach ((array) $paths as $path) — handles scalar by wrapping in an array
                let path_list: Vec<PhpMixed> = match paths.as_ref() {
                    PhpMixed::List(l) => l.iter().map(|b| (**b).clone()).collect(),
                    PhpMixed::Array(a) => a.values().map(|b| (**b).clone()).collect(),
                    other => vec![other.clone()],
                };
                for path in path_list {
                    let mut path_str = path.as_string().unwrap_or("").to_string();
                    if (r#type == "files" || r#type == "classmap" || r#type == "exclude-from-classmap")
                        && package.get_target_dir().is_some()
                        && !Filesystem::is_readable(&format!("{}/{}", install_path, path_str))
                    {
                        // remove target-dir from file paths of the root package
                        if is_root {
                            let target_dir = str_replace(
                                "\\<dirsep\\>",
                                "[\\\\/]",
                                &preg_quote(
                                    &str_replace_multi(&package.get_target_dir().unwrap_or_default(), &[("/", "<dirsep>"), ("\\", "<dirsep>")]),
                                    None,
                                ),
                            );
                            path_str = ltrim(
                                &Preg::replace(&format!("{{^{}}}", target_dir), "", &ltrim(&path_str, "\\/")),
                                "\\/",
                            );
                        } else {
                            // add target-dir from file paths that don't have it
                            path_str = format!("{}/{}", package.get_target_dir().unwrap_or_default(), path_str);
                        }
                    }

                    if r#type == "exclude-from-classmap" {
                        // first escape user input
                        let p = Preg::replace("{/+}", "/", &preg_quote(&trim(&strtr(&path_str, "\\", "/"), "/"), None));

                        // add support for wildcards * and **
                        let p = strtr(&p, &{
                            let mut m: IndexMap<String, String> = IndexMap::new();
                            m.insert("\\*\\*".to_string(), ".+?".to_string());
                            m.insert("\\*".to_string(), "[^/]+?".to_string());
                            m
                        });

                        // add support for up-level relative paths
                        let mut updir: Option<String> = None;
                        let p = Preg::replace_callback(
                            "{^((?:(?:\\\\\\.){1,2}+/)+)}",
                            |matches: &Vec<String>| -> String {
                                // undo preg_quote for the matched string
                                updir = Some(str_replace("\\.", ".", &matches[1]));

                                String::new()
                            },
                            &p,
                        );
                        let install_path_for_resolve = if install_path.is_empty() {
                            strtr(&Platform::get_cwd(), "\\", "/")
                        } else {
                            install_path.clone()
                        };

                        let resolved_path = realpath(&format!("{}/{}", install_path_for_resolve, updir.clone().unwrap_or_default()));
                        let resolved_path = match resolved_path {
                            Some(rp) => rp,
                            None => continue,
                        };
                        let entry = format!(
                            "{}/{}($|/)",
                            preg_quote(&strtr(&resolved_path, "\\", "/"), None),
                            p
                        );
                        autoloads.insert(numeric_index.to_string(), Box::new(PhpMixed::String(entry)));
                        numeric_index += 1;
                        continue;
                    }

                    let relative_path = if install_path.is_empty() {
                        if path_str.is_empty() { ".".to_string() } else { path_str.clone() }
                    } else {
                        format!("{}/{}", install_path, path_str)
                    };

                    if r#type == "files" {
                        autoloads.insert(
                            self.get_file_identifier(&**package, &path_str),
                            Box::new(PhpMixed::String(relative_path)),
                        );
                        continue;
                    }
                    if r#type == "classmap" {
                        autoloads.insert(numeric_index.to_string(), Box::new(PhpMixed::String(relative_path)));
                        numeric_index += 1;
                        continue;
                    }

                    // psr-0/psr-4: append to namespace's list
                    let entry = autoloads
                        .entry(namespace.clone())
                        .or_insert_with(|| Box::new(PhpMixed::List(vec![])));
                    if let PhpMixed::List(l) = entry.as_mut() {
                        l.push(Box::new(PhpMixed::String(relative_path)));
                    }
                }
            }
        }

        autoloads
    }

    pub(crate) fn get_file_identifier(&self, package: &dyn PackageInterface, path: &str) -> String {
        // TODO composer v3 change this to sha1 or xxh3? Possibly not worth the potential breakage though
        hash("md5", &format!("{}:{}", package.get_name(), path))
    }

    /// Filters out dev-dependencies
    pub(crate) fn filter_package_map(
        &self,
        package_map: Vec<(Box<dyn PackageInterface>, Option<String>)>,
        root_package: &dyn RootPackageInterface,
    ) -> Vec<(Box<dyn PackageInterface>, Option<String>)> {
        let mut packages: IndexMap<String, Box<dyn PackageInterface>> = IndexMap::new();
        let mut include: IndexMap<String, bool> = IndexMap::new();
        let mut replaced_by: IndexMap<String, String> = IndexMap::new();

        for item in &package_map {
            let package = &item.0;
            let name = package.get_name().to_string();
            packages.insert(name.clone(), package.clone_box());
            for (_k, replace) in &package.get_replaces() {
                replaced_by.insert(replace.get_target().to_string(), name.clone());
            }
        }

        // Recursive walk emulating PHP's by-reference closure capture.
        fn add(
            package: &dyn PackageInterface,
            packages: &IndexMap<String, Box<dyn PackageInterface>>,
            include: &mut IndexMap<String, bool>,
            replaced_by: &IndexMap<String, String>,
        ) {
            for (_k, link) in &package.get_requires() {
                let mut target = link.get_target().to_string();
                if let Some(rep) = replaced_by.get(&target) {
                    target = rep.clone();
                }
                if !include.contains_key(&target) {
                    include.insert(target.clone(), true);
                    if let Some(p) = packages.get(&target) {
                        add(&**p, packages, include, replaced_by);
                    }
                }
            }
        }
        add(root_package.as_package_interface(), &packages, &mut include, &replaced_by);

        array_filter(package_map, |item: &(Box<dyn PackageInterface>, Option<String>)| -> bool {
            let package = &item.0;
            for name in package.get_names(true) {
                if include.contains_key(&name) {
                    return true;
                }
            }

            false
        })
    }

    /// Sorts packages by dependency weight
    ///
    /// Packages of equal weight are sorted alphabetically
    pub(crate) fn sort_package_map(
        &self,
        package_map: Vec<(Box<dyn PackageInterface>, Option<String>)>,
    ) -> Vec<(Box<dyn PackageInterface>, Option<String>)> {
        let mut packages: IndexMap<String, Box<dyn PackageInterface>> = IndexMap::new();
        let mut paths: IndexMap<String, Option<String>> = IndexMap::new();

        for item in &package_map {
            let (package, path) = item;
            let name = package.get_name().to_string();
            packages.insert(name.clone(), package.clone_box());
            paths.insert(name, path.clone());
        }

        let sorted_packages = PackageSorter::sort_packages(packages.values().map(|p| p.clone_box()).collect(), IndexMap::new());

        let mut sorted_package_map: Vec<(Box<dyn PackageInterface>, Option<String>)> = vec![];

        for package in sorted_packages {
            let name = package.get_name().to_string();
            sorted_package_map.push((packages.get(&name).unwrap().clone_box(), paths.get(&name).cloned().flatten()));
        }

        sorted_package_map
    }
}

pub fn composer_require(file_identifier: &str, file: &str) {
    if shirabe_php_shim::globals_get(&["__composer_autoload_files", file_identifier]).is_none()
        || !shirabe_php_shim::globals_get(&["__composer_autoload_files", file_identifier])
            .map(|v| v.as_bool().unwrap_or(false))
            .unwrap_or(false)
    {
        shirabe_php_shim::globals_set(&["__composer_autoload_files", file_identifier], PhpMixed::Bool(true));

        let _ = shirabe_php_shim::php_require(file);
    }
}

// Helper used by parse_autoloads_type for chained string substitutions.
fn str_replace_multi(input: &str, pairs: &[(&str, &str)]) -> String {
    let mut s = input.to_string();
    for (from, to) in pairs {
        s = str_replace(from, to, &s);
    }
    s
}
