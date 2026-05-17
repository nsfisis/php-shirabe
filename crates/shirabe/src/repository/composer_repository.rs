//! ref: composer/src/Composer/Repository/ComposerRepository.php

use indexmap::IndexMap;
use shirabe_external_packages::composer::metadata_minifier::metadata_minifier::MetadataMinifier;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use shirabe_php_shim::{
    InvalidArgumentException, JSON_UNESCAPED_SLASHES, JSON_UNESCAPED_UNICODE, LogicException,
    PHP_EOL, PhpMixed, RuntimeException, UnexpectedValueException, extension_loaded, hash,
    http_build_query, in_array, json_decode, parse_url_all, realpath, spl_object_hash, strtolower,
    strtr, urlencode, var_export,
};

use shirabe_semver::compiling_matcher::CompilingMatcher;
use shirabe_semver::constraint::constraint::Constraint;
use shirabe_semver::constraint::constraint_interface::ConstraintInterface;
use shirabe_semver::constraint::match_all_constraint::MatchAllConstraint;

use crate::advisory::partial_security_advisory::PartialSecurityAdvisory;
use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::transport_exception::TransportException;
use crate::event_dispatcher::event_dispatcher::EventDispatcher;
use crate::io::io_interface::IOInterface;
use crate::json::json_file::JsonFile;
use crate::package::base_package::BasePackage;
use crate::package::loader::array_loader::ArrayLoader;
use crate::package::package_interface::PackageInterface;
use crate::package::version::stability_filter::StabilityFilter;
use crate::package::version::version_parser::VersionParser;
use crate::plugin::plugin_events::PluginEvents;
use crate::plugin::post_file_download_event::PostFileDownloadEvent;
use crate::plugin::pre_file_download_event::PreFileDownloadEvent;
use crate::repository::advisory_provider_interface::{
    PartialOrSecurityAdvisory, SecurityAdvisoryResult,
};
use crate::repository::array_repository::ArrayRepository;
use crate::repository::configurable_repository_interface::ConfigurableRepositoryInterface;
use crate::repository::platform_repository::PlatformRepository;
use crate::repository::repository_security_exception::RepositorySecurityException;
use crate::util::http::response::Response;
use crate::util::http_downloader::HttpDownloader;
use crate::util::r#loop::Loop;
use crate::util::url::Url;

#[derive(Debug)]
pub enum RootData {
    Data(IndexMap<String, PhpMixed>),
    True,
}

#[derive(Debug)]
pub struct SecurityAdvisoryConfig {
    pub metadata: bool,
    pub api_url: Option<String>,
}

#[derive(Debug)]
pub struct SourceMirror {
    pub url: String,
    pub preferred: bool,
}

#[derive(Debug)]
pub struct DistMirror {
    pub url: String,
    pub preferred: bool,
}

#[derive(Debug)]
pub struct ProviderListingEntry {
    pub sha256: String,
}

#[derive(Debug)]
pub struct ComposerRepository {
    inner: ArrayRepository,
    /// @phpstan-var array{url: string, options?: mixed[], type?: 'composer', allow_ssl_downgrade?: bool}
    repo_config: IndexMap<String, PhpMixed>,
    options: IndexMap<String, PhpMixed>,
    /// non-empty-string
    url: String,
    /// non-empty-string
    base_url: String,
    io: Box<dyn IOInterface>,
    http_downloader: HttpDownloader,
    r#loop: Loop,
    pub(crate) cache: Cache,
    pub(crate) notify_url: Option<String>,
    pub(crate) search_url: Option<String>,
    pub(crate) providers_api_url: Option<String>,
    pub(crate) has_providers: bool,
    pub(crate) providers_url: Option<String>,
    pub(crate) list_url: Option<String>,
    pub(crate) has_available_package_list: bool,
    pub(crate) available_packages: Option<IndexMap<String, String>>,
    pub(crate) available_package_patterns: Option<Vec<String>>,
    pub(crate) lazy_providers_url: Option<String>,
    pub(crate) provider_listing: Option<IndexMap<String, ProviderListingEntry>>,
    pub(crate) loader: ArrayLoader,
    allow_ssl_downgrade: bool,
    event_dispatcher: Option<EventDispatcher>,
    source_mirrors: Option<IndexMap<String, Vec<SourceMirror>>>,
    dist_mirrors: Option<Vec<DistMirror>>,
    degraded_mode: bool,
    root_data: Option<RootData>,
    has_partial_packages: bool,
    partial_packages_by_name: Option<IndexMap<String, Vec<IndexMap<String, PhpMixed>>>>,
    displayed_warning_about_non_matching_package_index: bool,
    security_advisory_config: Option<SecurityAdvisoryConfig>,
    /// list of package names which are fresh and can be loaded from the cache directly in case loadPackage is called several times
    /// useful for v2 metadata repositories with lazy providers
    freshMetadataUrls: IndexMap<String, bool>,
    /// list of package names which returned a 404 and should not be re-fetched in case loadPackage is called several times
    /// useful for v2 metadata repositories with lazy providers
    packagesNotFoundCache: IndexMap<String, bool>,
    version_parser: VersionParser,
}

#[derive(Debug)]
pub enum FindPackageReturn {
    Package(Box<dyn BasePackage>),
    Packages(Vec<Box<dyn BasePackage>>),
    None,
}

#[derive(Debug)]
pub struct LoadPackagesResult {
    pub names_found: Vec<String>,
    pub packages: IndexMap<String, Box<dyn BasePackage>>,
}

#[derive(Debug)]
pub struct LoadAsyncPackagesResult {
    pub names_found: IndexMap<String, bool>,
    pub packages: IndexMap<String, Box<dyn BasePackage>>,
}

impl ConfigurableRepositoryInterface for ComposerRepository {
    fn get_repo_config(&self) -> IndexMap<String, PhpMixed> {
        self.repo_config.clone()
    }
}

impl ComposerRepository {
    pub fn new(
        mut repo_config: IndexMap<String, PhpMixed>,
        io: Box<dyn IOInterface>,
        config: &Config,
        http_downloader: HttpDownloader,
        event_dispatcher: Option<EventDispatcher>,
    ) -> anyhow::Result<Self> {
        // parent::__construct();
        let inner = ArrayRepository::new();

        let url_str = repo_config
            .get("url")
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .to_string();
        if !Preg::is_match(r"{^[\w.]+\??://}", &url_str)? {
            if let Some(local_file_path) = realpath(&url_str) {
                // it is a local path, add file scheme
                repo_config.insert(
                    "url".to_string(),
                    PhpMixed::String(format!("file://{}", local_file_path)),
                );
            } else {
                // otherwise, assume http as the default protocol
                repo_config.insert(
                    "url".to_string(),
                    PhpMixed::String(format!("http://{}", url_str)),
                );
            }
        }
        let url_after = repo_config
            .get("url")
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .trim_end_matches('/')
            .to_string();
        repo_config.insert("url".to_string(), PhpMixed::String(url_after.clone()));
        if url_after.is_empty() {
            return Err(InvalidArgumentException {
                message: "The repository url must not be an empty string".to_string(),
                code: 0,
            }
            .into());
        }

        if url_after.starts_with("https?") {
            let scheme = if extension_loaded("openssl") {
                "https"
            } else {
                "http"
            };
            let rest = &url_after[6..];
            repo_config.insert(
                "url".to_string(),
                PhpMixed::String(format!("{}{}", scheme, rest)),
            );
        }

        let current_url = repo_config
            .get("url")
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .to_string();
        let url_bits = parse_url_all(&strtr(&current_url, "\\", "/"));
        let url_bits_arr = url_bits.as_array();
        let scheme_present = url_bits_arr
            .and_then(|a| a.get("scheme"))
            .and_then(|v| v.as_string())
            .map_or(false, |s| !s.is_empty());
        if url_bits_arr.is_none() || !scheme_present {
            return Err(UnexpectedValueException {
                message: format!("Invalid url given for Composer repository: {}", current_url),
                code: 0,
            }
            .into());
        }

        if !repo_config.contains_key("options") {
            repo_config.insert("options".to_string(), PhpMixed::Array(IndexMap::new()));
        }
        let mut allow_ssl_downgrade = false;
        if let Some(v) = repo_config.get("allow_ssl_downgrade") {
            if v.as_bool() == Some(true) {
                allow_ssl_downgrade = true;
            }
        }

        let options = repo_config
            .get("options")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|(k, v)| (k, *v))
            .collect::<IndexMap<String, PhpMixed>>();
        let mut url = repo_config
            .get("url")
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .to_string();

        // force url for packagist.org to repo.packagist.org
        let mut match_packagist: Vec<String> = Vec::new();
        if Preg::is_match_with_matches(
            r"{^(?P<proto>https?)://packagist\.org/?$}i",
            &url,
            &mut match_packagist,
        )? {
            let proto = match_packagist.get(1).cloned().unwrap_or_default();
            url = format!("{}://repo.packagist.org", proto);
        }

        let base_url_trimmed = Preg::replace(r"{(?:/[^/\\]+\.json)?(?:[?#].*)?$}", "", &url)?;
        let base_url = base_url_trimmed.trim_end_matches('/').to_string();
        assert!(!base_url.is_empty());

        let cache = Cache::new(
            &*io,
            format!(
                "{}/{}",
                config.get("cache-repo-dir").as_string().unwrap_or(""),
                Preg::replace(r"{[^a-z0-9.]}i", "-", &Url::sanitize(url.clone()))?,
            ),
        );
        let version_parser = VersionParser::new();
        let loader = ArrayLoader::new_with_parser(version_parser.clone());

        let r#loop = Loop::new(http_downloader.clone(), None);

        let mut this = Self {
            inner,
            repo_config,
            options,
            url,
            base_url,
            io,
            http_downloader,
            r#loop,
            cache,
            notify_url: None,
            search_url: None,
            providers_api_url: None,
            has_providers: false,
            providers_url: None,
            list_url: None,
            has_available_package_list: false,
            available_packages: None,
            available_package_patterns: None,
            lazy_providers_url: None,
            provider_listing: None,
            loader,
            allow_ssl_downgrade,
            event_dispatcher,
            source_mirrors: None,
            dist_mirrors: None,
            degraded_mode: false,
            root_data: None,
            has_partial_packages: false,
            partial_packages_by_name: None,
            displayed_warning_about_non_matching_package_index: false,
            security_advisory_config: None,
            freshMetadataUrls: IndexMap::new(),
            packagesNotFoundCache: IndexMap::new(),
            version_parser,
        };
        this.cache
            .set_read_only(config.get("cache-read-only").as_bool().unwrap_or(false));
        Ok(this)
    }

    pub fn get_repo_name(&self) -> String {
        format!("composer repo ({})", Url::sanitize(self.url.clone()))
    }

    pub fn get_repo_config_pub(&self) -> IndexMap<String, PhpMixed> {
        self.repo_config.clone()
    }

    /// @inheritDoc
    pub fn find_package(
        &mut self,
        name: String,
        constraint: PhpMixed,
    ) -> anyhow::Result<Option<Box<dyn BasePackage>>> {
        // this call initializes loadRootServerFile which is needed for the rest below to work
        let has_providers = self.has_providers()?;

        let name = strtolower(&name);
        let constraint: Box<dyn ConstraintInterface> = match constraint {
            PhpMixed::String(s) => self.version_parser.parse_constraints(&s)?,
            _ => {
                // already a ConstraintInterface object passed as opaque PhpMixed
                self.version_parser.parse_constraints("")?
            }
        };

        if self.lazy_providers_url.is_some() {
            if self.has_partial_packages()?
                && self
                    .partial_packages_by_name
                    .as_ref()
                    .map_or(false, |m| m.contains_key(&name))
            {
                let packages = self.what_provides(&name, None, None, IndexMap::new())?;
                let packages_vec: Vec<Box<dyn BasePackage>> = packages.into_values().collect();
                return Ok(
                    match self.filter_packages(packages_vec, Some(&*constraint), true) {
                        FindPackageReturn::Package(p) => Some(p),
                        _ => None,
                    },
                );
            }

            if self.has_available_package_list && !self.lazy_providers_repo_contains(&name)? {
                return Ok(None);
            }

            let mut map: IndexMap<String, Option<Box<dyn ConstraintInterface>>> = IndexMap::new();
            map.insert(name.clone(), Some(constraint));
            let packages = self.load_async_packages(map, None, None, IndexMap::new())?;

            if !packages.packages.is_empty() {
                return Ok(packages.packages.into_iter().next().map(|(_, v)| v));
            }

            return Ok(None);
        }

        if has_providers {
            for provider_name in self.get_provider_names()? {
                if name == provider_name {
                    let packages =
                        self.what_provides(&provider_name, None, None, IndexMap::new())?;
                    let packages_vec: Vec<Box<dyn BasePackage>> = packages.into_values().collect();
                    return Ok(
                        match self.filter_packages(packages_vec, Some(&*constraint), true) {
                            FindPackageReturn::Package(p) => Some(p),
                            _ => None,
                        },
                    );
                }
            }

            return Ok(None);
        }

        Ok(self.inner.find_package(name, Some(constraint)))
    }

    /// @inheritDoc
    pub fn find_packages(
        &mut self,
        name: String,
        constraint: Option<PhpMixed>,
    ) -> anyhow::Result<Vec<Box<dyn BasePackage>>> {
        // this call initializes loadRootServerFile which is needed for the rest below to work
        let has_providers = self.has_providers()?;

        let name = strtolower(&name);
        let constraint: Option<Box<dyn ConstraintInterface>> = match constraint {
            None => None,
            Some(PhpMixed::String(s)) => Some(self.version_parser.parse_constraints(&s)?),
            Some(_) => None,
        };

        if self.lazy_providers_url.is_some() {
            if self.has_partial_packages()?
                && self
                    .partial_packages_by_name
                    .as_ref()
                    .map_or(false, |m| m.contains_key(&name))
            {
                let packages = self.what_provides(&name, None, None, IndexMap::new())?;
                let packages_vec: Vec<Box<dyn BasePackage>> = packages.into_values().collect();
                return Ok(
                    match self.filter_packages(packages_vec, constraint.as_deref(), false) {
                        FindPackageReturn::Packages(v) => v,
                        _ => vec![],
                    },
                );
            }

            if self.has_available_package_list && !self.lazy_providers_repo_contains(&name)? {
                return Ok(vec![]);
            }

            let mut map: IndexMap<String, Option<Box<dyn ConstraintInterface>>> = IndexMap::new();
            map.insert(name.clone(), constraint);
            let result = self.load_async_packages(map, None, None, IndexMap::new())?;

            return Ok(result.packages.into_values().collect());
        }

        if has_providers {
            for provider_name in self.get_provider_names()? {
                if name == provider_name {
                    let packages =
                        self.what_provides(&provider_name, None, None, IndexMap::new())?;
                    let packages_vec: Vec<Box<dyn BasePackage>> = packages.into_values().collect();
                    return Ok(
                        match self.filter_packages(packages_vec, constraint.as_deref(), false) {
                            FindPackageReturn::Packages(v) => v,
                            _ => vec![],
                        },
                    );
                }
            }

            return Ok(vec![]);
        }

        Ok(self.inner.find_packages(name, constraint))
    }

    fn filter_packages(
        &self,
        packages: Vec<Box<dyn BasePackage>>,
        constraint: Option<&dyn ConstraintInterface>,
        return_first_match: bool,
    ) -> FindPackageReturn {
        if constraint.is_none() {
            if return_first_match {
                return match packages.into_iter().next() {
                    Some(p) => FindPackageReturn::Package(p),
                    None => FindPackageReturn::None,
                };
            }

            return FindPackageReturn::Packages(packages);
        }
        let constraint = constraint.unwrap();

        let mut filtered_packages: Vec<Box<dyn BasePackage>> = Vec::new();

        for package in packages.into_iter() {
            let pkg_constraint = Constraint::new("==", package.get_version().to_string());

            if constraint.matches(&pkg_constraint) {
                if return_first_match {
                    return FindPackageReturn::Package(package);
                }

                filtered_packages.push(package);
            }
        }

        if return_first_match {
            return FindPackageReturn::None;
        }

        FindPackageReturn::Packages(filtered_packages)
    }

    pub fn get_packages(&mut self) -> anyhow::Result<Vec<Box<dyn BasePackage>>> {
        let has_providers = self.has_providers()?;

        if self.lazy_providers_url.is_some() {
            if let Some(ref available_packages) = self.available_packages.clone() {
                if self.available_package_patterns.is_none() {
                    let mut package_map: IndexMap<String, Option<Box<dyn ConstraintInterface>>> =
                        IndexMap::new();
                    for name in available_packages.values() {
                        package_map.insert(
                            name.clone(),
                            Some(
                                Box::new(MatchAllConstraint::new()) as Box<dyn ConstraintInterface>
                            ),
                        );
                    }

                    let result =
                        self.load_async_packages(package_map, None, None, IndexMap::new())?;

                    return Ok(result.packages.into_values().collect());
                }
            }

            if self.has_partial_packages()? {
                if self.partial_packages_by_name.is_none() {
                    return Err(LogicException {
                        message:
                            "hasPartialPackages failed to initialize $this->partialPackagesByName"
                                .to_string(),
                        code: 0,
                    }
                    .into());
                }

                let partial = self.partial_packages_by_name.clone().unwrap();
                let flat: Vec<IndexMap<String, PhpMixed>> =
                    partial.into_values().flatten().collect();
                return self
                    .create_packages_flat(flat, Some("packages.json inline packages".to_string()));
            }

            return Err(LogicException {
                message: "Composer repositories that have lazy providers and no available-packages list can not load the complete list of packages, use getPackageNames instead.".to_string(),
                code: 0,
            }.into());
        }

        if has_providers {
            return Err(LogicException {
                message: "Composer repositories that have providers can not load the complete list of packages, use getPackageNames instead.".to_string(),
                code: 0,
            }.into());
        }

        Ok(self.inner.get_packages())
    }

    /// @param packageFilter Package pattern filter which can include "*" as a wildcard
    pub fn get_package_names(
        &mut self,
        package_filter: Option<&str>,
    ) -> anyhow::Result<Vec<String>> {
        let has_providers = self.has_providers()?;

        let package_filter_regex: Option<String> = match package_filter {
            Some(p) if !p.is_empty() => Some(BasePackage::package_name_to_regexp(p)),
            _ => None,
        };
        let filter_results = |results: Vec<String>| -> anyhow::Result<Vec<String>> {
            match &package_filter_regex {
                Some(regex) => Ok(Preg::grep(regex, &results)?),
                None => Ok(results),
            }
        };

        if self.lazy_providers_url.is_some() {
            if let Some(ref available_packages) = self.available_packages {
                let keys: Vec<String> = available_packages.keys().cloned().collect();
                return filter_results(keys);
            }

            if self.list_url.is_some() {
                // no need to call $filterResults here as the $packageFilter is applied in the function itself
                return self.load_package_list(package_filter);
            }

            if self.has_partial_packages()? && self.partial_packages_by_name.is_some() {
                let keys: Vec<String> = self
                    .partial_packages_by_name
                    .as_ref()
                    .unwrap()
                    .keys()
                    .cloned()
                    .collect();
                return filter_results(keys);
            }

            return Ok(vec![]);
        }

        if has_providers {
            return filter_results(self.get_provider_names()?);
        }

        let mut names: Vec<String> = Vec::new();
        for package in self.get_packages()? {
            names.push(package.get_pretty_name().to_string());
        }

        filter_results(names)
    }

    fn get_vendor_names(&mut self) -> anyhow::Result<Vec<String>> {
        let cache_key = "vendor-list.txt";
        let cache_age = self.cache.get_age(cache_key);
        if let Some(age) = cache_age {
            if age < 600 {
                if let Some(cached_data) = self.cache.read(cache_key) {
                    let cached_data: Vec<String> =
                        cached_data.split('\n').map(|s| s.to_string()).collect();
                    return Ok(cached_data);
                }
            }
        }

        let names = self.get_package_names(None)?;

        let mut uniques: IndexMap<String, bool> = IndexMap::new();
        for name in &names {
            let vendor = name.splitn(2, '/').next().unwrap_or("").to_string();
            uniques.insert(vendor, true);
        }

        let vendors: Vec<String> = uniques.keys().cloned().collect();

        if !self.cache.is_read_only() {
            self.cache.write(cache_key, &vendors.join("\n"));
        }

        Ok(vendors)
    }

    fn load_package_list(&mut self, package_filter: Option<&str>) -> anyhow::Result<Vec<String>> {
        if self.list_url.is_none() {
            return Err(LogicException {
                message: "Make sure to call loadRootServerFile before loadPackageList".to_string(),
                code: 0,
            }
            .into());
        }

        let mut url = self.list_url.clone().unwrap();
        if let Some(filter) = package_filter {
            if !filter.is_empty() {
                url.push_str(&format!("?filter={}", urlencode(filter)));
                let result = self
                    .http_downloader
                    .get(&url, &self.options)?
                    .decode_json()?;
                let package_names: Vec<String> = result
                    .as_array()
                    .and_then(|a| a.get("packageNames"))
                    .and_then(|v| v.as_list())
                    .map(|l| {
                        l.iter()
                            .filter_map(|v| v.as_string().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();

                return Ok(package_names);
            }
        }

        let cache_key = "package-list.txt";
        let cache_age = self.cache.get_age(cache_key);
        if let Some(age) = cache_age {
            if age < 600 {
                if let Some(cached_data) = self.cache.read(cache_key) {
                    let cached_data: Vec<String> =
                        cached_data.split('\n').map(|s| s.to_string()).collect();
                    return Ok(cached_data);
                }
            }
        }

        let result = self
            .http_downloader
            .get(&url, &self.options)?
            .decode_json()?;
        let package_names: Vec<String> = result
            .as_array()
            .and_then(|a| a.get("packageNames"))
            .and_then(|v| v.as_list())
            .map(|l| {
                l.iter()
                    .filter_map(|v| v.as_string().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        if !self.cache.is_read_only() {
            self.cache.write(cache_key, &package_names.join("\n"));
        }

        Ok(package_names)
    }

    pub fn load_packages(
        &mut self,
        mut package_name_map: IndexMap<String, Option<Box<dyn ConstraintInterface>>>,
        acceptable_stabilities: IndexMap<String, i64>,
        stability_flags: IndexMap<String, i64>,
        already_loaded: IndexMap<String, IndexMap<String, Box<dyn PackageInterface>>>,
    ) -> anyhow::Result<LoadPackagesResult> {
        // this call initializes loadRootServerFile which is needed for the rest below to work
        let has_providers = self.has_providers()?;

        if !has_providers && !self.has_partial_packages()? && self.lazy_providers_url.is_none() {
            return self.inner.load_packages(
                package_name_map,
                acceptable_stabilities,
                stability_flags,
                already_loaded,
            );
        }

        let mut packages: IndexMap<String, Box<dyn BasePackage>> = IndexMap::new();
        let mut names_found: IndexMap<String, bool> = IndexMap::new();

        if has_providers || self.has_partial_packages()? {
            let names: Vec<String> = package_name_map.keys().cloned().collect();
            for name in names {
                let mut matches: IndexMap<String, Box<dyn BasePackage>> = IndexMap::new();

                // if a repo has no providers but only partial packages and the partial packages are missing
                // then we don't want to call whatProvides as it would try to load from the providers and fail
                if !has_providers
                    && !self
                        .partial_packages_by_name
                        .as_ref()
                        .map_or(false, |m| m.contains_key(&name))
                {
                    continue;
                }

                let candidates = self.what_provides(
                    &name,
                    Some(&acceptable_stabilities),
                    Some(&stability_flags),
                    already_loaded.clone(),
                )?;
                let constraint = package_name_map.get(&name).cloned().flatten();
                for (_uid, candidate) in candidates.iter() {
                    if candidate.get_name() != name {
                        return Err(LogicException {
                            message: "whatProvides should never return a package with a different name than the requested one".to_string(),
                            code: 0,
                        }.into());
                    }
                    names_found.insert(name.clone(), true);

                    let matches_constraint = match &constraint {
                        None => true,
                        Some(c) => {
                            let pkg_c = Constraint::new("==", candidate.get_version().to_string());
                            c.matches(&pkg_c)
                        }
                    };
                    if matches_constraint {
                        let hash_c = spl_object_hash(&**candidate);
                        matches.insert(hash_c, dyn_clone_box(&**candidate));
                        if let Some(alias) = candidate.as_alias_package() {
                            let aliased = alias.get_alias_of();
                            let aliased_hash = spl_object_hash(aliased);
                            if !matches.contains_key(&aliased_hash) {
                                matches.insert(aliased_hash, dyn_clone_box(aliased));
                            }
                        }
                    }
                }

                // add aliases of matched packages even if they did not match the constraint
                for (_uid, candidate) in candidates.iter() {
                    if let Some(alias) = candidate.as_alias_package() {
                        let aliased = alias.get_alias_of();
                        let aliased_hash = spl_object_hash(aliased);
                        if matches.contains_key(&aliased_hash) {
                            let hash_c = spl_object_hash(&**candidate);
                            matches.insert(hash_c, dyn_clone_box(&**candidate));
                        }
                    }
                }
                for (k, v) in matches.into_iter() {
                    packages.insert(k, v);
                }

                package_name_map.shift_remove(&name);
            }
        }

        if self.lazy_providers_url.is_some() && !package_name_map.is_empty() {
            if self.has_available_package_list {
                let names: Vec<String> = package_name_map.keys().cloned().collect();
                for name in names {
                    if !self.lazy_providers_repo_contains(&strtolower(&name))? {
                        package_name_map.shift_remove(&name);
                    }
                }
            }

            let result = self.load_async_packages(
                package_name_map,
                Some(&acceptable_stabilities),
                Some(&stability_flags),
                already_loaded,
            )?;
            for (k, v) in result.packages.into_iter() {
                packages.insert(k, v);
            }
            for (k, v) in result.names_found.into_iter() {
                names_found.insert(k, v);
            }
        }

        Ok(LoadPackagesResult {
            names_found: names_found.keys().cloned().collect(),
            packages,
        })
    }

    /// @inheritDoc
    pub fn search(
        &mut self,
        query: String,
        mode: i64,
        r#type: Option<String>,
    ) -> anyhow::Result<Vec<IndexMap<String, PhpMixed>>> {
        self.load_root_server_file(Some(600))?;

        if let Some(search_url) = self.search_url.clone() {
            if mode == SEARCH_FULLTEXT {
                let url = search_url
                    .replace("%query%", &urlencode(&query))
                    .replace("%type%", r#type.as_deref().unwrap_or(""));

                let search = self
                    .http_downloader
                    .get(&url, &self.options)?
                    .decode_json()?;

                let results_arr = search
                    .as_array()
                    .and_then(|a| a.get("results"))
                    .and_then(|v| v.as_list())
                    .cloned()
                    .unwrap_or_default();
                if results_arr.is_empty() {
                    return Ok(vec![]);
                }

                let mut results: Vec<IndexMap<String, PhpMixed>> = Vec::new();
                for result in results_arr.iter() {
                    let arr = match result.as_array() {
                        Some(a) => a,
                        None => continue,
                    };
                    // do not show virtual packages in results as they are not directly useful from a composer perspective
                    if let Some(v) = arr.get("virtual") {
                        // PHP's `empty()` is false when the value is truthy
                        let is_empty = match v {
                            PhpMixed::Null => true,
                            PhpMixed::Bool(false) => true,
                            PhpMixed::Int(0) => true,
                            PhpMixed::Float(f) if *f == 0.0 => true,
                            PhpMixed::String(s) if s.is_empty() || s == "0" => true,
                            PhpMixed::List(l) if l.is_empty() => true,
                            PhpMixed::Array(a) if a.is_empty() => true,
                            _ => false,
                        };
                        if !is_empty {
                            continue;
                        }
                    }

                    results.push(
                        arr.iter()
                            .map(|(k, v)| (k.clone(), (**v).clone()))
                            .collect(),
                    );
                }

                return Ok(results);
            }
        }

        if mode == SEARCH_VENDOR {
            let mut results: Vec<IndexMap<String, PhpMixed>> = Vec::new();
            let parts = Preg::split(r"{\s+}", &query)?;
            let regex = format!("{{(?:{})}}i", parts.join("|"));

            let vendor_names = self.get_vendor_names()?;
            for name in Preg::grep(&regex, &vendor_names)? {
                let mut entry = IndexMap::new();
                entry.insert("name".to_string(), PhpMixed::String(name));
                entry.insert("description".to_string(), PhpMixed::String(String::new()));
                results.push(entry);
            }

            return Ok(results);
        }

        if self.has_providers()? || self.lazy_providers_url.is_some() {
            // optimize search for "^foo/bar" where at least "^foo/" is present by loading this directly from the listUrl if present
            let mut match_groups: Vec<String> = Vec::new();
            if Preg::is_match_strict_groups(
                r"{^\^(?P<query>(?P<vendor>[a-z0-9_.-]+)/[a-z0-9_.-]*)\*?$}i",
                &query,
                &mut match_groups,
            )? && self.list_url.is_some()
            {
                let q = match_groups.get(1).cloned().unwrap_or_default();
                let vendor = match_groups.get(2).cloned().unwrap_or_default();
                let url = format!(
                    "{}?vendor={}&filter={}",
                    self.list_url.as_ref().unwrap(),
                    urlencode(&vendor),
                    urlencode(&format!("{}*", q)),
                );
                let result = self
                    .http_downloader
                    .get(&url, &self.options)?
                    .decode_json()?;

                let mut results: Vec<IndexMap<String, PhpMixed>> = Vec::new();
                if let Some(list) = result
                    .as_array()
                    .and_then(|a| a.get("packageNames"))
                    .and_then(|v| v.as_list())
                {
                    for name_mixed in list.iter() {
                        if let Some(name) = name_mixed.as_string() {
                            let mut entry = IndexMap::new();
                            entry.insert("name".to_string(), PhpMixed::String(name.to_string()));
                            entry
                                .insert("description".to_string(), PhpMixed::String(String::new()));
                            results.push(entry);
                        }
                    }
                }

                return Ok(results);
            }

            let mut results: Vec<IndexMap<String, PhpMixed>> = Vec::new();
            let parts = Preg::split(r"{\s+}", &query)?;
            let regex = format!("{{(?:{})}}i", parts.join("|"));

            let package_names = self.get_package_names(None)?;
            for name in Preg::grep(&regex, &package_names)? {
                let mut entry = IndexMap::new();
                entry.insert("name".to_string(), PhpMixed::String(name));
                entry.insert("description".to_string(), PhpMixed::String(String::new()));
                results.push(entry);
            }

            return Ok(results);
        }

        Ok(self.inner.search(query, mode, None))
    }

    pub fn has_security_advisories(&mut self) -> anyhow::Result<bool> {
        self.load_root_server_file(Some(600))?;

        Ok(self
            .security_advisory_config
            .as_ref()
            .map_or(false, |c| c.metadata || c.api_url.is_some()))
    }

    /// @inheritDoc
    pub fn get_security_advisories(
        &mut self,
        mut package_constraint_map: IndexMap<String, Box<dyn ConstraintInterface>>,
        allow_partial_advisories: bool,
    ) -> anyhow::Result<SecurityAdvisoryResult> {
        self.load_root_server_file(Some(600))?;
        if self.security_advisory_config.is_none() {
            return Ok(SecurityAdvisoryResult {
                names_found: vec![],
                advisories: IndexMap::new(),
            });
        }

        let mut advisories: IndexMap<String, Vec<PartialOrSecurityAdvisory>> = IndexMap::new();
        let mut names_found: IndexMap<String, bool> = IndexMap::new();

        let api_url = self
            .security_advisory_config
            .as_ref()
            .and_then(|c| c.api_url.clone());

        // respect available-package-patterns / available-packages directives from the repo
        if self.has_available_package_list {
            let names: Vec<String> = package_constraint_map.keys().cloned().collect();
            for name in names {
                if !self.lazy_providers_repo_contains(&strtolower(&name))? {
                    package_constraint_map.shift_remove(&name);
                }
            }
        }

        let parser = VersionParser::new();
        let repo_name = self.get_repo_name();
        let create = |data: &IndexMap<String, PhpMixed>,
                      name: &str,
                      package_constraint_map: &IndexMap<String, Box<dyn ConstraintInterface>>|
         -> anyhow::Result<Option<PartialOrSecurityAdvisory>> {
            let advisory =
                PartialSecurityAdvisory::create(name.to_string(), data.clone(), &parser)?;
            let is_full = matches!(advisory, PartialOrSecurityAdvisory::Full(_));
            if !allow_partial_advisories && !is_full {
                let data_mixed = PhpMixed::Array(
                    data.iter()
                        .map(|(k, v)| (k.clone(), Box::new(v.clone())))
                        .collect(),
                );
                return Err(RuntimeException {
                    message: format!(
                        "Advisory for {} could not be loaded as a full advisory from {}{}{}",
                        name,
                        repo_name,
                        PHP_EOL,
                        var_export(&data_mixed, true),
                    ),
                    code: 0,
                }
                .into());
            }
            let affected_versions = match &advisory {
                PartialOrSecurityAdvisory::Partial(p) => &p.affected_versions,
                PartialOrSecurityAdvisory::Full(p) => &p.inner.affected_versions,
            };
            let constraint = package_constraint_map.get(name).map(|c| &**c);
            if let Some(c) = constraint {
                if !affected_versions.matches_constraint(c) {
                    return Ok(None);
                }
            } else {
                return Ok(None);
            }

            Ok(Some(advisory))
        };

        if self
            .security_advisory_config
            .as_ref()
            .map_or(false, |c| c.metadata)
            && (allow_partial_advisories || api_url.is_none())
        {
            let mut promises: Vec<Box<dyn PromiseInterface>> = Vec::new();
            let names: Vec<String> = package_constraint_map.keys().cloned().collect();
            for name in names {
                let name = strtolower(&name);

                // skip platform packages, root package and composer-plugin-api
                if PlatformRepository::is_platform_package(&name) || name == "__root__" {
                    continue;
                }

                let promise = self
                    .start_cached_async_download(&name, Some(&name))?
                    .then_boxed(Box::new({
                        let advisories_ptr = &mut advisories as *mut _;
                        let names_found_ptr = &mut names_found as *mut _;
                        let package_constraint_map_ptr = &mut package_constraint_map as *mut _;
                        let name = name.clone();
                        let create = &create;
                        move |spec: PhpMixed| -> anyhow::Result<()> {
                            // [$response] = $spec;
                            let response = spec
                                .as_list()
                                .and_then(|l| l.first())
                                .map(|b| (**b).clone())
                                .unwrap_or(PhpMixed::Null);
                            let response_arr = match response.as_array() {
                                Some(a) => a.clone(),
                                None => return Ok(()),
                            };
                            let sec_advs = match response_arr.get("security-advisories") {
                                Some(v) => v.clone(),
                                None => return Ok(()),
                            };
                            let sec_advs_arr = match sec_advs.as_array() {
                                Some(a) => a.clone(),
                                None => return Ok(()),
                            };
                            unsafe {
                                (*names_found_ptr).insert(name.clone(), true);
                            }
                            if !sec_advs_arr.is_empty() {
                                let mut entries: Vec<PartialOrSecurityAdvisory> = Vec::new();
                                for (_k, data_mixed) in sec_advs_arr.iter() {
                                    if let Some(data) = data_mixed.as_array() {
                                        let data_map: IndexMap<String, PhpMixed> = data
                                            .iter()
                                            .map(|(k, v)| (k.clone(), (**v).clone()))
                                            .collect();
                                        let pcm: &IndexMap<String, Box<dyn ConstraintInterface>> =
                                            unsafe { &*package_constraint_map_ptr };
                                        if let Some(adv) = create(&data_map, &name, pcm)? {
                                            entries.push(adv);
                                        }
                                    }
                                }
                                unsafe {
                                    (*advisories_ptr).insert(name.clone(), entries);
                                }
                            }
                            unsafe {
                                (*package_constraint_map_ptr).shift_remove(&name);
                            }
                            Ok(())
                        }
                    }));
                promises.push(promise);
            }

            self.r#loop.wait(promises, None)?;
        }

        if let Some(api_url) = api_url {
            if !package_constraint_map.is_empty() {
                let mut options = self.options.clone();
                let http_entry = options
                    .entry("http".to_string())
                    .or_insert(PhpMixed::Array(IndexMap::new()));
                if let PhpMixed::Array(ref mut http_map) = http_entry {
                    http_map.insert(
                        "method".to_string(),
                        Box::new(PhpMixed::String("POST".to_string())),
                    );
                    if let Some(header_box) = http_map.get("header") {
                        // cast to array
                        let arr = match &**header_box {
                            PhpMixed::List(l) => l.clone(),
                            other => vec![Box::new(other.clone())],
                        };
                        http_map.insert("header".to_string(), Box::new(PhpMixed::List(arr)));
                    }
                    let mut headers = match http_map.get("header") {
                        Some(b) => match &**b {
                            PhpMixed::List(l) => l.clone(),
                            _ => vec![],
                        },
                        None => vec![],
                    };
                    headers.push(Box::new(PhpMixed::String(
                        "Content-type: application/x-www-form-urlencoded".to_string(),
                    )));
                    http_map.insert("header".to_string(), Box::new(PhpMixed::List(headers)));
                    http_map.insert("timeout".to_string(), Box::new(PhpMixed::Int(10)));
                    let packages_list: Vec<(String, String)> = package_constraint_map
                        .keys()
                        .map(|k| ("packages".to_string(), k.clone()))
                        .collect();
                    let body = http_build_query(
                        &packages_list
                            .iter()
                            .map(|(k, v)| (k.as_str(), v.as_str()))
                            .collect::<Vec<_>>(),
                        "&",
                        "=",
                    );
                    http_map.insert("content".to_string(), Box::new(PhpMixed::String(body)));
                }

                let response = self.http_downloader.get(&api_url, &options)?;
                let mut warned = false;
                let decoded = response.decode_json()?;
                let advisories_response = decoded
                    .as_array()
                    .and_then(|a| a.get("advisories"))
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                for (name, list_box) in advisories_response.iter() {
                    if !package_constraint_map.contains_key(name) {
                        if !warned {
                            self.io.write_error(&format!(
                                "<warning>{} returned names which were not requested in response to the security-advisories API. {} was not requested but is present in the response. Requested names were: {}</warning>",
                                self.get_repo_name(),
                                name,
                                package_constraint_map.keys().cloned().collect::<Vec<_>>().join(", "),
                            ));
                            warned = true;
                        }
                        continue;
                    }
                    let list = match list_box.as_list() {
                        Some(l) => l.clone(),
                        None => continue,
                    };
                    if !list.is_empty() {
                        let mut entries: Vec<PartialOrSecurityAdvisory> = Vec::new();
                        for data_mixed in list.iter() {
                            if let Some(data) = data_mixed.as_array() {
                                let data_map: IndexMap<String, PhpMixed> = data
                                    .iter()
                                    .map(|(k, v)| (k.clone(), (**v).clone()))
                                    .collect();
                                if let Some(adv) = create(&data_map, name, &package_constraint_map)?
                                {
                                    entries.push(adv);
                                }
                            }
                        }
                        advisories.insert(name.clone(), entries);
                    }
                    names_found.insert(name.clone(), true);
                }
            }
        }

        let advisories_filtered: IndexMap<String, Vec<PartialOrSecurityAdvisory>> = advisories
            .into_iter()
            .filter(|(_, adv)| !adv.is_empty())
            .collect();

        Ok(SecurityAdvisoryResult {
            names_found: names_found.keys().cloned().collect(),
            advisories: advisories_filtered,
        })
    }

    pub fn get_providers(
        &mut self,
        package_name: &str,
    ) -> anyhow::Result<IndexMap<String, IndexMap<String, PhpMixed>>> {
        self.load_root_server_file(None)?;
        let mut result: IndexMap<String, IndexMap<String, PhpMixed>> = IndexMap::new();

        if let Some(providers_api_url) = self.providers_api_url.clone() {
            let api_result = match self.http_downloader.get(
                &providers_api_url.replace("%package%", package_name),
                &self.options,
            ) {
                Ok(resp) => resp.decode_json()?,
                Err(e) => {
                    if let Some(te) = e.downcast_ref::<TransportException>() {
                        if te.get_status_code() == 404 {
                            return Ok(result);
                        }
                    }
                    return Err(e);
                }
            };

            if let Some(providers) = api_result
                .as_array()
                .and_then(|a| a.get("providers"))
                .and_then(|v| v.as_list())
            {
                for provider_mixed in providers.iter() {
                    if let Some(provider) = provider_mixed.as_array() {
                        if let Some(name) = provider.get("name").and_then(|v| v.as_string()) {
                            let entry: IndexMap<String, PhpMixed> = provider
                                .iter()
                                .map(|(k, v)| (k.clone(), (**v).clone()))
                                .collect();
                            result.insert(name.to_string(), entry);
                        }
                    }
                }
            }

            return Ok(result);
        }

        if self.has_partial_packages()? {
            if self.partial_packages_by_name.is_none() {
                return Err(LogicException {
                    message: "hasPartialPackages failed to initialize $this->partialPackagesByName"
                        .to_string(),
                    code: 0,
                }
                .into());
            }
            for (_name, versions) in self.partial_packages_by_name.as_ref().unwrap().iter() {
                for candidate in versions.iter() {
                    let candidate_name = candidate
                        .get("name")
                        .and_then(|v| v.as_string())
                        .unwrap_or("")
                        .to_string();
                    if result.contains_key(&candidate_name)
                        || !candidate
                            .get("provide")
                            .and_then(|v| v.as_array())
                            .map_or(false, |a| a.contains_key(package_name))
                    {
                        continue;
                    }
                    let mut entry: IndexMap<String, PhpMixed> = IndexMap::new();
                    entry.insert("name".to_string(), PhpMixed::String(candidate_name.clone()));
                    entry.insert(
                        "description".to_string(),
                        candidate
                            .get("description")
                            .cloned()
                            .unwrap_or(PhpMixed::String(String::new())),
                    );
                    entry.insert(
                        "type".to_string(),
                        candidate
                            .get("type")
                            .cloned()
                            .unwrap_or(PhpMixed::String(String::new())),
                    );
                    result.insert(candidate_name, entry);
                }
            }
        }

        if !self.inner.is_packages_empty() {
            for (k, v) in self.inner.get_providers(package_name) {
                result.insert(k, v);
            }
        }

        Ok(result)
    }

    fn get_provider_names(&mut self) -> anyhow::Result<Vec<String>> {
        self.load_root_server_file(None)?;

        if self.provider_listing.is_none() {
            let data = self.load_root_server_file(None)?;
            if let RootData::Data(arr) = &data {
                let arr_clone = arr.clone();
                self.load_provider_listings(&arr_clone)?;
            }
        }

        if self.lazy_providers_url.is_some() {
            // Can not determine list of provided packages for lazy repositories
            return Ok(vec![]);
        }

        if self.providers_url.is_some() && self.provider_listing.is_some() {
            return Ok(self
                .provider_listing
                .as_ref()
                .unwrap()
                .keys()
                .cloned()
                .collect());
        }

        Ok(vec![])
    }

    fn configure_package_transport_options(&self, package: &mut dyn PackageInterface) {
        for url in package.get_dist_urls() {
            if url.starts_with(&self.base_url) {
                package.set_transport_options(self.options.clone());

                return;
            }
        }
    }

    fn has_providers(&mut self) -> anyhow::Result<bool> {
        self.load_root_server_file(None)?;

        Ok(self.has_providers)
    }

    /// @param  name package name
    fn what_provides(
        &mut self,
        name: &str,
        acceptable_stabilities: Option<&IndexMap<String, i64>>,
        stability_flags: Option<&IndexMap<String, i64>>,
        already_loaded: IndexMap<String, IndexMap<String, Box<dyn PackageInterface>>>,
    ) -> anyhow::Result<IndexMap<String, Box<dyn BasePackage>>> {
        let mut packages_source: Option<String> = None;
        let packages: IndexMap<String, PhpMixed>;
        let loading_partial_package: bool;
        if !self.has_partial_packages()?
            || !self
                .partial_packages_by_name
                .as_ref()
                .map_or(false, |m| m.contains_key(name))
        {
            // skip platform packages, root package and composer-plugin-api
            if PlatformRepository::is_platform_package(name) || name == "__root__" {
                return Ok(IndexMap::new());
            }

            if self.provider_listing.is_none() {
                let data = self.load_root_server_file(None)?;
                if let RootData::Data(arr) = &data {
                    let arr_clone = arr.clone();
                    self.load_provider_listings(&arr_clone)?;
                }
            }

            let mut use_last_modified_check = false;
            let hash_opt: Option<String>;
            let url: String;
            let cache_key: String;
            if self.lazy_providers_url.is_some()
                && !self
                    .provider_listing
                    .as_ref()
                    .map_or(false, |m| m.contains_key(name))
            {
                hash_opt = None;
                url = self
                    .lazy_providers_url
                    .as_ref()
                    .unwrap()
                    .replace("%package%", name);
                cache_key = format!("provider-{}.json", strtr(name, "/", "$"));
                use_last_modified_check = true;
            } else if let Some(providers_url) = self.providers_url.clone() {
                // package does not exist in this repo
                if !self
                    .provider_listing
                    .as_ref()
                    .map_or(false, |m| m.contains_key(name))
                {
                    return Ok(IndexMap::new());
                }

                let listing = self.provider_listing.as_ref().unwrap();
                let entry = listing.get(name).unwrap();
                hash_opt = Some(entry.sha256.clone());
                url = providers_url
                    .replace("%package%", name)
                    .replace("%hash%", &entry.sha256);
                cache_key = format!("provider-{}.json", strtr(name, "/", "$"));
            } else {
                return Ok(IndexMap::new());
            }

            let mut packages_opt: Option<IndexMap<String, PhpMixed>> = None;
            if !use_last_modified_check
                && hash_opt.is_some()
                && self.cache.sha256(&cache_key).as_deref() == hash_opt.as_deref()
            {
                if let Some(raw) = self.cache.read(&cache_key) {
                    let decoded = json_decode(&raw, true)?;
                    if let Some(arr) = decoded.as_array() {
                        let map: IndexMap<String, PhpMixed> = arr
                            .iter()
                            .map(|(k, v)| (k.clone(), (**v).clone()))
                            .collect();
                        packages_opt = Some(map);
                        packages_source = Some(format!(
                            "cached file ({} originating from {})",
                            cache_key,
                            Url::sanitize(url.clone())
                        ));
                    }
                }
            } else if use_last_modified_check {
                if let Some(contents_raw) = self.cache.read(&cache_key) {
                    let contents = json_decode(&contents_raw, true)?;
                    let contents_arr = contents.as_array().cloned();
                    // we already loaded some packages from this file, so assume it is fresh and avoid fetching it again
                    if already_loaded.contains_key(name) {
                        if let Some(arr) = &contents_arr {
                            let map: IndexMap<String, PhpMixed> = arr
                                .iter()
                                .map(|(k, v)| (k.clone(), (**v).clone()))
                                .collect();
                            packages_opt = Some(map);
                            packages_source = Some(format!(
                                "cached file ({} originating from {})",
                                cache_key,
                                Url::sanitize(url.clone())
                            ));
                        }
                    } else if let Some(arr) = &contents_arr {
                        if let Some(last_modified) =
                            arr.get("last-modified").and_then(|v| v.as_string())
                        {
                            let response =
                                self.fetch_file_if_last_modified(&url, &cache_key, last_modified)?;
                            match response {
                                FetchFileIfLastModifiedResult::NotModified => {
                                    let map: IndexMap<String, PhpMixed> = arr
                                        .iter()
                                        .map(|(k, v)| (k.clone(), (**v).clone()))
                                        .collect();
                                    packages_opt = Some(map);
                                    packages_source = Some(format!(
                                        "cached file ({} originating from {})",
                                        cache_key,
                                        Url::sanitize(url.clone())
                                    ));
                                }
                                FetchFileIfLastModifiedResult::Data(data) => {
                                    packages_opt = Some(data);
                                    packages_source = Some(format!(
                                        "downloaded file ({})",
                                        Url::sanitize(url.clone())
                                    ));
                                }
                            }
                        }
                    }
                }
            }

            if packages_opt.is_none() {
                match self.fetch_file(
                    &url,
                    Some(&cache_key),
                    hash_opt.as_deref(),
                    use_last_modified_check,
                ) {
                    Ok(p) => {
                        packages_opt = Some(p);
                        packages_source =
                            Some(format!("downloaded file ({})", Url::sanitize(url.clone())));
                    }
                    Err(e) => {
                        // 404s are acceptable for lazy provider repos
                        if let Some(te) = e.downcast_ref::<TransportException>() {
                            let status_code = te.get_status_code();
                            if self.lazy_providers_url.is_some()
                                && in_array(
                                    PhpMixed::Int(status_code),
                                    &PhpMixed::List(vec![
                                        Box::new(PhpMixed::Int(404)),
                                        Box::new(PhpMixed::Int(499)),
                                    ]),
                                    true,
                                )
                            {
                                let mut p: IndexMap<String, PhpMixed> = IndexMap::new();
                                p.insert("packages".to_string(), PhpMixed::Array(IndexMap::new()));
                                packages_opt = Some(p);
                                packages_source = Some(format!(
                                    "not-found file ({})",
                                    Url::sanitize(url.clone())
                                ));
                                if status_code == 499 {
                                    self.io
                                        .error(&format!("<warning>{}</warning>", te.get_message()));
                                }
                            } else {
                                return Err(e);
                            }
                        } else {
                            return Err(e);
                        }
                    }
                }
            }

            packages = packages_opt.unwrap();
            loading_partial_package = false;
        } else {
            let mut versions_map: IndexMap<String, PhpMixed> = IndexMap::new();
            let mut packages_inner: IndexMap<String, PhpMixed> = IndexMap::new();
            let entries = self
                .partial_packages_by_name
                .as_ref()
                .unwrap()
                .get(name)
                .cloned()
                .unwrap_or_default();
            let entries_mixed: Vec<Box<PhpMixed>> = entries
                .into_iter()
                .map(|m| {
                    Box::new(PhpMixed::Array(
                        m.into_iter().map(|(k, v)| (k, Box::new(v))).collect(),
                    ))
                })
                .collect();
            versions_map.insert("versions".to_string(), PhpMixed::List(entries_mixed));
            packages_inner.insert(
                "packages".to_string(),
                PhpMixed::Array(
                    versions_map
                        .into_iter()
                        .map(|(k, v)| (k, Box::new(v)))
                        .collect(),
                ),
            );
            packages = packages_inner;
            packages_source = Some(format!(
                "root file ({})",
                Url::sanitize(self.get_packages_json_url())
            ));
            loading_partial_package = true;
        }

        let mut result: IndexMap<String, Box<dyn BasePackage>> = IndexMap::new();
        let mut versions_to_load: IndexMap<String, IndexMap<String, PhpMixed>> = IndexMap::new();
        let packages_inner = packages
            .get("packages")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for (_pkg_key, versions_mixed) in packages_inner.iter() {
            // $versions can be either array<string, array> or list<array>
            let iter_versions: Vec<PhpMixed> = match &**versions_mixed {
                PhpMixed::Array(a) => a.values().map(|v| (**v).clone()).collect(),
                PhpMixed::List(l) => l.iter().map(|v| (**v).clone()).collect(),
                _ => continue,
            };
            for version_mixed in iter_versions.iter() {
                let version_arr = match version_mixed.as_array() {
                    Some(a) => a,
                    None => continue,
                };
                let mut version: IndexMap<String, PhpMixed> = version_arr
                    .iter()
                    .map(|(k, v)| (k.clone(), (**v).clone()))
                    .collect();
                let normalized_name = strtolower(
                    version
                        .get("name")
                        .and_then(|v| v.as_string())
                        .unwrap_or(""),
                );

                // only load the actual named package, not other packages that might find themselves in the same file
                if normalized_name != name {
                    continue;
                }

                if !loading_partial_package
                    && self.has_partial_packages()?
                    && self
                        .partial_packages_by_name
                        .as_ref()
                        .map_or(false, |m| m.contains_key(&normalized_name))
                {
                    continue;
                }

                let uid_key = match version.get("uid") {
                    Some(PhpMixed::Int(i)) => i.to_string(),
                    Some(PhpMixed::String(s)) => s.clone(),
                    Some(other) => format!("{:?}", other),
                    None => continue,
                };
                if !versions_to_load.contains_key(&uid_key) {
                    let has_version_normalized = version.contains_key("version_normalized");
                    if !has_version_normalized {
                        let v = version
                            .get("version")
                            .and_then(|v| v.as_string())
                            .unwrap_or("")
                            .to_string();
                        let normalized = self.version_parser.normalize(&v, None)?;
                        version.insert(
                            "version_normalized".to_string(),
                            PhpMixed::String(normalized),
                        );
                    } else if version
                        .get("version_normalized")
                        .and_then(|v| v.as_string())
                        .map_or(false, |s| s == VersionParser::DEFAULT_BRANCH_ALIAS)
                    {
                        // handling of existing repos which need to remain composer v1 compatible, in case the version_normalized contained VersionParser::DEFAULT_BRANCH_ALIAS, we renormalize it
                        let v = version
                            .get("version")
                            .and_then(|v| v.as_string())
                            .unwrap_or("")
                            .to_string();
                        let normalized = self.version_parser.normalize(&v, None)?;
                        version.insert(
                            "version_normalized".to_string(),
                            PhpMixed::String(normalized),
                        );
                    }

                    let version_normalized = version
                        .get("version_normalized")
                        .and_then(|v| v.as_string())
                        .unwrap_or("")
                        .to_string();
                    // avoid loading packages which have already been loaded
                    if already_loaded
                        .get(name)
                        .map_or(false, |m| m.contains_key(&version_normalized))
                    {
                        continue;
                    }

                    if self.is_version_acceptable(
                        None,
                        &normalized_name,
                        &version,
                        acceptable_stabilities,
                        stability_flags,
                    )? {
                        versions_to_load.insert(uid_key, version);
                    }
                }
            }
        }

        // load acceptable packages in the providers
        let versions_to_load_vec: Vec<IndexMap<String, PhpMixed>> =
            versions_to_load.values().cloned().collect();
        let loaded_packages = self.create_packages_flat(versions_to_load_vec, packages_source)?;
        let uids: Vec<String> = versions_to_load.keys().cloned().collect();

        for (index, mut package) in loaded_packages.into_iter().enumerate() {
            package.set_repository_self();
            let uid = &uids[index];

            if let Some(alias) = package.as_alias_package_mut() {
                let aliased = alias.get_alias_of_mut();
                aliased.set_repository_self();

                result.insert(uid.clone(), dyn_clone_box(aliased));
                result.insert(format!("{}-alias", uid), package);
            } else {
                result.insert(uid.clone(), package);
            }
        }

        Ok(result)
    }

    /// @inheritDoc
    pub fn initialize(&mut self) -> anyhow::Result<()> {
        self.inner.initialize()?;

        let repo_data = self.load_data_from_server()?;

        let source = format!(
            "root file ({})",
            Url::sanitize(self.get_packages_json_url())
        );
        for package in self.create_packages_flat(repo_data, Some(source))? {
            self.add_package(package);
        }
        Ok(())
    }

    /// Adds a new package to the repository
    pub fn add_package(&mut self, mut package: Box<dyn BasePackage>) {
        // configurePackageTransportOptions(*package);
        self.configure_package_transport_options(&mut *package);
        self.inner.add_package(package);
    }

    /// @param packageNames array of package name => ConstraintInterface|null - if a constraint is provided, only packages matching it will be loaded
    fn load_async_packages(
        &mut self,
        mut package_names: IndexMap<String, Option<Box<dyn ConstraintInterface>>>,
        acceptable_stabilities: Option<&IndexMap<String, i64>>,
        stability_flags: Option<&IndexMap<String, i64>>,
        already_loaded: IndexMap<String, IndexMap<String, Box<dyn PackageInterface>>>,
    ) -> anyhow::Result<LoadAsyncPackagesResult> {
        self.load_root_server_file(None)?;

        let mut packages: IndexMap<String, Box<dyn BasePackage>> = IndexMap::new();
        let mut names_found: IndexMap<String, bool> = IndexMap::new();
        let mut promises: Vec<Box<dyn PromiseInterface>> = Vec::new();

        if self.lazy_providers_url.is_none() {
            return Err(LogicException {
                message:
                    "loadAsyncPackages only supports v2 protocol composer repos with a metadata-url"
                        .to_string(),
                code: 0,
            }
            .into());
        }

        // load ~dev versions of the packages as well if needed
        let names_snapshot: Vec<String> = package_names.keys().cloned().collect();
        for name in names_snapshot {
            let constraint = package_names.get(&name).cloned().flatten();
            if acceptable_stabilities.is_none()
                || stability_flags.is_none()
                || StabilityFilter::is_package_acceptable(
                    acceptable_stabilities.unwrap(),
                    stability_flags.unwrap(),
                    &[name.clone()],
                    "dev",
                )
            {
                package_names.insert(format!("{}~dev", name), constraint);
            }
            // if only dev stability is requested, we skip loading the non dev file
            if acceptable_stabilities.map_or(false, |m| m.contains_key("dev") && m.len() == 1)
                && stability_flags.map_or(false, |m| m.is_empty())
            {
                package_names.shift_remove(&name);
            }
        }

        let names_iter: Vec<(String, Option<Box<dyn ConstraintInterface>>)> = package_names
            .iter()
            .map(|(k, v)| {
                let cloned: Option<Box<dyn ConstraintInterface>> =
                    v.as_ref().map(|c| dyn_clone_constraint(&**c));
                (k.clone(), cloned)
            })
            .collect();
        for (name, constraint) in names_iter {
            let name = strtolower(&name);

            let real_name = Preg::replace(r"{~dev$}", "", &name)?;
            // skip platform packages, root package and composer-plugin-api
            if PlatformRepository::is_platform_package(&real_name) || real_name == "__root__" {
                continue;
            }

            let already_loaded_clone = already_loaded.clone();
            let acceptable_stabilities_clone = acceptable_stabilities.cloned();
            let stability_flags_clone = stability_flags.cloned();
            let version_parser = self.version_parser.clone();
            let promise = self
                .start_cached_async_download(&name, Some(&real_name))?
                .then_boxed(Box::new({
                    let packages_ptr = &mut packages as *mut _;
                    let names_found_ptr = &mut names_found as *mut _;
                    let real_name = real_name.clone();
                    let constraint = constraint;
                    move |spec: PhpMixed| -> anyhow::Result<()> {
                        let spec_list = spec.as_list().cloned().unwrap_or_default();
                        let response = spec_list
                            .first()
                            .map(|b| (**b).clone())
                            .unwrap_or(PhpMixed::Null);
                        let packages_source_val = spec_list
                            .get(1)
                            .map(|b| (**b).clone())
                            .unwrap_or(PhpMixed::Null);
                        let packages_source: Option<String> =
                            packages_source_val.as_string().map(|s| s.to_string());
                        if response.is_null() {
                            return Ok(());
                        }
                        let response_arr = match response.as_array() {
                            Some(a) => a.clone(),
                            None => return Ok(()),
                        };
                        let inner_packages = response_arr.get("packages");
                        let versions_mixed = match inner_packages
                            .and_then(|v| v.as_array())
                            .and_then(|a| a.get(&real_name))
                            .cloned()
                        {
                            Some(b) => *b,
                            None => return Ok(()),
                        };

                        let mut versions: Vec<IndexMap<String, PhpMixed>> = match &versions_mixed {
                            PhpMixed::List(l) => l
                                .iter()
                                .filter_map(|v| {
                                    v.as_array().map(|a| {
                                        a.iter()
                                            .map(|(k, v)| (k.clone(), (**v).clone()))
                                            .collect::<IndexMap<String, PhpMixed>>()
                                    })
                                })
                                .collect(),
                            PhpMixed::Array(a) => a
                                .values()
                                .filter_map(|v| {
                                    v.as_array().map(|a| {
                                        a.iter()
                                            .map(|(k, v)| (k.clone(), (**v).clone()))
                                            .collect::<IndexMap<String, PhpMixed>>()
                                    })
                                })
                                .collect(),
                            _ => return Ok(()),
                        };

                        let minified = response_arr
                            .get("minified")
                            .and_then(|v| v.as_string())
                            .map_or(false, |s| s == "composer/2.0");
                        if minified {
                            versions = MetadataMinifier::expand(versions);
                        }

                        unsafe {
                            (*names_found_ptr).insert(real_name.clone(), true);
                        }
                        let mut versions_to_load: Vec<IndexMap<String, PhpMixed>> = Vec::new();
                        for version in versions.into_iter() {
                            let mut version = version;
                            let has_vn = version.contains_key("version_normalized");
                            if !has_vn {
                                let v = version
                                    .get("version")
                                    .and_then(|v| v.as_string())
                                    .unwrap_or("")
                                    .to_string();
                                let normalized = version_parser.normalize(&v, None)?;
                                version.insert(
                                    "version_normalized".to_string(),
                                    PhpMixed::String(normalized),
                                );
                            } else if version
                                .get("version_normalized")
                                .and_then(|v| v.as_string())
                                .map_or(false, |s| s == VersionParser::DEFAULT_BRANCH_ALIAS)
                            {
                                // handling of existing repos which need to remain composer v1 compatible, in case the version_normalized contained VersionParser::DEFAULT_BRANCH_ALIAS, we renormalize it
                                let v = version
                                    .get("version")
                                    .and_then(|v| v.as_string())
                                    .unwrap_or("")
                                    .to_string();
                                let normalized = version_parser.normalize(&v, None)?;
                                version.insert(
                                    "version_normalized".to_string(),
                                    PhpMixed::String(normalized),
                                );
                            }

                            let version_normalized = version
                                .get("version_normalized")
                                .and_then(|v| v.as_string())
                                .unwrap_or("")
                                .to_string();
                            // avoid loading packages which have already been loaded
                            if already_loaded_clone
                                .get(&real_name)
                                .map_or(false, |m| m.contains_key(&version_normalized))
                            {
                                continue;
                            }

                            let acceptable = ComposerRepository::is_version_acceptable_static(
                                constraint.as_deref(),
                                &real_name,
                                &version,
                                acceptable_stabilities_clone.as_ref(),
                                stability_flags_clone.as_ref(),
                            )?;
                            if acceptable {
                                versions_to_load.push(version);
                            }
                        }

                        let loaded_packages: Vec<Box<dyn BasePackage>> =
                            ComposerRepository::create_packages_static(
                                versions_to_load,
                                packages_source,
                            )?;
                        for mut package in loaded_packages.into_iter() {
                            package.set_repository_self();
                            let hash_c = spl_object_hash(&*package);
                            if let Some(alias) = package.as_alias_package_mut() {
                                let aliased_hash = spl_object_hash(alias.get_alias_of());
                                if !unsafe { (*packages_ptr).contains_key(&aliased_hash) } {
                                    alias.get_alias_of_mut().set_repository_self();
                                    let aliased_clone = dyn_clone_box(alias.get_alias_of());
                                    unsafe {
                                        (*packages_ptr).insert(aliased_hash, aliased_clone);
                                    }
                                }
                            }
                            unsafe {
                                (*packages_ptr).insert(hash_c, package);
                            }
                        }
                        Ok(())
                    }
                }));
            promises.push(promise);
        }

        self.r#loop.wait(promises, None)?;

        Ok(LoadAsyncPackagesResult {
            names_found,
            packages,
        })
    }

    fn start_cached_async_download(
        &mut self,
        file_name: &str,
        package_name: Option<&str>,
    ) -> anyhow::Result<Box<dyn PromiseInterface>> {
        if self.lazy_providers_url.is_none() {
            return Err(LogicException {
                message: "startCachedAsyncDownload only supports v2 protocol composer repos with a metadata-url".to_string(),
                code: 0,
            }.into());
        }

        let name = strtolower(file_name);
        let package_name = package_name
            .map(|s| s.to_string())
            .unwrap_or_else(|| name.clone());

        let url = self
            .lazy_providers_url
            .as_ref()
            .unwrap()
            .replace("%package%", &name);
        let cache_key = format!("provider-{}.json", strtr(&name, "/", "~"));

        let mut last_modified: Option<String> = None;
        let contents_opt: Option<IndexMap<String, PhpMixed>>;
        if let Some(raw) = self.cache.read(&cache_key) {
            let decoded = json_decode(&raw, true)?;
            if let Some(arr) = decoded.as_array() {
                let map: IndexMap<String, PhpMixed> = arr
                    .iter()
                    .map(|(k, v)| (k.clone(), (**v).clone()))
                    .collect();
                last_modified = map
                    .get("last-modified")
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_string());
                contents_opt = Some(map);
            } else {
                contents_opt = None;
            }
        } else {
            contents_opt = None;
        }

        let promise = self.async_fetch_file(&url, &cache_key, last_modified.as_deref())?;
        let url_owned = url.clone();
        let cache_key_owned = cache_key.clone();
        let contents = contents_opt;
        Ok(promise.then_boxed(Box::new(
            move |response: PhpMixed| -> anyhow::Result<PhpMixed> {
                let mut packages_source =
                    format!("downloaded file ({})", Url::sanitize(url_owned.clone()));

                let response_data = if response.as_bool() == Some(true) {
                    packages_source = format!(
                        "cached file ({} originating from {})",
                        cache_key_owned,
                        Url::sanitize(url_owned.clone())
                    );
                    contents
                        .clone()
                        .map(|m| {
                            PhpMixed::Array(m.into_iter().map(|(k, v)| (k, Box::new(v))).collect())
                        })
                        .unwrap_or(PhpMixed::Null)
                } else {
                    response
                };

                let response_arr = response_data.as_array();
                let has_pkg = response_arr
                    .and_then(|a| a.get("packages"))
                    .and_then(|v| v.as_array())
                    .map_or(false, |a| a.contains_key(&package_name));
                let has_advisories =
                    response_arr.map_or(false, |a| a.contains_key("security-advisories"));
                if !has_pkg && !has_advisories {
                    return Ok(PhpMixed::List(vec![
                        Box::new(PhpMixed::Null),
                        Box::new(PhpMixed::String(packages_source)),
                    ]));
                }

                Ok(PhpMixed::List(vec![
                    Box::new(response_data),
                    Box::new(PhpMixed::String(packages_source)),
                ]))
            },
        )))
    }

    /// @param name package name (must be lowercased already)
    fn is_version_acceptable(
        &self,
        constraint: Option<&dyn ConstraintInterface>,
        name: &str,
        version_data: &IndexMap<String, PhpMixed>,
        acceptable_stabilities: Option<&IndexMap<String, i64>>,
        stability_flags: Option<&IndexMap<String, i64>>,
    ) -> anyhow::Result<bool> {
        Self::is_version_acceptable_with_loader(
            &self.loader,
            constraint,
            name,
            version_data,
            acceptable_stabilities,
            stability_flags,
        )
    }

    fn is_version_acceptable_static(
        constraint: Option<&dyn ConstraintInterface>,
        name: &str,
        version_data: &IndexMap<String, PhpMixed>,
        acceptable_stabilities: Option<&IndexMap<String, i64>>,
        stability_flags: Option<&IndexMap<String, i64>>,
    ) -> anyhow::Result<bool> {
        Self::is_version_acceptable_with_loader(
            &ArrayLoader::new_with_parser(VersionParser::new()),
            constraint,
            name,
            version_data,
            acceptable_stabilities,
            stability_flags,
        )
    }

    fn is_version_acceptable_with_loader(
        loader: &ArrayLoader,
        constraint: Option<&dyn ConstraintInterface>,
        name: &str,
        version_data: &IndexMap<String, PhpMixed>,
        acceptable_stabilities: Option<&IndexMap<String, i64>>,
        stability_flags: Option<&IndexMap<String, i64>>,
    ) -> anyhow::Result<bool> {
        let mut versions: Vec<String> = vec![
            version_data
                .get("version_normalized")
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string(),
        ];

        if let Some(alias) = loader.get_branch_alias(version_data) {
            versions.push(alias);
        }

        for version in versions.iter() {
            if acceptable_stabilities.is_some()
                && stability_flags.is_some()
                && !StabilityFilter::is_package_acceptable(
                    acceptable_stabilities.unwrap(),
                    stability_flags.unwrap(),
                    &[name.to_string()],
                    &VersionParser::parse_stability(version),
                )
            {
                continue;
            }

            if let Some(c) = constraint {
                if !CompilingMatcher::match_(c, Constraint::OP_EQ, version) {
                    continue;
                }
            }

            return Ok(true);
        }

        Ok(false)
    }

    fn get_packages_json_url(&self) -> String {
        let json_url_parts = parse_url_all(&strtr(&self.url, "\\", "/"));

        let has_json = json_url_parts
            .as_array()
            .and_then(|a| a.get("path"))
            .and_then(|v| v.as_string())
            .map_or(false, |p| p.contains(".json"));
        if has_json {
            return self.url.clone();
        }

        format!("{}/packages.json", self.url)
    }

    fn load_root_server_file(&mut self, root_max_age: Option<i64>) -> anyhow::Result<RootData> {
        if let Some(rd) = &self.root_data {
            return Ok(clone_root_data(rd));
        }

        if !extension_loaded("openssl") && self.url.starts_with("https") {
            return Err(RuntimeException {
                message: format!(
                    "You must enable the openssl extension in your php.ini to load information from {}",
                    self.url
                ),
                code: 0,
            }.into());
        }

        let mut data: Option<IndexMap<String, PhpMixed>> = None;
        if let Some(cached_raw) = self.cache.read("packages.json") {
            let cached_decoded = json_decode(&cached_raw, true)?;
            if let Some(arr) = cached_decoded.as_array() {
                let cached_data: IndexMap<String, PhpMixed> = arr
                    .iter()
                    .map(|(k, v)| (k.clone(), (**v).clone()))
                    .collect();
                let age = self.cache.get_age("packages.json");
                if root_max_age.is_some() && age.is_some() && age.unwrap() <= root_max_age.unwrap()
                {
                    data = Some(cached_data);
                } else if let Some(last_modified) = cached_data
                    .get("last-modified")
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_string())
                {
                    let response = self.fetch_file_if_last_modified(
                        &self.get_packages_json_url(),
                        "packages.json",
                        &last_modified,
                    )?;
                    data = Some(match response {
                        FetchFileIfLastModifiedResult::NotModified => cached_data,
                        FetchFileIfLastModifiedResult::Data(d) => d,
                    });
                }
            }
        }

        if data.is_none() {
            data = Some(self.fetch_file(
                &self.get_packages_json_url(),
                Some("packages.json"),
                None,
                true,
            )?);
        }

        let mut data = data.unwrap();

        if let Some(notify_batch) = data
            .get("notify-batch")
            .and_then(|v| v.as_string())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
        {
            self.notify_url = Some(self.canonicalize_url(&notify_batch)?);
        } else if let Some(notify) = data
            .get("notify")
            .and_then(|v| v.as_string())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
        {
            self.notify_url = Some(self.canonicalize_url(&notify)?);
        }

        if let Some(search) = data
            .get("search")
            .and_then(|v| v.as_string())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
        {
            self.search_url = Some(self.canonicalize_url(&search)?);
        }

        if let Some(mirrors) = data.get("mirrors").and_then(|v| v.as_list()).cloned() {
            for mirror_mixed in mirrors.iter() {
                let mirror = match mirror_mixed.as_array() {
                    Some(a) => a,
                    None => continue,
                };
                if let Some(git_url) = mirror
                    .get("git-url")
                    .and_then(|v| v.as_string())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                {
                    let preferred = mirror
                        .get("preferred")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    self.source_mirrors
                        .get_or_insert_with(IndexMap::new)
                        .entry("git".to_string())
                        .or_insert_with(Vec::new)
                        .push(SourceMirror {
                            url: git_url,
                            preferred,
                        });
                }
                if let Some(hg_url) = mirror
                    .get("hg-url")
                    .and_then(|v| v.as_string())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                {
                    let preferred = mirror
                        .get("preferred")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    self.source_mirrors
                        .get_or_insert_with(IndexMap::new)
                        .entry("hg".to_string())
                        .or_insert_with(Vec::new)
                        .push(SourceMirror {
                            url: hg_url,
                            preferred,
                        });
                }
                if let Some(dist_url) = mirror
                    .get("dist-url")
                    .and_then(|v| v.as_string())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                {
                    let preferred = mirror
                        .get("preferred")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    self.dist_mirrors
                        .get_or_insert_with(Vec::new)
                        .push(DistMirror {
                            url: self.canonicalize_url(&dist_url)?,
                            preferred,
                        });
                }
            }
        }

        if let Some(providers_lazy_url) = data
            .get("providers-lazy-url")
            .and_then(|v| v.as_string())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
        {
            self.lazy_providers_url = Some(self.canonicalize_url(&providers_lazy_url)?);
            self.has_providers = true;

            self.has_partial_packages = data
                .get("packages")
                .map(|v| match v {
                    PhpMixed::Array(a) => !a.is_empty(),
                    PhpMixed::List(l) => !l.is_empty(),
                    _ => false,
                })
                .unwrap_or(false);
        }

        // metadata-url indicates V2 repo protocol so it takes over from all the V1 types
        // V2 only has lazyProviders and possibly partial packages, but no ability to process anything else,
        // V2 also supports async loading
        if let Some(metadata_url) = data
            .get("metadata-url")
            .and_then(|v| v.as_string())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
        {
            self.lazy_providers_url = Some(self.canonicalize_url(&metadata_url)?);
            self.providers_url = None;
            self.has_providers = false;
            self.has_partial_packages = data
                .get("packages")
                .map(|v| match v {
                    PhpMixed::Array(a) => !a.is_empty(),
                    PhpMixed::List(l) => !l.is_empty(),
                    _ => false,
                })
                .unwrap_or(false);
            self.allow_ssl_downgrade = false;

            // provides a list of package names that are available in this repo
            // this disables lazy-provider behavior in the sense that if a list is available we assume it is finite and won't search for other packages in that repo
            // while if no list is there lazyProvidersUrl is used when looking for any package name to see if the repo knows it
            if let Some(available) = data
                .get("available-packages")
                .and_then(|v| v.as_list())
                .cloned()
            {
                if !available.is_empty() {
                    let avail_packages: Vec<String> = available
                        .iter()
                        .filter_map(|v| v.as_string().map(|s| strtolower(s)))
                        .collect();
                    let mut combined: IndexMap<String, String> = IndexMap::new();
                    for k in avail_packages.iter() {
                        combined.insert(k.clone(), k.clone());
                    }
                    self.available_packages = Some(combined);
                    self.has_available_package_list = true;
                }
            }

            // Provides a list of package name patterns (using * wildcards to match any substring, e.g. "vendor/*") that are available in this repo
            // Disables lazy-provider behavior as with available-packages, but may allow much more compact expression of packages covered by this repository.
            // Over-specifying covered packages is safe, but may result in increased traffic to your repository.
            if let Some(patterns) = data
                .get("available-package-patterns")
                .and_then(|v| v.as_list())
                .cloned()
            {
                if !patterns.is_empty() {
                    let mapped: Vec<String> = patterns
                        .iter()
                        .filter_map(|v| v.as_string())
                        .map(|p| BasePackage::package_name_to_regexp(p))
                        .collect();
                    self.available_package_patterns = Some(mapped);
                    self.has_available_package_list = true;
                }
            }

            // Remove legacy keys as most repos need to be compatible with Composer v1
            // as well but we are not interested in the old format anymore at this point
            data.shift_remove("providers-url");
            data.shift_remove("providers");
            data.shift_remove("providers-includes");

            if let Some(sec) = data
                .get("security-advisories")
                .and_then(|v| v.as_array())
                .cloned()
            {
                let metadata = sec
                    .get("metadata")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let api_url_str = sec
                    .get("api-url")
                    .and_then(|v| v.as_string())
                    .filter(|s| !s.is_empty());
                let api_url = if let Some(s) = api_url_str {
                    Some(self.canonicalize_url(s)?)
                } else {
                    None
                };
                self.security_advisory_config = Some(SecurityAdvisoryConfig {
                    metadata,
                    api_url: api_url.clone(),
                });
                if api_url.is_none() && !self.has_available_package_list {
                    return Err(UnexpectedValueException {
                        message: format!(
                            "Invalid security advisory configuration on {}: If the repository does not provide a security-advisories.api-url then available-packages or available-package-patterns are required to be provided for performance reason.",
                            self.get_repo_name()
                        ),
                        code: 0,
                    }.into());
                }
            }
        }

        if self.allow_ssl_downgrade {
            self.url = self.url.replace("https://", "http://");
            self.base_url = self.base_url.replace("https://", "http://");
        }

        if let Some(providers_url) = data
            .get("providers-url")
            .and_then(|v| v.as_string())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
        {
            self.providers_url = Some(self.canonicalize_url(&providers_url)?);
            self.has_providers = true;
        }

        if let Some(list) = data
            .get("list")
            .and_then(|v| v.as_string())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
        {
            self.list_url = Some(self.canonicalize_url(&list)?);
        }

        let providers_non_empty = data
            .get("providers")
            .map(|v| match v {
                PhpMixed::Array(a) => !a.is_empty(),
                PhpMixed::List(l) => !l.is_empty(),
                _ => false,
            })
            .unwrap_or(false);
        let providers_includes_non_empty = data
            .get("providers-includes")
            .map(|v| match v {
                PhpMixed::Array(a) => !a.is_empty(),
                PhpMixed::List(l) => !l.is_empty(),
                _ => false,
            })
            .unwrap_or(false);
        if providers_non_empty || providers_includes_non_empty {
            self.has_providers = true;
        }

        if let Some(providers_api) = data
            .get("providers-api")
            .and_then(|v| v.as_string())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
        {
            self.providers_api_url = Some(self.canonicalize_url(&providers_api)?);
        }

        self.root_data = Some(RootData::Data(data.clone()));
        Ok(RootData::Data(data))
    }

    fn canonicalize_url(&self, url: &str) -> anyhow::Result<String> {
        if url.is_empty() {
            return Err(InvalidArgumentException {
                message: "Expected a string with a value and not an empty string".to_string(),
                code: 0,
            }
            .into());
        }

        if url.starts_with('/') {
            let mut matches: Vec<String> = Vec::new();
            if Preg::is_match_with_matches(r"{^[^:]++://[^/]*+}", &self.url, &mut matches)? {
                return Ok(format!(
                    "{}{}",
                    matches.get(0).cloned().unwrap_or_default(),
                    url
                ));
            }

            return Ok(self.url.clone());
        }

        Ok(url.to_string())
    }

    fn load_data_from_server(&mut self) -> anyhow::Result<Vec<IndexMap<String, PhpMixed>>> {
        let data = self.load_root_server_file(None)?;
        let data = match data {
            RootData::True => {
                return Err(LogicException {
                    message: "loadRootServerFile should not return true during initialization"
                        .to_string(),
                    code: 0,
                }
                .into());
            }
            RootData::Data(d) => d,
        };

        self.load_includes(&data)
    }

    fn has_partial_packages(&mut self) -> anyhow::Result<bool> {
        if self.has_partial_packages && self.partial_packages_by_name.is_none() {
            self.initialize_partial_packages()?;
        }

        Ok(self.has_partial_packages)
    }

    fn load_provider_listings(&mut self, data: &IndexMap<String, PhpMixed>) -> anyhow::Result<()> {
        if let Some(providers) = data.get("providers").and_then(|v| v.as_array()) {
            if self.provider_listing.is_none() {
                self.provider_listing = Some(IndexMap::new());
            }
            let listing = self.provider_listing.as_mut().unwrap();
            for (k, v) in providers.iter() {
                if let Some(arr) = v.as_array() {
                    if let Some(sha256) = arr.get("sha256").and_then(|v| v.as_string()) {
                        listing.insert(
                            k.clone(),
                            ProviderListingEntry {
                                sha256: sha256.to_string(),
                            },
                        );
                    }
                }
            }
        }

        if self.providers_url.is_some() {
            if let Some(includes) = data
                .get("provider-includes")
                .and_then(|v| v.as_array())
                .cloned()
            {
                for (include, metadata_mixed) in includes.iter() {
                    let metadata = match metadata_mixed.as_array() {
                        Some(a) => a,
                        None => continue,
                    };
                    let sha256 = metadata
                        .get("sha256")
                        .and_then(|v| v.as_string())
                        .unwrap_or("")
                        .to_string();
                    let url = format!("{}/{}", self.base_url, include.replace("%hash%", &sha256));
                    let cache_key = include.replace("%hash%", "").replace("$", "");
                    let included_data: IndexMap<String, PhpMixed> =
                        if self.cache.sha256(&cache_key).as_deref() == Some(sha256.as_str()) {
                            let raw = self.cache.read(&cache_key).unwrap_or_default();
                            let decoded = json_decode(&raw, true)?;
                            decoded
                                .as_array()
                                .map(|a| {
                                    a.iter().map(|(k, v)| (k.clone(), (**v).clone())).collect()
                                })
                                .unwrap_or_default()
                        } else {
                            self.fetch_file(&url, Some(&cache_key), Some(&sha256), false)?
                        };

                    self.load_provider_listings(&included_data)?;
                }
            }
        }
        Ok(())
    }

    fn load_includes(
        &mut self,
        data: &IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<Vec<IndexMap<String, PhpMixed>>> {
        let mut packages: Vec<IndexMap<String, PhpMixed>> = Vec::new();

        // legacy repo handling
        if !data.contains_key("packages") && !data.contains_key("includes") {
            for (_k, pkg_mixed) in data.iter() {
                let pkg = match pkg_mixed.as_array() {
                    Some(a) => a,
                    None => continue,
                };
                if let Some(versions) = pkg.get("versions").and_then(|v| v.as_array()) {
                    for (_, metadata) in versions.iter() {
                        if let Some(m) = metadata.as_array() {
                            packages
                                .push(m.iter().map(|(k, v)| (k.clone(), (**v).clone())).collect());
                        }
                    }
                }
            }

            return Ok(packages);
        }

        if let Some(pkgs) = data.get("packages").and_then(|v| v.as_array()).cloned() {
            for (package, versions_mixed) in pkgs.iter() {
                let package_name = strtolower(package);
                let versions = match versions_mixed.as_array() {
                    Some(a) => a.clone(),
                    None => continue,
                };
                for (_version, metadata_mixed) in versions.iter() {
                    let metadata = match metadata_mixed.as_array() {
                        Some(a) => a.clone(),
                        None => continue,
                    };
                    let metadata_map: IndexMap<String, PhpMixed> = metadata
                        .iter()
                        .map(|(k, v)| (k.clone(), (**v).clone()))
                        .collect();
                    packages.push(metadata_map.clone());
                    let meta_name = metadata_map
                        .get("name")
                        .and_then(|v| v.as_string())
                        .unwrap_or("")
                        .to_string();
                    if !self.displayed_warning_about_non_matching_package_index
                        && package_name != strtolower(&meta_name)
                    {
                        self.displayed_warning_about_non_matching_package_index = true;
                        self.io.write_error(&format!(
                            "<warning>Warning: the packages key '{}' doesn't match the name defined in the package metadata '{}' in repository {}</warning>",
                            package, meta_name, self.base_url
                        ));
                    }
                }
            }
        }

        if let Some(includes) = data.get("includes").and_then(|v| v.as_array()).cloned() {
            for (include, metadata_mixed) in includes.iter() {
                let metadata = match metadata_mixed.as_array() {
                    Some(a) => a,
                    None => continue,
                };
                let sha1 = metadata
                    .get("sha1")
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_string());
                let included_data: IndexMap<String, PhpMixed> = if let Some(ref sha1) = sha1 {
                    if self.cache.sha1(include).as_deref() == Some(sha1.as_str()) {
                        let raw = self.cache.read(include).unwrap_or_default();
                        let decoded = json_decode(&raw, true)?;
                        decoded
                            .as_array()
                            .map(|a| a.iter().map(|(k, v)| (k.clone(), (**v).clone())).collect())
                            .unwrap_or_default()
                    } else {
                        self.fetch_file(include, None, None, false)?
                    }
                } else {
                    self.fetch_file(include, None, None, false)?
                };
                let included_packages = self.load_includes(&included_data)?;
                for p in included_packages.into_iter() {
                    packages.push(p);
                }
            }
        }

        Ok(packages)
    }

    fn create_packages_flat(
        &mut self,
        packages: Vec<IndexMap<String, PhpMixed>>,
        source: Option<String>,
    ) -> anyhow::Result<Vec<Box<dyn BasePackage>>> {
        if packages.is_empty() {
            return Ok(vec![]);
        }

        let mut packages = packages;
        let result = (|| -> anyhow::Result<Vec<Box<dyn BasePackage>>> {
            for data in packages.iter_mut() {
                if !data.contains_key("notification-url") {
                    data.insert(
                        "notification-url".to_string(),
                        match &self.notify_url {
                            Some(s) => PhpMixed::String(s.clone()),
                            None => PhpMixed::Null,
                        },
                    );
                }
            }

            let package_instances = self.loader.load_packages(packages.clone())?;

            let mut results: Vec<Box<dyn BasePackage>> = Vec::new();
            for mut package in package_instances.into_iter() {
                if let Some(src_type) = package.get_source_type() {
                    if let Some(mirrors) =
                        self.source_mirrors.as_ref().and_then(|m| m.get(src_type))
                    {
                        package.set_source_mirrors(mirrors);
                    }
                }
                if let Some(dist_mirrors) = self.dist_mirrors.as_ref() {
                    package.set_dist_mirrors(dist_mirrors);
                }
                self.configure_package_transport_options(&mut *package);
                results.push(package);
            }
            Ok(results)
        })();

        result.map_err(|e| {
            RuntimeException {
                message: format!(
                    "Could not load packages in {}{}: [{}] {}",
                    self.get_repo_name(),
                    source
                        .as_ref()
                        .map(|s| format!(" from {}", s))
                        .unwrap_or_default(),
                    "Exception",
                    e.to_string()
                ),
                code: 0,
            }
            .into()
        })
    }

    fn create_packages_static(
        packages: Vec<IndexMap<String, PhpMixed>>,
        _source: Option<String>,
    ) -> anyhow::Result<Vec<Box<dyn BasePackage>>> {
        if packages.is_empty() {
            return Ok(vec![]);
        }
        let loader = ArrayLoader::new_with_parser(VersionParser::new());
        Ok(loader.load_packages(packages)?)
    }

    fn fetch_file(
        &mut self,
        filename: &str,
        cache_key: Option<&str>,
        sha256: Option<&str>,
        store_last_modified_time: bool,
    ) -> anyhow::Result<IndexMap<String, PhpMixed>> {
        if filename.is_empty() {
            return Err(InvalidArgumentException {
                message: "$filename should not be an empty string".to_string(),
                code: 0,
            }
            .into());
        }

        let (mut filename, cache_key_owned): (String, Option<String>) = match cache_key {
            None => {
                let ck = filename.to_string();
                let fn_full = format!("{}/{}", self.base_url, filename);
                (fn_full, Some(ck))
            }
            Some(ck) => (filename.to_string(), Some(ck.to_string())),
        };

        // url-encode $ signs in URLs as bad proxies choke on them
        if let Some(pos) = filename.find('$') {
            if pos > 0 && Preg::is_match(r"{^https?://}i", &filename)? {
                filename = format!("{}%24{}", &filename[..pos], &filename[pos + 1..]);
            }
        }

        let mut retries: i64 = 3;
        let mut data: Option<IndexMap<String, PhpMixed>> = None;
        while {
            let cont = retries > 0;
            retries -= 1;
            cont
        } {
            let attempt: anyhow::Result<()> = (|| -> anyhow::Result<()> {
                let mut options = self.options.clone();
                if let Some(dispatcher) = self.event_dispatcher.as_mut() {
                    let mut pre_file_download_event = PreFileDownloadEvent::new(
                        PluginEvents::PRE_FILE_DOWNLOAD.to_string(),
                        &self.http_downloader,
                        filename.clone(),
                        "metadata".to_string(),
                        {
                            let mut m: IndexMap<String, PhpMixed> = IndexMap::new();
                            // TODO(plugin): pass repository self-reference
                            m.insert("repository".to_string(), PhpMixed::Null);
                            m
                        },
                    );
                    pre_file_download_event.set_transport_options(self.options.clone());
                    dispatcher.dispatch(
                        &pre_file_download_event.get_name(),
                        &mut pre_file_download_event,
                    );
                    filename = pre_file_download_event.get_processed_url();
                    options = pre_file_download_event.get_transport_options();
                }

                let response = self.http_downloader.get(&filename, &options)?;
                let mut json = response.get_body().to_string();
                if let Some(sha256_val) = sha256 {
                    if sha256_val != hash("sha256", &json) {
                        // undo downgrade before trying again if http seems to be hijacked or modifying content somehow
                        if self.allow_ssl_downgrade {
                            self.url = self.url.replace("http://", "https://");
                            self.base_url = self.base_url.replace("http://", "https://");
                            filename = filename.replace("http://", "https://");
                        }

                        if retries > 0 {
                            std::thread::sleep(std::time::Duration::from_micros(100000));
                            return Err(RetryMarker.into());
                        }

                        // TODO use scarier wording once we know for sure it doesn't do false positives anymore
                        return Err(RepositorySecurityException(shirabe_php_shim::Exception {
                            message: format!(
                                "The contents of {} do not match its signature. This could indicate a man-in-the-middle attack or e.g. antivirus software corrupting files. Try running composer again and report this if you think it is a mistake.",
                                filename
                            ),
                            code: 0,
                        }).into());
                    }
                }

                if let Some(dispatcher) = self.event_dispatcher.as_mut() {
                    let mut post_file_download_event = PostFileDownloadEvent::new(
                        PluginEvents::POST_FILE_DOWNLOAD.to_string(),
                        None,
                        sha256.map(|s| s.to_string()),
                        filename.clone(),
                        "metadata".to_string(),
                        {
                            let mut m: IndexMap<String, PhpMixed> = IndexMap::new();
                            // TODO(plugin): pass response and repository self-reference
                            m.insert("response".to_string(), PhpMixed::Null);
                            m.insert("repository".to_string(), PhpMixed::Null);
                            m
                        },
                    );
                    dispatcher.dispatch(
                        &post_file_download_event.get_name(),
                        &mut post_file_download_event,
                    );
                }

                let decoded = response.decode_json()?;
                let mut data_local: IndexMap<String, PhpMixed> = decoded
                    .as_array()
                    .map(|a| a.iter().map(|(k, v)| (k.clone(), (**v).clone())).collect())
                    .unwrap_or_default();
                HttpDownloader::output_warnings(&*self.io, &self.url, &data_local);

                if let Some(ck) = cache_key_owned.as_ref() {
                    if !ck.is_empty() && !self.cache.is_read_only() {
                        if store_last_modified_time {
                            if let Some(last_modified_date) = response.get_header("last-modified") {
                                data_local.insert(
                                    "last-modified".to_string(),
                                    PhpMixed::String(last_modified_date),
                                );
                                let as_mixed = PhpMixed::Array(
                                    data_local
                                        .iter()
                                        .map(|(k, v)| (k.clone(), Box::new(v.clone())))
                                        .collect(),
                                );
                                json = JsonFile::encode(&as_mixed, 0)?;
                            }
                        }
                        self.cache.write(ck, &json);
                    }
                }

                response.collect();

                data = Some(data_local);
                Ok(())
            })();

            match attempt {
                Ok(()) => break,
                Err(e) => {
                    if e.downcast_ref::<RetryMarker>().is_some() {
                        continue;
                    }
                    if e.downcast_ref::<LogicException>().is_some() {
                        return Err(e);
                    }
                    if let Some(te) = e.downcast_ref::<TransportException>() {
                        if te.get_status_code() == 404 {
                            return Err(e);
                        }
                    }
                    if e.downcast_ref::<RepositorySecurityException>().is_some() {
                        return Err(e);
                    }

                    if let Some(ck) = cache_key_owned.as_ref() {
                        if !ck.is_empty() {
                            if let Some(contents) = self.cache.read(ck) {
                                if !self.degraded_mode {
                                    self.io.write_error(&format!(
                                        "<warning>{} could not be fully loaded ({}), package information was loaded from the local cache and may be out of date</warning>",
                                        self.url,
                                        e.to_string()
                                    ));
                                }
                                self.degraded_mode = true;
                                let parsed = JsonFile::parse_json(
                                    &contents,
                                    Some(&format!("{}{}", self.cache.get_root(), ck)),
                                )?;
                                let map: IndexMap<String, PhpMixed> = parsed
                                    .as_array()
                                    .map(|a| {
                                        a.iter().map(|(k, v)| (k.clone(), (**v).clone())).collect()
                                    })
                                    .unwrap_or_default();
                                data = Some(map);

                                break;
                            }
                        }
                    }

                    return Err(e);
                }
            }
        }

        match data {
            Some(d) => Ok(d),
            None => Err(LogicException {
                message: "ComposerRepository: Undefined $data. Please report at https://github.com/composer/composer/issues/new.".to_string(),
                code: 0,
            }.into()),
        }
    }

    fn fetch_file_if_last_modified(
        &mut self,
        filename: &str,
        cache_key: &str,
        last_modified_time: &str,
    ) -> anyhow::Result<FetchFileIfLastModifiedResult> {
        if filename.is_empty() {
            return Err(InvalidArgumentException {
                message: "$filename should not be an empty string".to_string(),
                code: 0,
            }
            .into());
        }

        let mut filename = filename.to_string();
        let result: anyhow::Result<FetchFileIfLastModifiedResult> = (|| {
            let mut options = self.options.clone();
            if let Some(dispatcher) = self.event_dispatcher.as_mut() {
                let mut pre_file_download_event = PreFileDownloadEvent::new(
                    PluginEvents::PRE_FILE_DOWNLOAD.to_string(),
                    &self.http_downloader,
                    filename.clone(),
                    "metadata".to_string(),
                    {
                        let mut m: IndexMap<String, PhpMixed> = IndexMap::new();
                        m.insert("repository".to_string(), PhpMixed::Null);
                        m
                    },
                );
                pre_file_download_event.set_transport_options(self.options.clone());
                dispatcher.dispatch(
                    &pre_file_download_event.get_name(),
                    &mut pre_file_download_event,
                );
                filename = pre_file_download_event.get_processed_url();
                options = pre_file_download_event.get_transport_options();
            }

            // cast http.header to array, then append
            let http_entry = options
                .entry("http".to_string())
                .or_insert(PhpMixed::Array(IndexMap::new()));
            if let PhpMixed::Array(ref mut http_map) = http_entry {
                if let Some(existing) = http_map.get("header") {
                    let arr = match &**existing {
                        PhpMixed::List(l) => l.clone(),
                        other => vec![Box::new(other.clone())],
                    };
                    http_map.insert("header".to_string(), Box::new(PhpMixed::List(arr)));
                }
                let mut headers = match http_map.get("header") {
                    Some(b) => match &**b {
                        PhpMixed::List(l) => l.clone(),
                        _ => vec![],
                    },
                    None => vec![],
                };
                headers.push(Box::new(PhpMixed::String(format!(
                    "If-Modified-Since: {}",
                    last_modified_time
                ))));
                http_map.insert("header".to_string(), Box::new(PhpMixed::List(headers)));
            }

            let response = self.http_downloader.get(&filename, &options)?;
            let mut json = response.get_body().to_string();
            if json.is_empty() && response.get_status_code() == 304 {
                return Ok(FetchFileIfLastModifiedResult::NotModified);
            }

            if let Some(dispatcher) = self.event_dispatcher.as_mut() {
                let mut post_file_download_event = PostFileDownloadEvent::new(
                    PluginEvents::POST_FILE_DOWNLOAD.to_string(),
                    None,
                    None,
                    filename.clone(),
                    "metadata".to_string(),
                    {
                        let mut m: IndexMap<String, PhpMixed> = IndexMap::new();
                        m.insert("response".to_string(), PhpMixed::Null);
                        m.insert("repository".to_string(), PhpMixed::Null);
                        m
                    },
                );
                dispatcher.dispatch(
                    &post_file_download_event.get_name(),
                    &mut post_file_download_event,
                );
            }

            let decoded = response.decode_json()?;
            let mut data: IndexMap<String, PhpMixed> = decoded
                .as_array()
                .map(|a| a.iter().map(|(k, v)| (k.clone(), (**v).clone())).collect())
                .unwrap_or_default();
            HttpDownloader::output_warnings(&*self.io, &self.url, &data);

            let last_modified_date = response.get_header("last-modified");
            response.collect();
            if let Some(ref lmd) = last_modified_date {
                data.insert("last-modified".to_string(), PhpMixed::String(lmd.clone()));
                let as_mixed = PhpMixed::Array(
                    data.iter()
                        .map(|(k, v)| (k.clone(), Box::new(v.clone())))
                        .collect(),
                );
                json = JsonFile::encode(&as_mixed, 0)?;
            }
            if !self.cache.is_read_only() {
                self.cache.write(cache_key, &json);
            }

            Ok(FetchFileIfLastModifiedResult::Data(data))
        })();

        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                if e.downcast_ref::<LogicException>().is_some() {
                    return Err(e);
                }
                if let Some(te) = e.downcast_ref::<TransportException>() {
                    if te.get_status_code() == 404 {
                        return Err(e);
                    }
                }

                if !self.degraded_mode {
                    self.io.write_error(&format!(
                        "<warning>{} could not be fully loaded ({}), package information was loaded from the local cache and may be out of date</warning>",
                        self.url,
                        e.to_string()
                    ));
                }
                self.degraded_mode = true;

                Ok(FetchFileIfLastModifiedResult::NotModified)
            }
        }
    }

    fn async_fetch_file(
        &mut self,
        filename: &str,
        cache_key: &str,
        last_modified_time: Option<&str>,
    ) -> anyhow::Result<Box<dyn PromiseInterface>> {
        if filename.is_empty() {
            return Err(InvalidArgumentException {
                message: "$filename should not be an empty string".to_string(),
                code: 0,
            }
            .into());
        }

        if self.packagesNotFoundCache.contains_key(filename) {
            let mut empty: IndexMap<String, PhpMixed> = IndexMap::new();
            empty.insert("packages".to_string(), PhpMixed::Array(IndexMap::new()));
            return Ok(react_promise_resolve(PhpMixed::Array(
                empty.into_iter().map(|(k, v)| (k, Box::new(v))).collect(),
            )));
        }

        if self.freshMetadataUrls.contains_key(filename) && last_modified_time.is_some() {
            // make it look like we got a 304 response
            let promise = react_promise_resolve(PhpMixed::Bool(true));

            return Ok(promise);
        }

        let mut filename = filename.to_string();
        let mut options = self.options.clone();
        if let Some(dispatcher) = self.event_dispatcher.as_mut() {
            let mut pre_file_download_event = PreFileDownloadEvent::new(
                PluginEvents::PRE_FILE_DOWNLOAD.to_string(),
                &self.http_downloader,
                filename.clone(),
                "metadata".to_string(),
                {
                    let mut m: IndexMap<String, PhpMixed> = IndexMap::new();
                    m.insert("repository".to_string(), PhpMixed::Null);
                    m
                },
            );
            pre_file_download_event.set_transport_options(self.options.clone());
            dispatcher.dispatch(
                &pre_file_download_event.get_name(),
                &mut pre_file_download_event,
            );
            filename = pre_file_download_event.get_processed_url();
            options = pre_file_download_event.get_transport_options();
        }

        if let Some(last_modified_time) = last_modified_time {
            let http_entry = options
                .entry("http".to_string())
                .or_insert(PhpMixed::Array(IndexMap::new()));
            if let PhpMixed::Array(ref mut http_map) = http_entry {
                if let Some(existing) = http_map.get("header") {
                    let arr = match &**existing {
                        PhpMixed::List(l) => l.clone(),
                        other => vec![Box::new(other.clone())],
                    };
                    http_map.insert("header".to_string(), Box::new(PhpMixed::List(arr)));
                }
                let mut headers = match http_map.get("header") {
                    Some(b) => match &**b {
                        PhpMixed::List(l) => l.clone(),
                        _ => vec![],
                    },
                    None => vec![],
                };
                headers.push(Box::new(PhpMixed::String(format!(
                    "If-Modified-Since: {}",
                    last_modified_time
                ))));
                http_map.insert("header".to_string(), Box::new(PhpMixed::List(headers)));
            }
        }

        let filename_for_closures = filename.clone();
        let cache_key_owned = cache_key.to_string();
        let url_owned = self.url.clone();
        let last_modified_time_owned = last_modified_time.map(|s| s.to_string());

        let packages_not_found_ptr = &mut self.packagesNotFoundCache as *mut _;
        let fresh_metadata_ptr = &mut self.freshMetadataUrls as *mut _;
        let degraded_ptr = &mut self.degraded_mode as *mut _;
        let cache_ptr = &mut self.cache as *mut _;
        let io_ptr = self.io.as_ref() as *const dyn IOInterface;

        let accept = {
            let filename = filename_for_closures.clone();
            let cache_key_owned = cache_key_owned.clone();
            let url_owned = url_owned.clone();
            move |response_mixed: PhpMixed| -> anyhow::Result<PhpMixed> {
                // emulate: $response is a Response object; status code/body/header accessed via methods
                let response = Response::from_php_mixed(response_mixed)?;
                // package not found is acceptable for a v2 protocol repository
                if response.get_status_code() == 404 {
                    unsafe {
                        (*packages_not_found_ptr).insert(filename.clone(), true);
                    }

                    let mut empty: IndexMap<String, PhpMixed> = IndexMap::new();
                    empty.insert("packages".to_string(), PhpMixed::Array(IndexMap::new()));
                    return Ok(PhpMixed::Array(
                        empty.into_iter().map(|(k, v)| (k, Box::new(v))).collect(),
                    ));
                }

                let mut json = response.get_body().to_string();
                if json.is_empty() && response.get_status_code() == 304 {
                    unsafe {
                        (*fresh_metadata_ptr).insert(filename.clone(), true);
                    }

                    return Ok(PhpMixed::Bool(true));
                }

                // TODO(plugin): dispatch PostFileDownloadEvent

                let decoded = response.decode_json()?;
                let mut data: IndexMap<String, PhpMixed> = decoded
                    .as_array()
                    .map(|a| a.iter().map(|(k, v)| (k.clone(), (**v).clone())).collect())
                    .unwrap_or_default();
                let io_ref = unsafe { &*io_ptr };
                HttpDownloader::output_warnings(io_ref, &url_owned, &data);

                let last_modified_date = response.get_header("last-modified");
                response.collect();
                if let Some(lmd) = last_modified_date {
                    data.insert("last-modified".to_string(), PhpMixed::String(lmd));
                    let as_mixed = PhpMixed::Array(
                        data.iter()
                            .map(|(k, v)| (k.clone(), Box::new(v.clone())))
                            .collect(),
                    );
                    json = JsonFile::encode(
                        &as_mixed,
                        JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE,
                    )?;
                }
                let is_ro = unsafe { (*cache_ptr).is_read_only() };
                if !is_ro {
                    unsafe {
                        (*cache_ptr).write(&cache_key_owned, &json);
                    }
                }
                unsafe {
                    (*fresh_metadata_ptr).insert(filename.clone(), true);
                }

                Ok(PhpMixed::Array(
                    data.into_iter().map(|(k, v)| (k, Box::new(v))).collect(),
                ))
            }
        };

        let reject = {
            let filename = filename_for_closures.clone();
            let url_owned = url_owned.clone();
            let last_modified_time = last_modified_time_owned.clone();
            let accept_clone = accept.clone();
            move |e: anyhow::Error| -> anyhow::Result<PhpMixed> {
                if let Some(te) = e.downcast_ref::<TransportException>() {
                    if te.get_status_code() == 404 {
                        unsafe {
                            (*packages_not_found_ptr).insert(filename.clone(), true);
                        }

                        return Ok(PhpMixed::Bool(false));
                    }
                }

                let is_degraded = unsafe { *degraded_ptr };
                if !is_degraded {
                    let io_ref = unsafe { &*io_ptr };
                    io_ref.write_error(&format!(
                        "<warning>{} could not be fully loaded ({}), package information was loaded from the local cache and may be out of date</warning>",
                        url_owned,
                        e.to_string()
                    ));
                }
                unsafe {
                    *degraded_ptr = true;
                }

                // if the file is in the cache, we fake a 304 Not Modified to allow the process to continue
                if last_modified_time.is_some() {
                    let resp = Response::new_fake(&url_owned, 304, IndexMap::new(), String::new());
                    return accept_clone(resp.to_php_mixed());
                }

                // special error code returned when network is being artificially disabled
                if let Some(te) = e.downcast_ref::<TransportException>() {
                    if te.get_status_code() == 499 {
                        let resp =
                            Response::new_fake(&url_owned, 404, IndexMap::new(), String::new());
                        return accept_clone(resp.to_php_mixed());
                    }
                }

                Err(e)
            }
        };

        let initial = self.http_downloader.add(&filename, &options)?;
        Ok(initial.then_with_reject_boxed(Box::new(accept), Box::new(reject)))
    }

    /// This initializes the packages key of a partial packages.json that contain some packages inlined + a providers-lazy-url
    ///
    /// This should only be called once
    fn initialize_partial_packages(&mut self) -> anyhow::Result<()> {
        let root_data = self.load_root_server_file(None)?;
        let root_data = match root_data {
            RootData::True => return Ok(()),
            RootData::Data(d) => d,
        };

        self.partial_packages_by_name = Some(IndexMap::new());
        if let Some(packages) = root_data
            .get("packages")
            .and_then(|v| v.as_array())
            .cloned()
        {
            for (package, versions_mixed) in packages.iter() {
                let versions = match versions_mixed.as_array() {
                    Some(a) => a.clone(),
                    None => continue,
                };
                for (_v_key, version_mixed) in versions.iter() {
                    let version = match version_mixed.as_array() {
                        Some(a) => a,
                        None => continue,
                    };
                    let name_str = version
                        .get("name")
                        .and_then(|v| v.as_string())
                        .unwrap_or("")
                        .to_string();
                    let version_package_name = strtolower(&name_str);
                    let version_map: IndexMap<String, PhpMixed> = version
                        .iter()
                        .map(|(k, v)| (k.clone(), (**v).clone()))
                        .collect();
                    self.partial_packages_by_name
                        .as_mut()
                        .unwrap()
                        .entry(version_package_name.clone())
                        .or_insert_with(Vec::new)
                        .push(version_map);
                    if !self.displayed_warning_about_non_matching_package_index
                        && version_package_name != strtolower(package)
                    {
                        self.io.write_error(&format!(
                            "<warning>Warning: the packages key '{}' doesn't match the name defined in the package metadata '{}' in repository {}</warning>",
                            package, name_str, self.base_url
                        ));
                        self.displayed_warning_about_non_matching_package_index = true;
                    }
                }
            }
        }

        // wipe rootData as it is fully consumed at this point and this saves some memory
        self.root_data = Some(RootData::True);
        Ok(())
    }

    /// Checks if the package name is present in this lazy providers repo
    ///
    /// @return true if the package name is present in availablePackages or matched by availablePackagePatterns
    pub(crate) fn lazy_providers_repo_contains(&self, name: &str) -> anyhow::Result<bool> {
        if !self.has_available_package_list {
            return Err(LogicException {
                message: "lazyProvidersRepoContains should not be called unless hasAvailablePackageList is true".to_string(),
                code: 0,
            }.into());
        }

        if let Some(ref available) = self.available_packages {
            if available.contains_key(name) {
                return Ok(true);
            }
        }

        if let Some(ref patterns) = self.available_package_patterns {
            for provider_regex in patterns.iter() {
                if Preg::is_match(provider_regex, name)? {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }
}

pub const SEARCH_FULLTEXT: i64 = 0;
pub const SEARCH_VENDOR: i64 = 2;

#[derive(Debug)]
enum FetchFileIfLastModifiedResult {
    NotModified,
    Data(IndexMap<String, PhpMixed>),
}

#[derive(Debug)]
struct RetryMarker;

impl std::fmt::Display for RetryMarker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RetryMarker")
    }
}

impl std::error::Error for RetryMarker {}

fn clone_root_data(rd: &RootData) -> RootData {
    match rd {
        RootData::True => RootData::True,
        RootData::Data(d) => RootData::Data(d.clone()),
    }
}

fn dyn_clone_box(_pkg: &dyn BasePackage) -> Box<dyn BasePackage> {
    todo!()
}

fn dyn_clone_constraint(_c: &dyn ConstraintInterface) -> Box<dyn ConstraintInterface> {
    todo!()
}

fn react_promise_resolve(_value: PhpMixed) -> Box<dyn PromiseInterface> {
    todo!()
}
