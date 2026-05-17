//! ref: composer/src/Composer/Repository/VcsRepository.php

use crate::io::io_interface;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{
    InvalidArgumentException, PhpMixed, array_search_mixed, count, get_class, in_array,
    str_replace, strpos,
};
use shirabe_semver::constraint::constraint::Constraint;

use crate::config::Config;
use crate::downloader::transport_exception::TransportException;
use crate::event_dispatcher::event_dispatcher::EventDispatcher;
use crate::io::io_interface::IOInterface;
use crate::package::base_package::BasePackage;
use crate::package::loader::array_loader::ArrayLoader;
use crate::package::loader::invalid_package_exception::InvalidPackageException;
use crate::package::loader::loader_interface::LoaderInterface;
use crate::package::loader::validating_array_loader::ValidatingArrayLoader;
use crate::package::version::version_parser::VersionParser;
use crate::repository::array_repository::ArrayRepository;
use crate::repository::configurable_repository_interface::ConfigurableRepositoryInterface;
use crate::repository::invalid_repository_exception::InvalidRepositoryException;
use crate::repository::vcs::vcs_driver_interface::VcsDriverInterface;
use crate::repository::version_cache_interface::VersionCacheInterface;
use crate::util::http_downloader::HttpDownloader;
use crate::util::platform::Platform;
use crate::util::process_executor::ProcessExecutor;
use crate::util::url::Url;

#[derive(Debug)]
pub struct VcsRepository {
    pub(crate) inner: ArrayRepository,
    /// @var string
    pub(crate) url: String,
    /// @var ?string
    pub(crate) package_name: Option<String>,
    /// @var bool
    pub(crate) is_verbose: bool,
    /// @var bool
    pub(crate) is_very_verbose: bool,
    /// @var IOInterface
    pub(crate) io: Box<dyn IOInterface>,
    /// @var Config
    pub(crate) config: Config,
    /// @var VersionParser
    pub(crate) version_parser: Option<VersionParser>,
    /// @var string
    pub(crate) r#type: String,
    /// @var ?LoaderInterface
    pub(crate) loader: Option<Box<dyn LoaderInterface>>,
    /// @var array<string, mixed>
    pub(crate) repo_config: IndexMap<String, PhpMixed>,
    /// @var HttpDownloader
    pub(crate) http_downloader: HttpDownloader,
    /// @var ProcessExecutor
    pub(crate) process_executor: ProcessExecutor,
    /// @var bool
    pub(crate) branch_error_occurred: bool,
    /// @var array<string, class-string<VcsDriverInterface>>
    drivers: IndexMap<String, String>,
    /// @var ?VcsDriverInterface
    driver: Option<Box<dyn VcsDriverInterface>>,
    /// @var ?VersionCacheInterface
    version_cache: Option<Box<dyn VersionCacheInterface>>,
    /// @var list<string>
    empty_references: Vec<String>,
    /// @var array<'tags'|'branches', array<string, TransportException>>
    version_transport_exceptions: IndexMap<String, IndexMap<String, TransportException>>,
    /// @var ?EventDispatcher (preserved for plugin events)
    _dispatcher: Option<EventDispatcher>,
}

impl ConfigurableRepositoryInterface for VcsRepository {}

impl VcsRepository {
    /// @param array{url: string, type?: string}&array<string, mixed> $repoConfig
    /// @param array<string, class-string<VcsDriverInterface>>|null $drivers
    pub fn new(
        mut repo_config: IndexMap<String, PhpMixed>,
        io: Box<dyn IOInterface>,
        config: Config,
        http_downloader: HttpDownloader,
        dispatcher: Option<EventDispatcher>,
        process: Option<ProcessExecutor>,
        drivers: Option<IndexMap<String, String>>,
        version_cache: Option<Box<dyn VersionCacheInterface>>,
    ) -> Result<Self> {
        let inner = ArrayRepository::new(vec![])?;
        let drivers = drivers.unwrap_or_else(|| {
            let mut m: IndexMap<String, String> = IndexMap::new();
            m.insert(
                "github".to_string(),
                "Composer\\Repository\\Vcs\\GitHubDriver".to_string(),
            );
            m.insert(
                "gitlab".to_string(),
                "Composer\\Repository\\Vcs\\GitLabDriver".to_string(),
            );
            m.insert(
                "bitbucket".to_string(),
                "Composer\\Repository\\Vcs\\GitBitbucketDriver".to_string(),
            );
            m.insert(
                "git-bitbucket".to_string(),
                "Composer\\Repository\\Vcs\\GitBitbucketDriver".to_string(),
            );
            m.insert(
                "forgejo".to_string(),
                "Composer\\Repository\\Vcs\\ForgejoDriver".to_string(),
            );
            m.insert(
                "git".to_string(),
                "Composer\\Repository\\Vcs\\GitDriver".to_string(),
            );
            m.insert(
                "hg".to_string(),
                "Composer\\Repository\\Vcs\\HgDriver".to_string(),
            );
            m.insert(
                "perforce".to_string(),
                "Composer\\Repository\\Vcs\\PerforceDriver".to_string(),
            );
            m.insert(
                "fossil".to_string(),
                "Composer\\Repository\\Vcs\\FossilDriver".to_string(),
            );
            // svn must be last because identifying a subversion server for sure is practically impossible
            m.insert(
                "svn".to_string(),
                "Composer\\Repository\\Vcs\\SvnDriver".to_string(),
            );
            m
        });

        let url = Platform::expand_path(
            repo_config
                .get("url")
                .and_then(|v| v.as_string())
                .unwrap_or(""),
        );
        repo_config.insert("url".to_string(), PhpMixed::String(url.clone()));
        let r#type = repo_config
            .get("type")
            .and_then(|v| v.as_string())
            .unwrap_or("vcs")
            .to_string();
        let is_verbose = io.is_verbose();
        let is_very_verbose = io.is_very_verbose();
        let process_executor =
            process.unwrap_or_else(|| ProcessExecutor::new(Some(Box::new(&*io)), None));

        Ok(Self {
            inner,
            url,
            package_name: None,
            is_verbose,
            is_very_verbose,
            io,
            config,
            version_parser: None,
            r#type,
            loader: None,
            repo_config,
            http_downloader,
            process_executor,
            branch_error_occurred: false,
            drivers,
            driver: None,
            version_cache,
            empty_references: vec![],
            version_transport_exceptions: IndexMap::new(),
            _dispatcher: dispatcher,
        })
    }

    pub fn get_repo_name(&mut self) -> String {
        let driver = self.get_driver().expect("driver should be available");
        let driver_class = get_class(&PhpMixed::Null); // TODO(phase-b): obtain runtime class name of $driver
        let driver_type = array_search_mixed(
            &PhpMixed::String(driver_class.clone()),
            &PhpMixed::Array(
                self.drivers
                    .iter()
                    .map(|(k, v)| (k.clone(), Box::new(PhpMixed::String(v.clone()))))
                    .collect(),
            ),
            false,
        )
        .map(|v| v.as_string().unwrap_or("").to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or(driver_class);
        let _ = driver;

        format!("vcs repo ({} {})", driver_type, Url::sanitize(&self.url))
    }

    pub fn get_repo_config(&self) -> &IndexMap<String, PhpMixed> {
        &self.repo_config
    }

    pub fn set_loader(&mut self, loader: Box<dyn LoaderInterface>) {
        self.loader = Some(loader);
    }

    pub fn get_driver(&mut self) -> Option<&mut Box<dyn VcsDriverInterface>> {
        if self.driver.is_some() {
            return self.driver.as_mut();
        }

        if let Some(_class) = self.drivers.get(&self.r#type).cloned() {
            // TODO(phase-b): dynamic class-string instantiation `new $class(...)`
            let driver: Option<Box<dyn VcsDriverInterface>> = None;
            if let Some(mut d) = driver {
                let _ = d.initialize();
                self.driver = Some(d);
            }
            return self.driver.as_mut();
        }

        for (_, _driver_class) in self.drivers.iter() {
            // TODO(phase-b): static-method dispatch on class-string: `$driver::supports(...)`
            let supports = false;
            if supports {
                // TODO(phase-b): dynamic class-string instantiation `new $driver(...)`
                let d: Option<Box<dyn VcsDriverInterface>> = None;
                if let Some(mut d) = d {
                    let _ = d.initialize();
                    self.driver = Some(d);
                }
                return self.driver.as_mut();
            }
        }

        for (_, _driver_class) in self.drivers.iter() {
            // TODO(phase-b): static-method dispatch on class-string: `$driver::supports(..., true)`
            let supports = false;
            if supports {
                let d: Option<Box<dyn VcsDriverInterface>> = None;
                if let Some(mut d) = d {
                    let _ = d.initialize();
                    self.driver = Some(d);
                }
                return self.driver.as_mut();
            }
        }

        None
    }

    pub fn had_invalid_branches(&self) -> bool {
        self.branch_error_occurred
    }

    /// @return list<string>
    pub fn get_empty_references(&self) -> &Vec<String> {
        &self.empty_references
    }

    /// @return array<'tags'|'branches', array<string, TransportException>>
    pub fn get_version_transport_exceptions(
        &self,
    ) -> &IndexMap<String, IndexMap<String, TransportException>> {
        &self.version_transport_exceptions
    }

    pub fn initialize(&mut self) -> Result<()> {
        self.inner.initialize();

        let is_verbose = self.is_verbose;
        let is_very_verbose = self.is_very_verbose;

        let driver_url = self.url.clone();
        let driver = self.get_driver();
        if driver.is_none() {
            return Err(InvalidArgumentException {
                message: format!("No driver found to handle VCS repository {}", driver_url),
                code: 0,
            }
            .into());
        }
        // TODO(phase-b): VersionParser has no public `new`
        self.version_parser = Some(todo!("VersionParser::new()"));
        if self.loader.is_none() {
            self.loader = Some(Box::new(ArrayLoader::new(
                Some(todo!("phase-b: clone VersionParser")),
                false,
            )));
        }

        let mut has_root_identifier_composer_json = false;
        let root_identifier_result = self.driver.as_mut().unwrap().get_root_identifier();
        if let Ok(root_identifier) = root_identifier_result {
            match self
                .driver
                .as_mut()
                .unwrap()
                .has_composer_file(&root_identifier)
            {
                Ok(b) => {
                    has_root_identifier_composer_json = b;
                    if has_root_identifier_composer_json {
                        match self
                            .driver
                            .as_mut()
                            .unwrap()
                            .get_composer_information(&root_identifier)
                        {
                            Ok(Some(data)) => {
                                self.package_name = data
                                    .get("name")
                                    .and_then(|v| v.as_string())
                                    .filter(|s| !s.is_empty())
                                    .map(String::from);
                            }
                            Ok(None) => {}
                            Err(e) => {
                                // TODO(phase-b): unify exception handling below
                                if let Some(te) = e.downcast_ref::<TransportException>() {
                                    if self.should_rethrow_transport_exception(te) {
                                        return Err(e);
                                    }
                                }
                                if is_very_verbose {
                                    self.io.write_error(&format!(
                                        "<error>Skipped parsing {}, {}</error>",
                                        root_identifier, e
                                    ));
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    if let Some(te) = e.downcast_ref::<TransportException>() {
                        if self.should_rethrow_transport_exception(te) {
                            return Err(e);
                        }
                    }
                    if is_very_verbose {
                        self.io.write_error(&format!(
                            "<error>Skipped parsing {}, {}</error>",
                            root_identifier, e
                        ));
                    }
                }
            }
        }

        let driver = self.driver.as_mut().unwrap();
        for (tag, identifier) in driver.get_tags()? {
            let mut tag = tag;
            let msg = format!(
                "Reading composer.json of <info>{}</info> (<comment>{}</comment>)",
                self.package_name
                    .clone()
                    .unwrap_or_else(|| self.url.clone()),
                tag
            );

            // strip the release- prefix from tags if present
            tag = str_replace("release-", "", &tag);

            let cached_package = self.get_cached_package_version(
                &tag,
                &identifier,
                is_verbose,
                is_very_verbose,
                false,
            )?;
            if let CachedPackageResult::Package(pkg) = cached_package {
                self.inner.add_package(pkg)?;
                continue;
            }
            if matches!(cached_package, CachedPackageResult::Missing) {
                self.empty_references.push(identifier.clone());
                continue;
            }

            let parsed_tag = self.validate_tag(&tag);
            if parsed_tag.is_none() {
                if is_very_verbose {
                    self.io.write_error(&format!(
                        "<warning>Skipped tag {}, invalid tag name</warning>",
                        tag
                    ));
                }
                continue;
            }
            let parsed_tag = parsed_tag.unwrap();

            if is_very_verbose {
                self.io.write_error(&msg);
            } else if is_verbose {
                self.io.overwrite_error(
                    PhpMixed::String(msg.clone()),
                    false,
                    None,
                    io_interface::NORMAL,
                );
            }

            let result: Result<()> = (|| -> Result<()> {
                let driver = self.driver.as_mut().unwrap();
                let data_opt = driver.get_composer_information(&identifier)?;
                if data_opt.is_none() {
                    if is_very_verbose {
                        self.io.write_error(&format!(
                            "<warning>Skipped tag {}, no composer file</warning>",
                            tag
                        ));
                    }
                    self.empty_references.push(identifier.clone());
                    return Ok(());
                }
                let mut data = data_opt.unwrap();

                // manually versioned package
                if data.contains_key("version") {
                    let normalized = self.version_parser.as_ref().unwrap().normalize(
                        data.get("version")
                            .and_then(|v| v.as_string())
                            .unwrap_or(""),
                        None,
                    )?;
                    data.insert(
                        "version_normalized".to_string(),
                        PhpMixed::String(normalized),
                    );
                } else {
                    // auto-versioned package, read value from tag
                    data.insert("version".to_string(), PhpMixed::String(tag.clone()));
                    data.insert(
                        "version_normalized".to_string(),
                        PhpMixed::String(parsed_tag.clone()),
                    );
                }

                // make sure tag packages have no -dev flag
                data.insert(
                    "version".to_string(),
                    PhpMixed::String(Preg::replace(
                        r"{[.-]?dev$}i",
                        "",
                        data.get("version")
                            .and_then(|v| v.as_string())
                            .unwrap_or(""),
                    )),
                );
                data.insert(
                    "version_normalized".to_string(),
                    PhpMixed::String(Preg::replace(
                        r"{(^dev-|[.-]?dev$)}i",
                        "",
                        data.get("version_normalized")
                            .and_then(|v| v.as_string())
                            .unwrap_or(""),
                    )),
                );

                // make sure tag do not contain the default-branch marker
                data.shift_remove("default-branch");

                let version_normalized = data
                    .get("version_normalized")
                    .and_then(|v| v.as_string())
                    .unwrap_or("")
                    .to_string();

                // broken package, version doesn't match tag
                if version_normalized != parsed_tag {
                    if is_very_verbose {
                        if Preg::is_match(r"{(^dev-|[.-]?dev$)}i", &parsed_tag) {
                            self.io.write_error(&format!(
                                "<warning>Skipped tag {}, invalid tag name, tags can not use dev prefixes or suffixes</warning>",
                                tag
                            ));
                        } else {
                            self.io.write_error(&format!(
                                "<warning>Skipped tag {}, tag ({}) does not match version ({}) in composer.json</warning>",
                                tag, parsed_tag, version_normalized
                            ));
                        }
                    }
                    return Ok(());
                }

                let tag_package_name = self
                    .package_name
                    .clone()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| {
                        data.get("name")
                            .and_then(|v| v.as_string())
                            .unwrap_or("")
                            .to_string()
                    });
                if let Some(existing_package) = self.inner.find_package(
                    &tag_package_name,
                    Box::new(Constraint::new("=", &version_normalized)),
                ) {
                    if is_very_verbose {
                        self.io.write_error(&format!(
                            "<warning>Skipped tag {}, it conflicts with an another tag ({}) as both resolve to {} internally</warning>",
                            tag, existing_package.get_pretty_version(), version_normalized
                        ));
                    }
                    return Ok(());
                }

                if is_very_verbose {
                    self.io
                        .write_error(&format!("Importing tag {} ({})", tag, version_normalized));
                }

                let driver = self.driver.as_mut().unwrap();
                let processed = self.pre_process(&**driver, data, &identifier)?;
                let loaded = self.loader.as_ref().unwrap().load(processed, None)?;
                self.inner.add_package(Box::new(loaded))?;
                Ok(())
            })();
            if let Err(e) = result {
                if let Some(te) = e.downcast_ref::<TransportException>() {
                    self.version_transport_exceptions
                        .entry("tags".to_string())
                        .or_insert_with(IndexMap::new)
                        .insert(tag.clone(), te.clone());
                    if te.get_code() == 404 {
                        self.empty_references.push(identifier.clone());
                    }
                    if self.should_rethrow_transport_exception(te) {
                        return Err(e);
                    }
                }
                if is_very_verbose {
                    let detail = if let Some(te) = e.downcast_ref::<TransportException>() {
                        format!(
                            "no composer file was found ({} HTTP status code)",
                            te.get_code()
                        )
                    } else {
                        format!("{}", e)
                    };
                    self.io.write_error(&format!(
                        "<warning>Skipped tag {}, {}</warning>",
                        tag, detail
                    ));
                }
                continue;
            }
        }

        if !is_very_verbose {
            self.io.overwrite_error(
                PhpMixed::String(String::new()),
                false,
                None,
                io_interface::NORMAL,
            );
        }

        let mut branches = self.driver.as_mut().unwrap().get_branches()?;
        // make sure the root identifier branch gets loaded first
        let root_identifier = self.driver.as_mut().unwrap().get_root_identifier()?;
        if has_root_identifier_composer_json && branches.contains_key(&root_identifier) {
            let mut new_branches: IndexMap<String, String> = IndexMap::new();
            new_branches.insert(
                root_identifier.clone(),
                branches.get(&root_identifier).cloned().unwrap_or_default(),
            );
            for (k, v) in branches {
                if !new_branches.contains_key(&k) {
                    new_branches.insert(k, v);
                }
            }
            branches = new_branches;
        }

        for (branch, identifier) in branches {
            let msg = format!(
                "Reading composer.json of <info>{}</info> (<comment>{}</comment>)",
                self.package_name
                    .clone()
                    .unwrap_or_else(|| self.url.clone()),
                branch
            );
            if is_very_verbose {
                self.io.write_error(&msg);
            } else if is_verbose {
                self.io.overwrite_error(
                    PhpMixed::String(msg.clone()),
                    false,
                    None,
                    io_interface::NORMAL,
                );
            }

            let parsed_branch_opt = self.validate_branch(&branch);
            if parsed_branch_opt.is_none() {
                if is_very_verbose {
                    self.io.write_error(&format!(
                        "<warning>Skipped branch {}, invalid name</warning>",
                        branch
                    ));
                }
                continue;
            }
            let mut parsed_branch = parsed_branch_opt.unwrap();

            // make sure branch packages have a dev flag
            let version: String;
            if strpos(&parsed_branch, "dev-") == Some(0)
                || VersionParser::DEFAULT_BRANCH_ALIAS == parsed_branch
            {
                version = format!("dev-{}", str_replace("#", "+", &branch));
                parsed_branch = str_replace("#", "+", &parsed_branch);
            } else {
                let prefix = if strpos(&branch, "v") == Some(0) {
                    "v"
                } else {
                    ""
                };
                version = format!(
                    "{}{}",
                    prefix,
                    Preg::replace(r"{(\.9{7})+}", ".x", &parsed_branch)
                );
            }

            let is_default_branch = self.driver.as_mut().unwrap().get_root_identifier()? == branch;
            let cached_package = self.get_cached_package_version(
                &version,
                &identifier,
                is_verbose,
                is_very_verbose,
                is_default_branch,
            )?;
            if let CachedPackageResult::Package(pkg) = cached_package {
                self.inner.add_package(pkg)?;
                continue;
            }
            if matches!(cached_package, CachedPackageResult::Missing) {
                self.empty_references.push(identifier.clone());
                continue;
            }

            let result: Result<()> = (|| -> Result<()> {
                let driver = self.driver.as_mut().unwrap();
                let data_opt = driver.get_composer_information(&identifier)?;
                if data_opt.is_none() {
                    if is_very_verbose {
                        self.io.write_error(&format!(
                            "<warning>Skipped branch {}, no composer file</warning>",
                            branch
                        ));
                    }
                    self.empty_references.push(identifier.clone());
                    return Ok(());
                }
                let mut data = data_opt.unwrap();

                // branches are always auto-versioned, read value from branch name
                data.insert("version".to_string(), PhpMixed::String(version.clone()));
                data.insert(
                    "version_normalized".to_string(),
                    PhpMixed::String(parsed_branch.clone()),
                );

                data.shift_remove("default-branch");
                if driver.get_root_identifier()? == branch {
                    data.insert("default-branch".to_string(), PhpMixed::Bool(true));
                }

                if is_very_verbose {
                    self.io.write_error(&format!(
                        "Importing branch {} ({})",
                        branch,
                        data.get("version")
                            .and_then(|v| v.as_string())
                            .unwrap_or("")
                    ));
                }

                let package_data = self.pre_process(&**driver, data, &identifier)?;
                let package = self
                    .loader
                    .as_ref()
                    .unwrap()
                    .load(package_data.clone(), None)?;
                // TODO(phase-b): `$this->loader instanceof ValidatingArrayLoader` downcast
                let loader_as_validating: Option<&ValidatingArrayLoader> = None;
                if let Some(validating) = loader_as_validating {
                    if count(&PhpMixed::Null) > 0 {
                        let _ = validating;
                        return Err(
                            InvalidPackageException::new(vec![], vec![], package_data).into()
                        );
                    }
                }
                self.inner.add_package(Box::new(package))?;
                Ok(())
            })();
            if let Err(e) = result {
                if let Some(te) = e.downcast_ref::<TransportException>() {
                    self.version_transport_exceptions
                        .entry("branches".to_string())
                        .or_insert_with(IndexMap::new)
                        .insert(branch.clone(), te.clone());
                    if te.get_code() == 404 {
                        self.empty_references.push(identifier.clone());
                    }
                    if self.should_rethrow_transport_exception(te) {
                        return Err(e);
                    }
                    if is_very_verbose {
                        self.io.write_error(&format!(
                            "<warning>Skipped branch {}, no composer file was found ({} HTTP status code)</warning>",
                            branch, te.get_code()
                        ));
                    }
                    continue;
                }
                if !is_very_verbose {
                    self.io.write_error("");
                }
                self.branch_error_occurred = true;
                self.io
                    .write_error(&format!("<error>Skipped branch {}, {}</error>", branch, e));
                self.io.write_error("");
                continue;
            }
        }
        self.driver.as_mut().unwrap().cleanup()?;

        if !is_very_verbose {
            self.io.overwrite_error(
                PhpMixed::String(String::new()),
                false,
                None,
                io_interface::NORMAL,
            );
        }

        if self.inner.get_packages().is_empty() {
            return Err(InvalidRepositoryException {
                message: format!(
                    "No valid composer.json was found in any branch or tag of {}, could not load a package from it.",
                    self.url
                ),
                code: 0,
            }
            .into());
        }

        Ok(())
    }

    /// @param array{name?: string, dist?: array{type: string, url: string, reference: string, shasum: string}, source?: array{type: string, url: string, reference: string}} $data
    ///
    /// @return array{name: string|null, dist: array{type: string, url: string, reference: string, shasum: string}|null, source: array{type: string, url: string, reference: string}}
    fn pre_process(
        &self,
        driver: &dyn VcsDriverInterface,
        mut data: IndexMap<String, PhpMixed>,
        identifier: &str,
    ) -> Result<IndexMap<String, PhpMixed>> {
        // keep the name of the main identifier for all packages
        // this ensures that a package can be renamed in one place and that all old tags
        // will still be installable using that new name without requiring re-tagging
        let data_package_name = data
            .get("name")
            .and_then(|v| v.as_string())
            .map(String::from);
        let name_value = self
            .package_name
            .clone()
            .filter(|s| !s.is_empty())
            .or(data_package_name);
        data.insert(
            "name".to_string(),
            match name_value {
                Some(n) => PhpMixed::String(n),
                None => PhpMixed::Null,
            },
        );

        if !data.contains_key("dist") {
            let dist = driver.get_dist(identifier);
            data.insert(
                "dist".to_string(),
                match dist {
                    Some(m) => PhpMixed::Array(
                        m.into_iter()
                            .map(|(k, v)| (k, Box::new(PhpMixed::String(v))))
                            .collect(),
                    ),
                    None => PhpMixed::Null,
                },
            );
        }
        if !data.contains_key("source") {
            let source = driver.get_source(identifier);
            data.insert(
                "source".to_string(),
                PhpMixed::Array(
                    source
                        .into_iter()
                        .map(|(k, v)| (k, Box::new(PhpMixed::String(v))))
                        .collect(),
                ),
            );
        }

        // if custom dist info is provided but does not provide a reference, copy the source reference to it
        let dist_is_array = matches!(data.get("dist"), Some(PhpMixed::Array(_)));
        let dist_lacks_reference = data
            .get("dist")
            .and_then(|v| match v {
                PhpMixed::Array(m) => Some(!m.contains_key("reference")),
                _ => None,
            })
            .unwrap_or(false);
        let source_reference = data.get("source").and_then(|v| match v {
            PhpMixed::Array(m) => m.get("reference").cloned(),
            _ => None,
        });
        if dist_is_array && dist_lacks_reference && source_reference.is_some() {
            if let Some(PhpMixed::Array(dist_map)) = data.get_mut("dist") {
                dist_map.insert("reference".to_string(), source_reference.unwrap());
            }
        }

        Ok(data)
    }

    /// @return string|false
    fn validate_branch(&self, branch: &str) -> Option<String> {
        let result = self
            .version_parser
            .as_ref()
            .unwrap()
            .normalize_branch(branch);
        if let Ok(normalized_branch) = result {
            // validate that the branch name has no weird characters conflicting with constraints
            if self
                .version_parser
                .as_ref()
                .unwrap()
                .parse_constraints(&normalized_branch)
                .is_ok()
            {
                return Some(normalized_branch);
            }
        }

        None
    }

    /// @return string|false
    fn validate_tag(&self, version: &str) -> Option<String> {
        self.version_parser
            .as_ref()
            .unwrap()
            .normalize(version, None)
            .ok()
    }

    /// @return \Composer\Package\CompletePackage|\Composer\Package\CompleteAliasPackage|null|false null if no cache present, false if the absence of a version was cached
    fn get_cached_package_version(
        &mut self,
        version: &str,
        identifier: &str,
        is_verbose: bool,
        is_very_verbose: bool,
        is_default_branch: bool,
    ) -> Result<CachedPackageResult> {
        if self.version_cache.is_none() {
            return Ok(CachedPackageResult::None);
        }

        let mut cached_package = self
            .version_cache
            .as_ref()
            .unwrap()
            .get_version_package(version, identifier);
        if matches!(cached_package, VersionCacheResult::Missing) {
            if is_very_verbose {
                self.io.write_error(&format!(
                    "<warning>Skipped {}, no composer file (cached from ref {})</warning>",
                    version, identifier
                ));
            }

            return Ok(CachedPackageResult::Missing);
        }

        if let VersionCacheResult::Package(ref mut data) = cached_package {
            let msg = format!(
                "Found cached composer.json of <info>{}</info> (<comment>{}</comment>)",
                self.package_name
                    .clone()
                    .unwrap_or_else(|| self.url.clone()),
                version
            );
            if is_very_verbose {
                self.io.write_error(&msg);
            } else if is_verbose {
                self.io.overwrite_error(
                    PhpMixed::String(msg.clone()),
                    false,
                    None,
                    io_interface::NORMAL,
                );
            }

            data.shift_remove("default-branch");
            if is_default_branch {
                data.insert("default-branch".to_string(), PhpMixed::Bool(true));
            }

            let name = data
                .get("name")
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string();
            let version_normalized = data
                .get("version_normalized")
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string();
            if let Some(existing_package) = self
                .inner
                .find_package(&name, Box::new(Constraint::new("=", &version_normalized)))
            {
                if is_very_verbose {
                    self.io.write_error(&format!(
                        "<warning>Skipped cached version {}, it conflicts with an another tag ({}) as both resolve to {} internally</warning>",
                        version, existing_package.get_pretty_version(), version_normalized
                    ));
                }
                cached_package = VersionCacheResult::None;
            }
        }

        if let VersionCacheResult::Package(data) = cached_package {
            let loaded = self.loader.as_ref().unwrap().load(data, None)?;
            return Ok(CachedPackageResult::Package(Box::new(loaded)));
        }

        Ok(CachedPackageResult::None)
    }

    fn should_rethrow_transport_exception(&self, e: &TransportException) -> bool {
        in_array(
            PhpMixed::Int(e.get_code()),
            &PhpMixed::List(vec![
                Box::new(PhpMixed::Int(401)),
                Box::new(PhpMixed::Int(403)),
                Box::new(PhpMixed::Int(429)),
            ]),
            true,
        ) || e.get_code() >= 500
    }
}

#[derive(Debug)]
enum CachedPackageResult {
    None,
    Missing,
    Package(Box<BasePackage>),
}

#[derive(Debug)]
enum VersionCacheResult {
    None,
    Missing,
    Package(IndexMap<String, PhpMixed>),
}
