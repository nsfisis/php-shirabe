//! ref: composer/src/Composer/Repository/PathRepository.php

use crate::config::Config;
use crate::event_dispatcher::EventDispatcher;
use crate::io::IOInterface;
use crate::json::JsonFile;
use crate::package::BasePackageHandle;
use crate::package::PackageInterfaceHandle;
use crate::package::loader::ArrayLoader;
use crate::package::loader::LoaderInterface;
use crate::package::version::VersionGuesser;
use crate::package::version::VersionParser;
use crate::repository::ArrayRepository;
use crate::repository::ConfigurableRepositoryInterface;
use crate::repository::RepositoryInterfaceWeakHandle;
use crate::repository::{
    FindPackageConstraint, LoadPackagesResult, ProviderInfo, RepositoryInterface, SearchResult,
};
use crate::util::Filesystem;
use crate::util::Git as GitUtil;
use crate::util::HttpDownloader;
use crate::util::Platform;
use crate::util::ProcessExecutor;
use crate::util::Url;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_php_shim::{
    DIRECTORY_SEPARATOR, GLOB_BRACE, GLOB_MARK, GLOB_ONLYDIR, PhpMixed, RuntimeException, defined,
    file_exists, file_get_contents, glob_with_flags, hash, php_regex, realpath, serialize,
};

#[derive(Debug)]
pub struct PathRepository {
    inner: ArrayRepository,
    loader: ArrayLoader,
    version_guesser: std::cell::RefCell<VersionGuesser>,
    url: String,
    repo_config: IndexMap<String, PhpMixed>,
    process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    options: IndexMap<String, PhpMixed>,
}

impl ConfigurableRepositoryInterface for PathRepository {
    fn get_repo_config(&self) -> IndexMap<String, PhpMixed> {
        self.repo_config.clone()
    }
}

impl PathRepository {
    pub fn new(
        repo_config: IndexMap<String, PhpMixed>,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        http_downloader: Option<std::rc::Rc<std::cell::RefCell<HttpDownloader>>>,
        dispatcher: Option<std::rc::Rc<std::cell::RefCell<EventDispatcher>>>,
        process: Option<std::rc::Rc<std::cell::RefCell<ProcessExecutor>>>,
    ) -> anyhow::Result<Self> {
        if !repo_config.contains_key("url") {
            return Err(RuntimeException {
                message: "You must specify the `url` configuration for the path repository"
                    .to_string(),
                code: 0,
            }
            .into());
        }

        let url_str = repo_config
            .get("url")
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .to_string();
        let url = Platform::expand_path(&url_str);
        let process = process.unwrap_or_else(|| {
            std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
                io.clone(),
            ))))
        });
        let version_guesser = VersionGuesser::new(
            config,
            process.clone(),
            VersionParser::new(),
            Some(io.clone()),
        );
        let mut options = repo_config
            .get("options")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect::<IndexMap<String, PhpMixed>>();
        if !options.contains_key("relative") {
            let filesystem = Filesystem::new(None);
            let is_relative = !filesystem.is_absolute_path(&url);
            options.insert("relative".to_string(), PhpMixed::Bool(is_relative));
        }

        Ok(Self {
            inner: ArrayRepository::new(vec![])?,
            loader: ArrayLoader::new(None, true),
            version_guesser: std::cell::RefCell::new(version_guesser),
            url,
            repo_config,
            process,
            options,
        })
    }

    pub fn get_repo_name(&self) -> String {
        format!(
            "path repo ({})",
            Url::sanitize(
                self.repo_config
                    .get("url")
                    .and_then(|v| v.as_string())
                    .unwrap_or("")
                    .to_string()
            )
        )
    }

    /// For testing only: drives `initialize` (globbing the configured url and loading each
    /// path package) and returns the packages collected by the inner `ArrayRepository`,
    /// mirroring the polymorphic `RepositoryInterface::getPackages` dispatch in PHP.
    pub fn __get_packages(&mut self) -> anyhow::Result<Vec<crate::package::BasePackageHandle>> {
        self.initialize()?;
        use crate::repository::RepositoryInterface;
        self.inner.get_packages()
    }

    /// For testing only: returns `RepositoryInterface::count` after `initialize`.
    pub fn __count(&mut self) -> anyhow::Result<usize> {
        self.initialize()?;
        use crate::repository::RepositoryInterface;
        self.inner.count()
    }

    /// For testing only: returns `RepositoryInterface::has_package` after `initialize`.
    pub fn __has_package(
        &mut self,
        package: crate::package::PackageInterfaceHandle,
    ) -> anyhow::Result<bool> {
        self.initialize()?;
        use crate::repository::RepositoryInterface;
        Ok(self.inner.has_package(package))
    }

    // In PHP the inherited ArrayRepository methods lazily call the overridden initialize() to glob
    // the configured url and load each path package. Without virtual dispatch we trigger that load
    // here before delegating to the inner repository; ArrayRepository's own lazy check then sees the
    // populated array and skips re-initializing it.
    fn ensure_initialized(&self) -> anyhow::Result<()> {
        if !self.inner.is_initialized() {
            self.initialize()?;
        }
        Ok(())
    }

    pub(crate) fn initialize(&self) -> anyhow::Result<()> {
        self.inner.initialize();

        let url_matches = self.get_url_matches()?;

        if url_matches.is_empty() {
            if Preg::is_match(php_regex!(r"{[*{}]}"), &self.url) {
                let mut url = self.url.clone();
                while Preg::is_match(php_regex!(r"{[*{}]}"), &url) {
                    url = shirabe_php_shim::dirname(&url);
                }
                // the parent directory before any wildcard exists, so we assume it is correctly configured but simply empty
                if shirabe_php_shim::is_dir(&url) {
                    return Ok(());
                }
            }

            return Err(RuntimeException {
                message: format!(
                    "The `url` supplied for the path ({}) repository does not exist",
                    self.url
                ),
                code: 0,
            }
            .into());
        }

        for url in url_matches {
            let path = format!("{}/", realpath(&url).unwrap_or_default());
            let composer_file_path = format!("{}composer.json", path);

            if !file_exists(&composer_file_path) {
                continue;
            }

            let json = file_get_contents(&composer_file_path).unwrap_or_default();
            let parsed = JsonFile::parse_json(Some(&json), Some(&composer_file_path))?;
            let mut package: IndexMap<String, PhpMixed> = match parsed {
                PhpMixed::Array(m) => m.into_iter().collect(),
                _ => IndexMap::new(),
            };
            let dist = {
                let mut dist: IndexMap<String, PhpMixed> = IndexMap::new();
                dist.insert("type".to_string(), PhpMixed::String("path".to_string()));
                dist.insert("url".to_string(), PhpMixed::String(url.clone()));
                dist
            };
            package.insert("dist".to_string(), PhpMixed::Array(dist));

            let reference = self
                .options
                .get("reference")
                .and_then(|v| v.as_string())
                .unwrap_or("auto")
                .to_string();
            if reference == "none" {
                if let Some(PhpMixed::Array(dist)) = package.get_mut("dist") {
                    dist.insert("reference".to_string(), PhpMixed::Null);
                }
            } else if reference == "config" || reference == "auto" {
                let options_mixed = PhpMixed::Array(
                    self.options
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                );
                let ref_hash = hash("sha1", &format!("{}{}", json, serialize(&options_mixed)));
                if let Some(PhpMixed::Array(dist)) = package.get_mut("dist") {
                    dist.insert("reference".to_string(), PhpMixed::String(ref_hash));
                }
            }

            // copy symlink/relative options to transport options
            let transport_options: IndexMap<String, PhpMixed> = self
                .options
                .iter()
                .filter(|(k, _)| k.as_str() == "symlink" || k.as_str() == "relative")
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            package.insert(
                "transport-options".to_string(),
                PhpMixed::Array(transport_options),
            );

            // use the version provided as option if available
            if let Some(name) = package
                .get("name")
                .and_then(|v| v.as_string())
                .map(|s| s.to_string())
                && let Some(version) = self
                    .options
                    .get("versions")
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.get(&name))
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_string())
            {
                package.insert("version".to_string(), PhpMixed::String(version));
            }

            // carry over the root package version if this path repo is in the same git repository as root package
            if !package.contains_key("version")
                && let Some(root_version) = Platform::get_env("COMPOSER_ROOT_VERSION")
                && !root_version.is_empty()
            {
                let mut ref1 = PhpMixed::Null;
                let mut ref2 = PhpMixed::Null;
                let cmd = PhpMixed::from(vec!["git", "rev-parse", "HEAD"]);
                let code1 = self
                    .process
                    .borrow_mut()
                    .execute(cmd.clone(), &mut ref1, Some(path.as_str()))
                    .unwrap_or(1);
                let code2 = self
                    .process
                    .borrow_mut()
                    .execute(cmd, &mut ref2, None)
                    .unwrap_or(1);
                if code1 == 0 && code2 == 0 && ref1.as_string() == ref2.as_string() {
                    package.insert(
                        "version".to_string(),
                        PhpMixed::String(
                            self.version_guesser.borrow().get_root_version_from_env()?,
                        ),
                    );
                }
            }

            let mut output = PhpMixed::Null;
            let command = GitUtil::build_rev_list_command(&self.process, {
                let mut args = vec![
                    "-n1".to_string(),
                    "--format=%H".to_string(),
                    "HEAD".to_string(),
                ];
                args.extend(GitUtil::get_no_show_signature_flags(&self.process));
                args
            });
            if reference == "auto"
                && shirabe_php_shim::is_dir(format!("{}/.git", path.trim_end_matches('/')))
                && self
                    .process
                    .borrow_mut()
                    .execute(PhpMixed::from(command), &mut output, Some(path.as_str()))
                    .unwrap_or(1)
                    == 0
            {
                let output_str = output.as_string().unwrap_or("").to_string();
                let ref_val = GitUtil::parse_rev_list_output(&output_str, &self.process)
                    .trim()
                    .to_string();
                if let Some(PhpMixed::Array(dist)) = package.get_mut("dist") {
                    dist.insert("reference".to_string(), PhpMixed::String(ref_val));
                }
            }

            if !package.contains_key("version") {
                let version_data = self
                    .version_guesser
                    .borrow_mut()
                    .guess_version(&package, &path)?;
                if let Some(version_data) = version_data {
                    if let Some(pretty_version) = version_data
                        .pretty_version
                        .as_ref()
                        .filter(|s| !s.is_empty())
                        .cloned()
                    {
                        // if there is a feature branch detected, we add a second package with the feature branch version
                        if let Some(feature_pretty_version) = version_data
                            .feature_pretty_version
                            .as_ref()
                            .filter(|s| !s.is_empty())
                            .cloned()
                        {
                            package.insert(
                                "version".to_string(),
                                PhpMixed::String(feature_pretty_version),
                            );
                            self.inner
                                .add_package(self.loader.load(package.clone(), None)?);
                        }

                        package.insert("version".to_string(), PhpMixed::String(pretty_version));
                    } else {
                        package.insert(
                            "version".to_string(),
                            PhpMixed::String("dev-main".to_string()),
                        );
                    }
                } else {
                    package.insert(
                        "version".to_string(),
                        PhpMixed::String("dev-main".to_string()),
                    );
                }
            }

            self.inner
                .add_package(self.loader.load(package.clone(), None).map_err(|e| {
                    RuntimeException {
                        message: format!("Failed loading the package in {}", composer_file_path),
                        code: 0,
                    }
                })?);
        }

        Ok(())
    }

    fn get_url_matches(&self) -> anyhow::Result<Vec<String>> {
        let mut flags = GLOB_MARK | GLOB_ONLYDIR;

        if defined("GLOB_BRACE") {
            flags |= GLOB_BRACE;
        } else if self.url.contains('{') || self.url.contains('}') {
            return Err(RuntimeException {
                message: format!(
                    "The operating system does not support GLOB_BRACE which is required for the url {}",
                    self.url
                ),
                code: 0,
            }
            .into());
        }

        // Ensure environment-specific path separators are normalized to URL separators
        Ok(glob_with_flags(&self.url, flags)
            .into_iter()
            .map(|val| {
                val.replace(DIRECTORY_SEPARATOR, "/")
                    .trim_end_matches('/')
                    .to_string()
            })
            .collect())
    }
}

impl RepositoryInterface for PathRepository {
    // The structural methods are inherited from ArrayRepository in PHP, where the lazy package load
    // is driven by the overridden initialize(). Here each one first ensures that load has happened
    // (see ensure_initialized), then delegates to the inner ArrayRepository.
    fn count(&self) -> anyhow::Result<usize> {
        self.ensure_initialized()?;
        self.inner.count()
    }

    fn has_package(&self, package: PackageInterfaceHandle) -> bool {
        // TODO(phase-d): hasPackage returns bool and cannot surface an initialization error; a
        // failed load leaves the inner repository with whatever packages were added before the
        // failure.
        let _ = self.ensure_initialized();
        self.inner.has_package(package)
    }

    fn find_package(
        &mut self,
        name: &str,
        constraint: FindPackageConstraint,
    ) -> anyhow::Result<Option<BasePackageHandle>> {
        self.ensure_initialized()?;
        self.inner.find_package(name, constraint)
    }

    fn find_packages(
        &mut self,
        name: &str,
        constraint: Option<FindPackageConstraint>,
    ) -> anyhow::Result<Vec<BasePackageHandle>> {
        self.ensure_initialized()?;
        self.inner.find_packages(name, constraint)
    }

    fn get_packages(&mut self) -> anyhow::Result<Vec<BasePackageHandle>> {
        self.ensure_initialized()?;
        self.inner.get_packages()
    }

    fn load_packages(
        &mut self,
        package_name_map: IndexMap<String, Option<shirabe_semver::constraint::AnyConstraint>>,
        acceptable_stabilities: IndexMap<String, i64>,
        stability_flags: IndexMap<String, i64>,
        already_loaded: IndexMap<String, IndexMap<String, PackageInterfaceHandle>>,
    ) -> anyhow::Result<LoadPackagesResult> {
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
    ) -> anyhow::Result<Vec<SearchResult>> {
        self.ensure_initialized()?;
        self.inner.search(query, mode, r#type)
    }

    fn get_providers(
        &mut self,
        package_name: String,
    ) -> anyhow::Result<IndexMap<String, ProviderInfo>> {
        self.ensure_initialized()?;
        self.inner.get_providers(package_name)
    }

    fn get_repo_name(&self) -> String {
        PathRepository::get_repo_name(self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn set_self_handle(&self, weak: RepositoryInterfaceWeakHandle) {
        self.inner.set_self_handle(weak);
    }
}
