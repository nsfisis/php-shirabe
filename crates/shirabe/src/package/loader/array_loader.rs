//! ref: composer/src/Composer/Package/Loader/ArrayLoader.php

use anyhow::Result;
use chrono::Utc;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_php_shim::{
    E_USER_DEPRECATED, PhpMixed, UnexpectedValueException, is_scalar, is_string, json_encode,
    ltrim, sprintf, stripos, strpos, strtolower, strval, substr, trigger_error, trim,
};

use crate::package::CompleteAliasPackageHandle;
use crate::package::CompletePackage;
use crate::package::CompletePackageHandle;
use crate::package::CompletePackageInterface;
use crate::package::Link;
use crate::package::Mirror;
use crate::package::Package;
use crate::package::PackageInterface;
use crate::package::PackageInterfaceHandle;
use crate::package::RootAliasPackageHandle;
use crate::package::RootPackage;
use crate::package::RootPackageHandle;
use crate::package::loader::LoaderInterface;
use crate::package::version::VersionParser;
use crate::package::{BasePackage, SUPPORTED_LINK_TYPES};

#[derive(Debug)]
pub struct ArrayLoader {
    /// @var VersionParser
    pub(crate) version_parser: VersionParser,
    /// @var bool
    pub(crate) load_options: bool,
}

impl ArrayLoader {
    pub fn new(parser: Option<VersionParser>, load_options: bool) -> Self {
        let parser = parser.unwrap_or_default();
        Self {
            version_parser: parser,
            load_options,
        }
    }
}

enum CompleteOrRootPackage {
    Complete(CompletePackage),
    Root(RootPackage),
}

impl CompleteOrRootPackage {
    fn package(&self) -> &Package {
        match self {
            Self::Complete(p) => &p.inner,
            Self::Root(p) => &p.inner.inner,
        }
    }

    fn package_mut(&mut self) -> &mut Package {
        match self {
            Self::Complete(p) => &mut p.inner,
            Self::Root(p) => &mut p.inner.inner,
        }
    }

    fn complete_mut(&mut self) -> &mut dyn CompletePackageInterface {
        match self {
            Self::Complete(p) => p,
            Self::Root(p) => p,
        }
    }

    fn is_root(&self) -> bool {
        matches!(self, Self::Root(_))
    }

    fn get_name(&self) -> &str {
        self.package().get_name()
    }

    fn get_pretty_version(&self) -> &str {
        self.package().get_pretty_version()
    }

    fn into_handle(self) -> PackageInterfaceHandle {
        match self {
            Self::Complete(p) => CompletePackageHandle::from_complete_package(p).into(),
            Self::Root(p) => RootPackageHandle::from_root_package(p).into(),
        }
    }
}

fn php_to_map(value: &PhpMixed) -> IndexMap<String, PhpMixed> {
    match value {
        PhpMixed::Array(m) => m.clone(),
        _ => IndexMap::new(),
    }
}

fn php_to_string_vec(value: &PhpMixed) -> Vec<String> {
    match value {
        PhpMixed::List(l) => l.iter().map(strval).collect(),
        PhpMixed::Array(m) => m.values().map(|v| strval(v)).collect(),
        _ => Vec::new(),
    }
}

fn apply_link_setter(package: &mut Package, method: &str, links: IndexMap<String, Link>) {
    if method == Link::TYPE_REQUIRE {
        package.set_requires(links);
    } else if method == Link::TYPE_DEV_REQUIRE {
        package.set_dev_requires(links);
    } else if method == Link::TYPE_CONFLICT {
        package.set_conflicts(links);
    } else if method == Link::TYPE_PROVIDE {
        package.set_provides(links);
    } else if method == Link::TYPE_REPLACE {
        package.set_replaces(links);
    }
}

fn php_to_mirrors(value: &PhpMixed) -> Vec<Mirror> {
    let entries: Vec<&PhpMixed> = match value {
        PhpMixed::List(l) => l.iter().collect(),
        PhpMixed::Array(m) => m.values().collect(),
        _ => Vec::new(),
    };
    entries
        .into_iter()
        .filter_map(|entry| match entry {
            PhpMixed::Array(m) => Some(Mirror {
                url: m
                    .get("url")
                    .and_then(|v| v.as_string())
                    .unwrap_or("")
                    .to_string(),
                preferred: m
                    .get("preferred")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            }),
            _ => None,
        })
        .collect()
}

impl LoaderInterface for ArrayLoader {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn load(
        &self,
        mut config: IndexMap<String, PhpMixed>,
        class: Option<String>,
    ) -> Result<PackageInterfaceHandle> {
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
            let links = self.parse_links(
                package.get_name(),
                package.get_pretty_version(),
                opts.method,
                match entry.unwrap() {
                    PhpMixed::Array(arr) => arr.clone(),
                    _ => IndexMap::new(),
                },
            )?;
            apply_link_setter(package.package_mut(), opts.method, links);
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
    ) -> Result<Vec<PackageInterfaceHandle>> {
        let mut packages: Vec<PackageInterfaceHandle> = vec![];
        let mut link_cache: IndexMap<
            String,
            IndexMap<String, IndexMap<String, IndexMap<String, (String, Link)>>>,
        > = IndexMap::new();

        for mut version in versions {
            let mut package = self.create_object(&version, "Composer\\Package\\CompletePackage")?;

            self.configure_cached_links(&mut link_cache, &mut package, &version)?;
            let package = self.configure_object(package, &mut version)?;

            packages.push(package);
        }

        Ok(packages)
    }

    fn create_object(
        &self,
        config: &IndexMap<String, PhpMixed>,
        class: &str,
    ) -> Result<CompleteOrRootPackage> {
        if !config.contains_key("name") {
            return Err(UnexpectedValueException {
                message: format!(
                    "Unknown package has no name defined ({}).",
                    json_encode(&PhpMixed::Array(config.clone())).unwrap_or_default()
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

        let name = config
            .get("name")
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .to_string();
        let pretty_version = config_version.as_string().unwrap_or("").to_string();

        if class == "Composer\\Package\\RootPackage" {
            Ok(CompleteOrRootPackage::Root(RootPackage::new(
                name,
                version,
                pretty_version,
            )))
        } else {
            Ok(CompleteOrRootPackage::Complete(CompletePackage::new(
                name,
                version,
                pretty_version,
            )))
        }
    }

    fn configure_object(
        &self,
        mut package: CompleteOrRootPackage,
        config: &mut IndexMap<String, PhpMixed>,
    ) -> Result<PackageInterfaceHandle> {
        package
            .package_mut()
            .set_type(if let Some(t) = config.get("type") {
                strtolower(t.as_string().unwrap_or(""))
            } else {
                "library".to_string()
            });

        if let Some(target_dir) = config.get("target-dir") {
            package
                .package_mut()
                .set_target_dir(target_dir.as_string().map(|s| s.to_string()));
        }

        if let Some(extra) = config.get("extra")
            && matches!(extra, PhpMixed::Array(_))
        {
            let extra_map = php_to_map(extra);
            package.package_mut().set_extra(extra_map);
        }

        if let Some(bin) = config.get("bin").cloned() {
            let mut bin_list = match bin {
                PhpMixed::Array(_) | PhpMixed::List(_) => bin,
                other => PhpMixed::List(vec![other]),
            };
            if let PhpMixed::List(ref mut list) = bin_list {
                for item in list.iter_mut() {
                    if let Some(s) = item.as_string() {
                        *item = PhpMixed::String(ltrim(s, Some("/")));
                    }
                }
            } else if let PhpMixed::Array(ref mut map) = bin_list {
                for (_k, v) in map.iter_mut() {
                    if let Some(s) = v.as_string() {
                        *v = PhpMixed::String(ltrim(s, Some("/")));
                    }
                }
            }
            let binaries = php_to_string_vec(&bin_list);
            config.insert("bin".to_string(), bin_list);
            package.package_mut().set_binaries(binaries);
        }

        if let Some(installation_source) = config.get("installation-source") {
            package
                .package_mut()
                .set_installation_source(installation_source.as_string().map(|s| s.to_string()));
        }

        if let Some(default_branch) = config.get("default-branch")
            && default_branch.as_bool() == Some(true)
        {
            package.package_mut().set_is_default_branch(true);
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
                    message: format!(
                        "Package {}'s source key should be specified as {{\"type\": ..., \"url\": ..., \"reference\": ...}},\n{} given.",

                            config
                                .get("name")
                                .and_then(|v| v.as_string())
                                .unwrap_or("")
                                .to_string(),
                        json_encode(&source).unwrap_or_default(),
                    ),
                    code: 0,
                }
                .into());
            }
            let source_map = source_map.unwrap();
            package
                .package_mut()
                .set_source_type(source_map.get("type").map(|v| strval(v)));
            package
                .package_mut()
                .set_source_url(source_map.get("url").map(|v| strval(v)));
            package
                .package_mut()
                .set_source_reference(source_map.get("reference").map(|v| strval(v)));
            if let Some(mirrors) = source_map.get("mirrors") {
                package
                    .package_mut()
                    .set_source_mirrors(Some(php_to_mirrors(mirrors)));
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
                    message: format!(
                        "Package {}'s dist key should be specified as {{\"type\": ..., \"url\": ..., \"reference\": ..., \"shasum\": ...}},\n{} given.",

                            config
                                .get("name")
                                .and_then(|v| v.as_string())
                                .unwrap_or("")
                                .to_string(),
                        json_encode(&dist).unwrap_or_default(),
                    ),
                    code: 0,
                }
                .into());
            }
            let dist_map = dist_map.unwrap();
            package
                .package_mut()
                .set_dist_type(dist_map.get("type").map(|v| strval(v)));
            package
                .package_mut()
                .set_dist_url(dist_map.get("url").map(|v| strval(v)));
            package
                .package_mut()
                .set_dist_reference(dist_map.get("reference").map(|v| strval(v)));
            package
                .package_mut()
                .set_dist_sha1_checksum(dist_map.get("shasum").map(|v| strval(v)));
            if let Some(mirrors) = dist_map.get("mirrors") {
                package
                    .package_mut()
                    .set_dist_mirrors(Some(php_to_mirrors(mirrors)));
            }
        }

        if let Some(suggest) = config.get("suggest").cloned()
            && let PhpMixed::Array(mut suggest_map) = suggest
        {
            for (target, reason) in suggest_map.iter_mut() {
                if let Some(r) = reason.as_string()
                    && trim(r, None) == "self.version"
                {
                    *reason = PhpMixed::String(package.get_pretty_version().to_string());
                    let _ = target;
                }
            }
            let suggests: IndexMap<String, String> = suggest_map
                .iter()
                .map(|(k, v)| (k.clone(), strval(v)))
                .collect();
            config.insert("suggest".to_string(), PhpMixed::Array(suggest_map));
            package.package_mut().set_suggests(suggests);
        }

        if let Some(autoload) = config.get("autoload") {
            let autoload_map = php_to_map(autoload);
            package.package_mut().set_autoload(autoload_map);
        }

        if let Some(autoload_dev) = config.get("autoload-dev") {
            let dev_autoload_map = php_to_map(autoload_dev);
            package.package_mut().set_dev_autoload(dev_autoload_map);
        }

        if let Some(include_path) = config.get("include-path") {
            let include_paths = php_to_string_vec(include_path);
            package.package_mut().set_include_paths(include_paths);
        }

        if let Some(php_ext) = config.get("php-ext") {
            let php_ext_map = php_to_map(php_ext);
            package.package_mut().set_php_ext(Some(php_ext_map));
        }

        if let Some(time_value) = config.get("time")
            && !shirabe_php_shim::empty(time_value)
        {
            let time_str = time_value.as_string().unwrap_or("");
            let time = if Preg::is_match(r"/^\d++$/D", time_str) {
                format!("@{}", time_str)
            } else {
                time_str.to_string()
            };

            if let Ok(date) = shirabe_php_shim::date_create::<Utc>(&time) {
                package.package_mut().set_release_date(Some(date));
            }
        }

        if let Some(notification_url) = config.get("notification-url")
            && !shirabe_php_shim::empty(notification_url)
        {
            package
                .package_mut()
                .set_notification_url(strval(notification_url));
        }

        if let Some(archive) = config.get("archive").cloned()
            && let PhpMixed::Array(archive_map) = archive
        {
            if let Some(name) = archive_map.get("name")
                && !shirabe_php_shim::empty(name)
            {
                package.complete_mut().set_archive_name(strval(name));
            }
            if let Some(exclude) = archive_map.get("exclude")
                && !shirabe_php_shim::empty(exclude)
            {
                package
                    .complete_mut()
                    .set_archive_excludes(php_to_string_vec(exclude));
            }
        }

        if let Some(scripts) = config.get("scripts").cloned()
            && let PhpMixed::Array(mut scripts_map) = scripts
        {
            for (event, listeners) in scripts_map.iter_mut() {
                let listeners_array = match &*listeners {
                    PhpMixed::Array(_) | PhpMixed::List(_) => listeners.clone(),
                    other => PhpMixed::List(vec![other.clone()]),
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
            let scripts: IndexMap<String, Vec<String>> = scripts_map
                .iter()
                .map(|(k, v)| (k.clone(), php_to_string_vec(v)))
                .collect();
            config.insert("scripts".to_string(), PhpMixed::Array(scripts_map));
            package.complete_mut().set_scripts(scripts);
        }

        if let Some(description) = config.get("description")
            && !shirabe_php_shim::empty(description)
            && is_string(description)
        {
            package
                .complete_mut()
                .set_description(description.as_string().unwrap_or("").to_string());
        }

        if let Some(homepage) = config.get("homepage")
            && !shirabe_php_shim::empty(homepage)
            && is_string(homepage)
        {
            package
                .complete_mut()
                .set_homepage(homepage.as_string().unwrap_or("").to_string());
        }

        if let Some(keywords) = config.get("keywords")
            && !shirabe_php_shim::empty(keywords)
            && matches!(keywords, PhpMixed::Array(_) | PhpMixed::List(_))
        {
            let keywords_vec: Vec<String> = match keywords {
                PhpMixed::List(list) => list.iter().map(strval).collect(),
                PhpMixed::Array(map) => map.values().map(|v| strval(v)).collect(),
                _ => vec![],
            };
            package.complete_mut().set_keywords(keywords_vec);
        }

        if let Some(license) = config.get("license")
            && !shirabe_php_shim::empty(license)
        {
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
            package.complete_mut().set_license(license_vec);
        }

        if let Some(authors) = config.get("authors")
            && !shirabe_php_shim::empty(authors)
            && let PhpMixed::List(list) = authors
        {
            let authors_vec: Vec<IndexMap<String, String>> = list
                .iter()
                .filter_map(|v| match v {
                    PhpMixed::Array(m) => Some(
                        m.iter()
                            .map(|(k, v)| (k.clone(), v.as_string().unwrap_or("").to_string()))
                            .collect(),
                    ),
                    _ => None,
                })
                .collect();
            package.complete_mut().set_authors(authors_vec);
        }

        if let Some(support) = config.get("support")
            && let PhpMixed::Array(map) = support
        {
            let support_map: IndexMap<String, String> = map
                .iter()
                .map(|(k, v)| (k.clone(), v.as_string().unwrap_or("").to_string()))
                .collect();
            package.complete_mut().set_support(support_map);
        }

        if let Some(funding) = config.get("funding")
            && !shirabe_php_shim::empty(funding)
            && let PhpMixed::List(list) = funding
        {
            let funding_vec: Vec<IndexMap<String, PhpMixed>> = list
                .iter()
                .filter_map(|v| match v {
                    PhpMixed::Array(m) => Some(m.clone()),
                    _ => None,
                })
                .collect();
            package.complete_mut().set_funding(funding_vec);
        }

        if let Some(abandoned) = config.get("abandoned") {
            package.complete_mut().set_abandoned(abandoned.clone());
        }

        if self.load_options
            && let Some(transport_options) = config.get("transport-options")
        {
            let options = php_to_map(transport_options);
            package.package_mut().set_transport_options(options);
        }

        let alias_normalized = self.get_branch_alias(config)?;
        if let Some(alias_normalized) = alias_normalized
            && !alias_normalized.is_empty()
        {
            let pretty_alias = Preg::replace(r"{(\.9{7})+}", ".x", &alias_normalized);

            return Ok(match package {
                CompleteOrRootPackage::Root(root) => RootAliasPackageHandle::new(
                    RootPackageHandle::from_root_package(root),
                    alias_normalized,
                    pretty_alias,
                )
                .into(),
                CompleteOrRootPackage::Complete(complete) => CompleteAliasPackageHandle::new(
                    CompletePackageHandle::from_complete_package(complete),
                    alias_normalized,
                    pretty_alias,
                )
                .into(),
            });
        }

        Ok(package.into_handle())
    }

    fn configure_cached_links(
        &self,
        link_cache: &mut IndexMap<
            String,
            IndexMap<String, IndexMap<String, IndexMap<String, (String, Link)>>>,
        >,
        package: &mut CompleteOrRootPackage,
        config: &IndexMap<String, PhpMixed>,
    ) -> Result<()> {
        let name = package.get_name().to_string();
        let pretty_version = package.get_pretty_version().to_string();

        for (r#type, opts) in SUPPORTED_LINK_TYPES.iter() {
            if let Some(entry) = config.get(*r#type) {
                let mut links: IndexMap<String, Link> = IndexMap::new();
                let entries: IndexMap<String, PhpMixed> = match entry {
                    PhpMixed::Array(m) => m.clone(),
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
                                .or_default()
                                .entry(r#type.to_string())
                                .or_default()
                                .entry(target.clone())
                                .or_default()
                                .insert(constraint_str.clone(), entry.clone());
                            entry
                        };
                        links.insert(target, link);
                    }
                }

                apply_link_setter(package.package_mut(), opts.method, links);
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
            Err(_e) => {
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

        Ok(Link::new(
            source.to_string(),
            target.to_string(),
            parsed_constraint,
            Some(description.to_string()),
            pretty_constraint.to_string(),
        ))
    }

    /// Retrieves a branch alias (dev-master => 1.0.x-dev for example) if it exists
    ///
    /// @param mixed[] $config the entire package config
    ///
    /// @return string|null normalized version of the branch alias or null if there is none
    pub fn get_branch_alias(&self, config: &IndexMap<String, PhpMixed>) -> Result<Option<String>> {
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
            .and_then(|v| match v {
                PhpMixed::Array(m) => Some(m),
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
                let validated_target_branch = if target_branch
                    == VersionParser::DEFAULT_BRANCH_ALIAS
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
                let source_prefix = self
                    .version_parser
                    .parse_numeric_alias_prefix(&source_branch);
                let target_prefix = self
                    .version_parser
                    .parse_numeric_alias_prefix(&target_branch);
                if let (Some(sp), Some(tp)) = (source_prefix.as_ref(), target_prefix.as_ref())
                    && stripos(tp, sp) != Some(0)
                {
                    continue;
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
