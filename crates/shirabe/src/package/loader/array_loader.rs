//! ref: composer/src/Composer/Package/Loader/ArrayLoader.php

use anyhow::Result;
use chrono::{DateTime, TimeZone, Utc};
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{
    is_scalar, is_string, json_encode, ltrim, sprintf, stripos, strpos, strtolower, strval, substr,
    trigger_error, trim, ucfirst, Exception, LogicException, PhpMixed, UnexpectedValueException,
    E_USER_DEPRECATED,
};

use crate::package::base_package::{BasePackage, SUPPORTED_LINK_TYPES};
use crate::package::complete_alias_package::CompleteAliasPackage;
use crate::package::complete_package::CompletePackage;
use crate::package::complete_package_interface::CompletePackageInterface;
use crate::package::link::Link;
use crate::package::loader::loader_interface::LoaderInterface;
use crate::package::package_interface::PackageInterface;
use crate::package::root_alias_package::RootAliasPackage;
use crate::package::root_package::RootPackage;
use crate::package::version::version_parser::VersionParser;

#[derive(Debug)]
pub struct ArrayLoader {
    /// @var VersionParser
    pub(crate) version_parser: VersionParser,
    /// @var bool
    pub(crate) load_options: bool,
}

impl ArrayLoader {
    pub fn new(parser: Option<VersionParser>, load_options: bool) -> Self {
        let parser = match parser {
            Some(p) => p,
            None => {
                // TODO(phase-b): VersionParser has no public `new` yet
                todo!("VersionParser::new()")
            }
        };
        Self {
            version_parser: parser,
            load_options,
        }
    }
}

impl LoaderInterface for ArrayLoader {
    /// @inheritDoc
    fn load(
        &self,
        mut config: IndexMap<String, PhpMixed>,
        class: Option<String>,
    ) -> Result<Box<BasePackage>> {
        let class = class.unwrap_or_else(|| "Composer\\Package\\CompletePackage".to_string());

        if class != "Composer\\Package\\CompletePackage"
            && class != "Composer\\Package\\RootPackage"
        {
            trigger_error(
                "The $class arg is deprecated, please reach out to Composer maintainers ASAP if you still need this.",
                E_USER_DEPRECATED,
            );
        }

        let mut package = self.create_object(&config, &class)?;

        for (r#type, opts) in SUPPORTED_LINK_TYPES.iter() {
            let entry = config.get(*r#type);
            let entry_is_array = entry
                .map(|v| matches!(v, PhpMixed::Array(_)))
                .unwrap_or(false);
            if entry.is_none() || !entry_is_array {
                continue;
            }
            let _method = format!("set{}", ucfirst(opts.method));
            let links = self.parse_links(
                package.get_name(),
                package.get_pretty_version(),
                opts.method,
                match entry.unwrap() {
                    PhpMixed::Array(arr) => arr
                        .iter()
                        .map(|(k, v)| (k.clone(), (**v).clone()))
                        .collect(),
                    _ => IndexMap::new(),
                },
            )?;
            // TODO(phase-b): PHP `$package->{$method}($links)` — dynamic setter dispatch by name
            let _ = &mut package;
            let _ = links;
        }

        let package = self.configure_object(package, &mut config)?;

        Ok(package)
    }
}

impl ArrayLoader {
    /// @param array<array<mixed>> $versions
    ///
    /// @return list<CompletePackage|CompleteAliasPackage>
    pub fn load_packages(
        &self,
        versions: Vec<IndexMap<String, PhpMixed>>,
    ) -> Result<Vec<Box<BasePackage>>> {
        let mut packages: Vec<Box<BasePackage>> = vec![];
        let mut link_cache: IndexMap<
            String,
            IndexMap<String, IndexMap<String, IndexMap<String, (String, Link)>>>,
        > = IndexMap::new();

        for mut version in versions {
            let package = self.create_object(&version, "Composer\\Package\\CompletePackage")?;

            self.configure_cached_links(&mut link_cache, &package, &version)?;
            let package = self.configure_object(package, &mut version)?;

            packages.push(package);
        }

        Ok(packages)
    }

    /// @template PackageClass of CompletePackage
    ///
    /// @param mixed[] $config package data
    /// @param string  $class  FQCN to be instantiated
    ///
    /// @return CompletePackage|RootPackage
    ///
    /// @phpstan-param class-string<PackageClass> $class
    fn create_object(
        &self,
        config: &IndexMap<String, PhpMixed>,
        class: &str,
    ) -> Result<Box<CompletePackage>> {
        if !config.contains_key("name") {
            return Err(UnexpectedValueException {
                message: format!(
                    "Unknown package has no name defined ({}).",
                    json_encode(&PhpMixed::Array(
                        config
                            .iter()
                            .map(|(k, v)| (k.clone(), Box::new(v.clone())))
                            .collect(),
                    ))
                    .unwrap_or_default()
                ),
                code: 0,
            }
            .into());
        }
        if !config.contains_key("version") || !is_scalar(config.get("version").unwrap()) {
            return Err(UnexpectedValueException {
                message: format!(
                    "Package {} has no version defined.",
                    config.get("name").and_then(|v| v.as_string()).unwrap_or("")
                ),
                code: 0,
            }
            .into());
        }
        let mut config_version = config.get("version").cloned().unwrap_or(PhpMixed::Null);
        if !is_string(&config_version) {
            config_version = PhpMixed::String(strval(&config_version));
        }

        // handle already normalized versions
        let version: String;
        if config.contains_key("version_normalized")
            && is_string(config.get("version_normalized").unwrap())
        {
            let mut v = config
                .get("version_normalized")
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string();

            // handling of existing repos which need to remain composer v1 compatible, in case the version_normalized contained VersionParser::DEFAULT_BRANCH_ALIAS, we renormalize it
            if v == VersionParser::DEFAULT_BRANCH_ALIAS {
                v = self
                    .version_parser
                    .normalize(config_version.as_string().unwrap_or(""), None)?;
            }
            version = v;
        } else {
            match self
                .version_parser
                .normalize(config_version.as_string().unwrap_or(""), None)
            {
                Ok(v) => version = v,
                Err(e) => {
                    // TODO(phase-b): preserve original exception chain via anyhow::Error::context
                    return Err(UnexpectedValueException {
                        message: format!(
                            "Failed to normalize version for package \"{}\": {}",
                            config.get("name").and_then(|v| v.as_string()).unwrap_or(""),
                            e
                        ),
                        code: 0,
                    }
                    .into());
                }
            }
        }

        // PHP: return new $class($config['name'], $version, $config['version']);
        // TODO(phase-b): dispatch class-string $class to CompletePackage / RootPackage
        // constructor; for now we only support CompletePackage
        let _ = class;
        let _name = config.get("name").and_then(|v| v.as_string()).unwrap_or("");
        let _pretty_version = config_version.as_string().unwrap_or("").to_string();
        let _ = version;
        todo!("phase-b: dynamic class-string instantiation new $class($name, $version, $prettyVersion)")
    }

    /// @param CompletePackage $package
    /// @param mixed[]         $config package data
    ///
    /// @return RootPackage|RootAliasPackage|CompletePackage|CompleteAliasPackage
    fn configure_object(
        &self,
        mut package: Box<CompletePackage>,
        config: &mut IndexMap<String, PhpMixed>,
    ) -> Result<Box<BasePackage>> {
        // PHP: if (!$package instanceof CompletePackage) — true by construction in Rust
        // (create_object always returns Box<CompletePackage>); kept as a no-op for parity.
        let _ = LogicException {
            message: "ArrayLoader expects instances of the Composer\\Package\\CompletePackage class to function correctly".to_string(),
            code: 0,
        };

        // PHP: $package->setType(isset($config['type']) ? strtolower($config['type']) : 'library');
        // TODO(phase-b): set_type on CompletePackage/Package
        let _type_value = if let Some(t) = config.get("type") {
            strtolower(t.as_string().unwrap_or(""))
        } else {
            "library".to_string()
        };

        if let Some(target_dir) = config.get("target-dir") {
            // TODO(phase-b): package.set_target_dir
            let _ = target_dir;
        }

        if let Some(extra) = config.get("extra") {
            if matches!(extra, PhpMixed::Array(_)) {
                // TODO(phase-b): package.set_extra
                let _ = extra;
            }
        }

        if let Some(bin) = config.get("bin").cloned() {
            let mut bin_list = match bin {
                PhpMixed::Array(_) | PhpMixed::List(_) => bin,
                other => PhpMixed::List(vec![Box::new(other)]),
            };
            // foreach ($config['bin'] as $key => $bin) { $config['bin'][$key] = ltrim($bin, '/'); }
            if let PhpMixed::List(ref mut list) = bin_list {
                for item in list.iter_mut() {
                    if let Some(s) = item.as_string() {
                        *item = Box::new(PhpMixed::String(ltrim(s, Some("/"))));
                    }
                }
            } else if let PhpMixed::Array(ref mut map) = bin_list {
                for (_k, v) in map.iter_mut() {
                    if let Some(s) = v.as_string() {
                        *v = Box::new(PhpMixed::String(ltrim(s, Some("/"))));
                    }
                }
            }
            config.insert("bin".to_string(), bin_list);
            // TODO(phase-b): package.set_binaries
        }

        if let Some(installation_source) = config.get("installation-source") {
            // TODO(phase-b): package.set_installation_source
            let _ = installation_source;
        }

        if let Some(default_branch) = config.get("default-branch") {
            if default_branch.as_bool() == Some(true) {
                // TODO(phase-b): package.set_is_default_branch(true)
            }
        }

        if let Some(source) = config.get("source").cloned() {
            let source_map = match &source {
                PhpMixed::Array(m) => Some(m.clone()),
                _ => None,
            };
            let has_required = source_map
                .as_ref()
                .map(|m| {
                    m.contains_key("type") && m.contains_key("url") && m.contains_key("reference")
                })
                .unwrap_or(false);
            if !has_required {
                return Err(UnexpectedValueException {
                    message: sprintf(
                        "Package %s's source key should be specified as {\"type\": ..., \"url\": ..., \"reference\": ...},\n%s given.",
                        &[
                            PhpMixed::String(
                                config
                                    .get("name")
                                    .and_then(|v| v.as_string())
                                    .unwrap_or("")
                                    .to_string(),
                            ),
                            PhpMixed::String(json_encode(&source).unwrap_or_default()),
                        ],
                    ),
                    code: 0,
                }
                .into());
            }
            let source_map = source_map.unwrap();
            // TODO(phase-b): package.set_source_type/_url/_reference/_mirrors
            let _ = source_map.get("type");
            let _ = source_map.get("url");
            let _reference = source_map.get("reference").map(|v| strval(v));
            let _ = _reference;
            if let Some(mirrors) = source_map.get("mirrors") {
                let _ = mirrors;
            }
        }

        if let Some(dist) = config.get("dist").cloned() {
            let dist_map = match &dist {
                PhpMixed::Array(m) => Some(m.clone()),
                _ => None,
            };
            let has_required = dist_map
                .as_ref()
                .map(|m| m.contains_key("type") && m.contains_key("url"))
                .unwrap_or(false);
            if !has_required {
                return Err(UnexpectedValueException {
                    message: sprintf(
                        "Package %s's dist key should be specified as {\"type\": ..., \"url\": ..., \"reference\": ..., \"shasum\": ...},\n%s given.",
                        &[
                            PhpMixed::String(
                                config
                                    .get("name")
                                    .and_then(|v| v.as_string())
                                    .unwrap_or("")
                                    .to_string(),
                            ),
                            PhpMixed::String(json_encode(&dist).unwrap_or_default()),
                        ],
                    ),
                    code: 0,
                }
                .into());
            }
            let dist_map = dist_map.unwrap();
            // TODO(phase-b): package.set_dist_type/_url/_reference/_sha1_checksum/_mirrors
            let _ = dist_map.get("type");
            let _ = dist_map.get("url");
            let _reference = dist_map.get("reference").map(|v| strval(v));
            let _ = _reference;
            let _shasum = dist_map.get("shasum");
            let _ = _shasum;
            if let Some(mirrors) = dist_map.get("mirrors") {
                let _ = mirrors;
            }
        }

        if let Some(suggest) = config.get("suggest").cloned() {
            if let PhpMixed::Array(mut suggest_map) = suggest {
                for (target, reason) in suggest_map.iter_mut() {
                    if let Some(r) = reason.as_string() {
                        if trim(r, None) == "self.version" {
                            *reason = Box::new(PhpMixed::String(
                                package.get_pretty_version().to_string(),
                            ));
                            let _ = target;
                        }
                    }
                }
                config.insert("suggest".to_string(), PhpMixed::Array(suggest_map));
                // TODO(phase-b): package.set_suggests
            }
        }

        if let Some(autoload) = config.get("autoload") {
            // TODO(phase-b): package.set_autoload
            let _ = autoload;
        }

        if let Some(autoload_dev) = config.get("autoload-dev") {
            // TODO(phase-b): package.set_dev_autoload
            let _ = autoload_dev;
        }

        if let Some(include_path) = config.get("include-path") {
            // TODO(phase-b): package.set_include_paths
            let _ = include_path;
        }

        if let Some(php_ext) = config.get("php-ext") {
            // TODO(phase-b): package.set_php_ext
            let _ = php_ext;
        }

        if let Some(time_value) = config.get("time") {
            if !shirabe_php_shim::empty(time_value) {
                let time_str = time_value.as_string().unwrap_or("");
                let time = if Preg::is_match(r"/^\d++$/D", time_str) {
                    format!("@{}", time_str)
                } else {
                    time_str.to_string()
                };

                let result: std::result::Result<DateTime<Utc>, Exception> =
                    // TODO(phase-b): port PHP `new \DateTime($time, new \DateTimeZone('UTC'))`
                    Utc.datetime_from_str(&time, "%Y-%m-%dT%H:%M:%S%z")
                        .map_err(|e| Exception {
                            message: e.to_string(),
                            code: 0,
                        });
                if let Ok(date) = result {
                    // TODO(phase-b): package.set_release_date(date)
                    let _ = date;
                }
            }
        }

        if let Some(notification_url) = config.get("notification-url") {
            if !shirabe_php_shim::empty(notification_url) {
                // TODO(phase-b): package.set_notification_url
                let _ = notification_url;
            }
        }

        // PHP: $package instanceof CompletePackageInterface — true since $package is CompletePackage
        {
            if let Some(archive) = config.get("archive").cloned() {
                if let PhpMixed::Array(archive_map) = archive {
                    if let Some(name) = archive_map.get("name") {
                        if !shirabe_php_shim::empty(name) {
                            // TODO(phase-b): package.set_archive_name
                            let _ = name;
                        }
                    }
                    if let Some(exclude) = archive_map.get("exclude") {
                        if !shirabe_php_shim::empty(exclude) {
                            // TODO(phase-b): package.set_archive_excludes
                            let _ = exclude;
                        }
                    }
                }
            }

            if let Some(scripts) = config.get("scripts").cloned() {
                if let PhpMixed::Array(mut scripts_map) = scripts {
                    for (event, listeners) in scripts_map.iter_mut() {
                        let listeners_array = match listeners.as_ref() {
                            PhpMixed::Array(_) | PhpMixed::List(_) => listeners.clone(),
                            other => Box::new(PhpMixed::List(vec![Box::new(other.clone())])),
                        };
                        *listeners = listeners_array;
                        let _ = event;
                    }
                    for reserved in ["composer", "php", "putenv"].iter() {
                        if scripts_map.contains_key(*reserved) {
                            trigger_error(
                                &format!(
                                    "The `{}` script name is reserved for internal use, please avoid defining it",
                                    reserved
                                ),
                                E_USER_DEPRECATED,
                            );
                        }
                    }
                    config.insert("scripts".to_string(), PhpMixed::Array(scripts_map));
                    // TODO(phase-b): package.set_scripts
                }
            }

            if let Some(description) = config.get("description") {
                if !shirabe_php_shim::empty(description) && is_string(description) {
                    package.set_description(
                        description.as_string().unwrap_or("").to_string(),
                    );
                }
            }

            if let Some(homepage) = config.get("homepage") {
                if !shirabe_php_shim::empty(homepage) && is_string(homepage) {
                    package.set_homepage(homepage.as_string().unwrap_or("").to_string());
                }
            }

            if let Some(keywords) = config.get("keywords") {
                if !shirabe_php_shim::empty(keywords) {
                    if matches!(keywords, PhpMixed::Array(_) | PhpMixed::List(_)) {
                        // PHP: array_map('strval', $config['keywords'])
                        let keywords_vec: Vec<String> = match keywords {
                            PhpMixed::List(list) => list.iter().map(|v| strval(v)).collect(),
                            PhpMixed::Array(map) => map.values().map(|v| strval(v)).collect(),
                            _ => vec![],
                        };
                        package.set_keywords(keywords_vec);
                    }
                }
            }

            if let Some(license) = config.get("license") {
                if !shirabe_php_shim::empty(license) {
                    let license_vec: Vec<String> = match license {
                        PhpMixed::Array(map) => map
                            .values()
                            .map(|v| v.as_string().unwrap_or("").to_string())
                            .collect(),
                        PhpMixed::List(list) => list
                            .iter()
                            .map(|v| v.as_string().unwrap_or("").to_string())
                            .collect(),
                        other => vec![other.as_string().unwrap_or("").to_string()],
                    };
                    package.set_license(license_vec);
                }
            }

            if let Some(authors) = config.get("authors") {
                if !shirabe_php_shim::empty(authors) {
                    if let PhpMixed::List(list) = authors {
                        let authors_vec: Vec<IndexMap<String, String>> = list
                            .iter()
                            .filter_map(|v| match v.as_ref() {
                                PhpMixed::Array(m) => Some(
                                    m.iter()
                                        .map(|(k, v)| {
                                            (k.clone(), v.as_string().unwrap_or("").to_string())
                                        })
                                        .collect(),
                                ),
                                _ => None,
                            })
                            .collect();
                        package.set_authors(authors_vec);
                    }
                }
            }

            if let Some(support) = config.get("support") {
                if let PhpMixed::Array(map) = support {
                    let support_map: IndexMap<String, String> = map
                        .iter()
                        .map(|(k, v)| (k.clone(), v.as_string().unwrap_or("").to_string()))
                        .collect();
                    package.set_support(support_map);
                }
            }

            if let Some(funding) = config.get("funding") {
                if !shirabe_php_shim::empty(funding) {
                    if let PhpMixed::List(list) = funding {
                        let funding_vec: Vec<IndexMap<String, PhpMixed>> = list
                            .iter()
                            .filter_map(|v| match v.as_ref() {
                                PhpMixed::Array(m) => Some(
                                    m.iter().map(|(k, v)| (k.clone(), (**v).clone())).collect(),
                                ),
                                _ => None,
                            })
                            .collect();
                        package.set_funding(funding_vec);
                    }
                }
            }

            if let Some(abandoned) = config.get("abandoned") {
                package.set_abandoned(abandoned.clone());
            }
        }

        if self.load_options {
            if let Some(transport_options) = config.get("transport-options") {
                // TODO(phase-b): package.set_transport_options
                let _ = transport_options;
            }
        }

        let alias_normalized = self.get_branch_alias(config)?;
        if let Some(alias_normalized) = alias_normalized {
            if !alias_normalized.is_empty() {
                let pretty_alias = Preg::replace(r"{(\.9{7})+}", ".x", &alias_normalized);

                // TODO(phase-b): `$package instanceof RootPackage` downcast from CompletePackage
                let package_as_root: Option<RootPackage> = None;
                if let Some(root) = package_as_root {
                    let _ = RootAliasPackage::new(root, alias_normalized, pretty_alias);
                    // TODO(phase-b): return Box<RootAliasPackage> wrapped as Box<BasePackage>
                    todo!("phase-b: return RootAliasPackage as Box<BasePackage>")
                }

                let _ = CompleteAliasPackage::new(*package, alias_normalized, pretty_alias);
                // TODO(phase-b): return Box<CompleteAliasPackage> wrapped as Box<BasePackage>
                todo!("phase-b: return CompleteAliasPackage as Box<BasePackage>")
            }
        }

        // TODO(phase-b): coerce Box<CompletePackage> -> Box<BasePackage>
        let _ = package;
        todo!("phase-b: return Box<CompletePackage> as Box<BasePackage>")
    }

    /// @param array<string, array<string, array<int|string, array<int|string, array{string, Link}>>>> $linkCache
    /// @param mixed[]                                                                             $config
    fn configure_cached_links(
        &self,
        link_cache: &mut IndexMap<
            String,
            IndexMap<String, IndexMap<String, IndexMap<String, (String, Link)>>>,
        >,
        package: &Box<CompletePackage>,
        config: &IndexMap<String, PhpMixed>,
    ) -> Result<()> {
        let name = package.get_name().to_string();
        let pretty_version = package.get_pretty_version().to_string();

        for (r#type, opts) in SUPPORTED_LINK_TYPES.iter() {
            if let Some(entry) = config.get(*r#type) {
                let _method = format!("set{}", ucfirst(opts.method));

                let mut links: IndexMap<String, Link> = IndexMap::new();
                let entries: IndexMap<String, PhpMixed> = match entry {
                    PhpMixed::Array(m) => m
                        .iter()
                        .map(|(k, v)| (k.clone(), (**v).clone()))
                        .collect(),
                    _ => continue,
                };
                for (pretty_target, constraint) in entries {
                    let target = strtolower(&pretty_target);

                    // recursive links are not supported
                    if target == name {
                        continue;
                    }

                    let constraint_str = constraint.as_string().unwrap_or("").to_string();
                    if constraint_str == "self.version" {
                        let link = self.create_link(
                            &name,
                            &pretty_version,
                            opts.method,
                            &target,
                            &constraint_str,
                        )?;
                        links.insert(target, link);
                    } else {
                        let cached = link_cache
                            .get(&name)
                            .and_then(|m| m.get(*r#type))
                            .and_then(|m| m.get(&target))
                            .and_then(|m| m.get(&constraint_str))
                            .cloned();
                        let (target, link) = if let Some(cached) = cached {
                            cached
                        } else {
                            let link = self.create_link(
                                &name,
                                &pretty_version,
                                opts.method,
                                &target,
                                &constraint_str,
                            )?;
                            let entry = (target.clone(), link);
                            link_cache
                                .entry(name.clone())
                                .or_insert_with(IndexMap::new)
                                .entry(r#type.to_string())
                                .or_insert_with(IndexMap::new)
                                .entry(target.clone())
                                .or_insert_with(IndexMap::new)
                                .insert(constraint_str.clone(), entry.clone());
                            entry
                        };
                        links.insert(target, link);
                    }
                }

                // TODO(phase-b): PHP `$package->{$method}($links)` — dynamic setter dispatch by name
                let _ = links;
            }
        }

        Ok(())
    }

    /// @param  string                    $source        source package name
    /// @param  string                    $sourceVersion source package version (pretty version ideally)
    /// @param  string                    $description   link description (e.g. requires, replaces, ..)
    /// @param  array<string|int, string> $links         array of package name => constraint mappings
    ///
    /// @return Link[]
    ///
    /// @phpstan-param Link::TYPE_* $description
    pub fn parse_links(
        &self,
        source: &str,
        source_version: &str,
        description: &str,
        links: IndexMap<String, PhpMixed>,
    ) -> Result<IndexMap<String, Link>> {
        let mut res: IndexMap<String, Link> = IndexMap::new();
        for (target, constraint) in links {
            if !is_string(&constraint) {
                continue;
            }
            let target = strtolower(&target);
            let link = self.create_link(
                source,
                source_version,
                description,
                &target,
                constraint.as_string().unwrap_or(""),
            )?;
            res.insert(target, link);
        }

        Ok(res)
    }

    /// @param  string       $source           source package name
    /// @param  string       $sourceVersion    source package version (pretty version ideally)
    /// @param  Link::TYPE_* $description      link description (e.g. requires, replaces, ..)
    /// @param  string       $target           target package name
    /// @param  string       $prettyConstraint constraint string
    fn create_link(
        &self,
        source: &str,
        source_version: &str,
        description: &str,
        target: &str,
        pretty_constraint: &str,
    ) -> Result<Link> {
        // PHP: if (!\is_string($prettyConstraint)) — always true in Rust signature, kept for parity
        let _ = pretty_constraint;

        let constraint = if pretty_constraint == "self.version" {
            source_version.to_string()
        } else {
            pretty_constraint.to_string()
        };

        let parsed_constraint = match self.version_parser.parse_constraints(&constraint) {
            Ok(c) => c,
            Err(e) => {
                // TODO(phase-b): preserve original exception chain
                let _ = &e;
                return Err(UnexpectedValueException {
                    message: format!(
                        "Link constraint in {} {} > {} should be a valid version constraint, got \"{}\"",
                        source, description, target, constraint
                    ),
                    code: 0,
                }
                .into());
            }
        };

        // TODO(phase-b): Link::new expects Box<dyn ConstraintInterface>; we have Arc<dyn ConstraintInterface + Send + Sync>
        let _ = parsed_constraint;
        Ok(Link::new(
            source.to_string(),
            target.to_string(),
            todo!("phase-b: convert Arc<dyn ConstraintInterface> to Box<dyn ConstraintInterface>"),
            Some(description.to_string()),
            Some(pretty_constraint.to_string()),
        ))
    }

    /// Retrieves a branch alias (dev-master => 1.0.x-dev for example) if it exists
    ///
    /// @param mixed[] $config the entire package config
    ///
    /// @return string|null normalized version of the branch alias or null if there is none
    pub fn get_branch_alias(
        &self,
        config: &IndexMap<String, PhpMixed>,
    ) -> Result<Option<String>> {
        if !config.contains_key("version") || !is_scalar(config.get("version").unwrap()) {
            return Err(UnexpectedValueException {
                message: "no/invalid version defined".to_string(),
                code: 0,
            }
            .into());
        }
        let mut config_version = config.get("version").cloned().unwrap_or(PhpMixed::Null);
        if !is_string(&config_version) {
            config_version = PhpMixed::String(strval(&config_version));
        }

        let version_str = config_version.as_string().unwrap_or("").to_string();
        if strpos(&version_str, "dev-") != Some(0) && "-dev" != substr(&version_str, -4, None) {
            return Ok(None);
        }

        let extra_branch_alias = config
            .get("extra")
            .and_then(|v| match v {
                PhpMixed::Array(m) => m.get("branch-alias").cloned(),
                _ => None,
            })
            .and_then(|v| match v.as_ref() {
                PhpMixed::Array(m) => Some(m.clone()),
                _ => None,
            });

        if let Some(branch_alias_map) = extra_branch_alias {
            for (source_branch, target_branch_value) in branch_alias_map {
                let source_branch = strval(&PhpMixed::String(source_branch));
                let target_branch = target_branch_value.as_string().unwrap_or("").to_string();

                // ensure it is an alias to a -dev package
                if "-dev" != substr(&target_branch, -4, None) {
                    continue;
                }

                // normalize without -dev and ensure it's a numeric branch that is parseable
                let validated_target_branch = if target_branch == VersionParser::DEFAULT_BRANCH_ALIAS
                {
                    VersionParser::DEFAULT_BRANCH_ALIAS.to_string()
                } else {
                    self.version_parser
                        .normalize_branch(&substr(&target_branch, 0, Some(-4)))?
                };
                if "-dev" != substr(&validated_target_branch, -4, None) {
                    continue;
                }

                // ensure that it is the current branch aliasing itself
                if strtolower(&version_str) != strtolower(&source_branch) {
                    continue;
                }

                // If using numeric aliases ensure the alias is a valid subversion
                let source_prefix =
                    self.version_parser.parse_numeric_alias_prefix(&source_branch);
                let target_prefix = self.version_parser.parse_numeric_alias_prefix(&target_branch);
                if let (Some(sp), Some(tp)) = (source_prefix.as_ref(), target_prefix.as_ref()) {
                    if stripos(tp, sp) != Some(0) {
                        continue;
                    }
                }

                return Ok(Some(validated_target_branch));
            }
        }

        let default_branch_is_true =
            config.get("default-branch").and_then(|v| v.as_bool()) == Some(true);
        if config.contains_key("default-branch")
            && default_branch_is_true
            && self
                .version_parser
                .parse_numeric_alias_prefix(&Preg::replace(r"{^v}", "", &version_str))
                .is_none()
        {
            return Ok(Some(VersionParser::DEFAULT_BRANCH_ALIAS.to_string()));
        }

        Ok(None)
    }
}
