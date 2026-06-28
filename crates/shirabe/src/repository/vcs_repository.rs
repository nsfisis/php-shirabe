//! ref: composer/src/Composer/Repository/VcsRepository.php

use crate::io::io_interface;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_php_shim::{InvalidArgumentException, PhpMixed, in_array, str_replace, strpos};
use shirabe_semver::constraint::SimpleConstraint;

use crate::config::Config;
use crate::downloader::TransportException;
use crate::event_dispatcher::EventDispatcher;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::package::BasePackageHandle;
use crate::package::PackageInterfaceHandle;
use crate::package::loader::ArrayLoader;
use crate::package::loader::InvalidPackageException;
use crate::package::loader::LoaderInterface;
use crate::package::loader::ValidatingArrayLoader;
use crate::package::version::VersionParser;
use crate::repository::ArrayRepository;
use crate::repository::ConfigurableRepositoryInterface;
use crate::repository::InvalidRepositoryException;
use crate::repository::RepositoryInterface;
use crate::repository::RepositoryInterfaceWeakHandle;
use crate::repository::vcs::VcsDriverInterface;
use crate::repository::vcs::VcsDriverKind;
use crate::repository::{FindPackageConstraint, LoadPackagesResult, ProviderInfo, SearchResult};
use crate::repository::{VersionCacheInterface, VersionCacheResult};
use crate::util::HttpDownloader;
use crate::util::Platform;
use crate::util::ProcessExecutor;
use crate::util::Url;

// TODO(phase-c): the driver registration should be refactored later.
#[derive(Debug)]
pub struct VcsRepository {
    pub(crate) inner: ArrayRepository,
    /// @var string
    pub(crate) url: String,
    /// @var ?string
    ///
    /// Interior mutability: set lazily by the (now `&self`) `initialize`, mirroring how PHP's
    /// inherited ArrayRepository methods drive the overridden `initialize()` on first access.
    pub(crate) package_name: std::cell::RefCell<Option<String>>,
    /// @var bool
    pub(crate) is_verbose: bool,
    /// @var bool
    pub(crate) is_very_verbose: bool,
    /// @var IOInterface
    pub(crate) io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    /// @var Config
    pub(crate) config: std::rc::Rc<std::cell::RefCell<Config>>,
    /// @var VersionParser
    pub(crate) version_parser: std::cell::RefCell<Option<VersionParser>>,
    /// @var string
    pub(crate) r#type: String,
    /// @var ?LoaderInterface
    pub(crate) loader: std::cell::RefCell<Option<Box<dyn LoaderInterface>>>,
    /// @var array<string, mixed>
    pub(crate) repo_config: IndexMap<String, PhpMixed>,
    /// @var HttpDownloader
    pub(crate) http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
    /// @var ProcessExecutor
    pub(crate) process_executor: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    /// @var bool
    pub(crate) branch_error_occurred: std::cell::Cell<bool>,
    /// @var array<string, class-string<VcsDriverInterface>>
    drivers: IndexMap<String, VcsDriverKind>,
    /// @var ?VcsDriverInterface
    ///
    /// Interior mutability: memoized by `ensure_driver` (PHP `getDriver`), which is reached from
    /// the `&self` RepositoryInterface methods (`count`, `has_package`, `get_repo_name`).
    driver: std::cell::RefCell<Option<Box<dyn VcsDriverInterface>>>,
    /// Kind of the resolved `driver`, used by `get_repo_name` to recover the driver type
    /// (PHP `array_search(get_class($driver), $this->drivers)`).
    driver_kind: std::cell::Cell<Option<VcsDriverKind>>,
    /// @var ?VersionCacheInterface
    version_cache: Option<Box<dyn VersionCacheInterface>>,
    /// @var list<string>
    empty_references: std::cell::RefCell<Vec<String>>,
    /// @var array<'tags'|'branches', array<string, TransportException>>
    version_transport_exceptions:
        std::cell::RefCell<IndexMap<String, IndexMap<String, TransportException>>>,
    /// @var ?EventDispatcher (preserved for plugin events)
    _dispatcher: Option<std::rc::Rc<std::cell::RefCell<EventDispatcher>>>,
}

impl ConfigurableRepositoryInterface for VcsRepository {
    fn get_repo_config(&self) -> IndexMap<String, PhpMixed> {
        self.repo_config.clone()
    }
}

impl VcsRepository {
    #[allow(clippy::too_many_arguments, reason = "to keep PHP signature")]
    pub fn new(
        mut repo_config: IndexMap<String, PhpMixed>,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
        dispatcher: Option<std::rc::Rc<std::cell::RefCell<EventDispatcher>>>,
        process: Option<std::rc::Rc<std::cell::RefCell<ProcessExecutor>>>,
        drivers: Option<IndexMap<String, VcsDriverKind>>,
        version_cache: Option<Box<dyn VersionCacheInterface>>,
    ) -> Result<Self> {
        let inner = ArrayRepository::new(vec![])?;
        let drivers = drivers.unwrap_or_else(|| {
            let mut m: IndexMap<String, VcsDriverKind> = IndexMap::new();
            m.insert("github".to_string(), VcsDriverKind::GitHub);
            m.insert("gitlab".to_string(), VcsDriverKind::GitLab);
            m.insert("bitbucket".to_string(), VcsDriverKind::GitBitbucket);
            m.insert("git-bitbucket".to_string(), VcsDriverKind::GitBitbucket);
            m.insert("forgejo".to_string(), VcsDriverKind::Forgejo);
            m.insert("git".to_string(), VcsDriverKind::Git);
            m.insert("hg".to_string(), VcsDriverKind::Hg);
            m.insert("perforce".to_string(), VcsDriverKind::Perforce);
            m.insert("fossil".to_string(), VcsDriverKind::Fossil);
            // svn must be last because identifying a subversion server for sure is practically impossible
            m.insert("svn".to_string(), VcsDriverKind::Svn);
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
        let process_executor = process.unwrap_or_else(|| {
            std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
                io.clone(),
            ))))
        });

        Ok(Self {
            inner,
            url,
            package_name: std::cell::RefCell::new(None),
            is_verbose,
            is_very_verbose,
            io,
            config,
            version_parser: std::cell::RefCell::new(None),
            r#type,
            loader: std::cell::RefCell::new(None),
            repo_config,
            http_downloader,
            process_executor,
            branch_error_occurred: std::cell::Cell::new(false),
            drivers,
            driver: std::cell::RefCell::new(None),
            driver_kind: std::cell::Cell::new(None),
            version_cache,
            empty_references: std::cell::RefCell::new(vec![]),
            version_transport_exceptions: std::cell::RefCell::new(IndexMap::new()),
            _dispatcher: dispatcher,
        })
    }

    pub fn get_repo_name(&self) -> String {
        // Ensure the driver is resolved so `driver_kind` is populated.
        self.ensure_driver();
        assert!(self.driver.borrow().is_some(), "driver should be available");
        // PHP: array_search(get_class($driver), $this->drivers), falling back to the class name.
        let driver_type = match self.driver_kind.get() {
            Some(kind) => self
                .drivers
                .iter()
                .find(|(_, v)| **v == kind)
                .map(|(name, _)| name.clone())
                .unwrap_or_else(|| kind.php_class_name().to_string()),
            None => String::new(),
        };

        format!(
            "vcs repo ({} {})",
            driver_type,
            Url::sanitize(self.url.clone())
        )
    }

    pub fn get_repo_config(&self) -> &IndexMap<String, PhpMixed> {
        &self.repo_config
    }

    pub fn set_loader(&mut self, loader: Box<dyn LoaderInterface>) {
        *self.loader.borrow_mut() = Some(loader);
    }

    /// PHP `getDriver()` lazily instantiates and memoizes the matching VCS driver. Because Rust's
    /// `RefCell` cannot hand out a `&mut` reference that outlives the borrow, this is split from the
    /// reference-returning shape into a resolver: it populates `self.driver`/`self.driver_kind` once,
    /// and callers borrow `self.driver` as needed afterwards.
    pub fn ensure_driver(&self) {
        if self.driver.borrow().is_some() {
            return;
        }

        if let Some(kind) = self.drivers.get(&self.r#type).copied() {
            let mut driver = kind.instantiate(
                self.repo_config.clone(),
                self.io.clone(),
                self.config.clone(),
                self.http_downloader.clone(),
                self.process_executor.clone(),
            );
            let _ = driver.initialize();
            *self.driver.borrow_mut() = Some(driver);
            self.driver_kind.set(Some(kind));
            return;
        }

        let kinds: Vec<VcsDriverKind> = self.drivers.values().copied().collect();

        for kind in &kinds {
            if kind
                .supports(self.io.clone(), self.config.clone(), &self.url, false)
                .unwrap_or(false)
            {
                let mut driver = kind.instantiate(
                    self.repo_config.clone(),
                    self.io.clone(),
                    self.config.clone(),
                    self.http_downloader.clone(),
                    self.process_executor.clone(),
                );
                let _ = driver.initialize();
                *self.driver.borrow_mut() = Some(driver);
                self.driver_kind.set(Some(*kind));
                return;
            }
        }

        for kind in &kinds {
            if kind
                .supports(self.io.clone(), self.config.clone(), &self.url, true)
                .unwrap_or(false)
            {
                let mut driver = kind.instantiate(
                    self.repo_config.clone(),
                    self.io.clone(),
                    self.config.clone(),
                    self.http_downloader.clone(),
                    self.process_executor.clone(),
                );
                let _ = driver.initialize();
                *self.driver.borrow_mut() = Some(driver);
                self.driver_kind.set(Some(*kind));
                return;
            }
        }
    }

    pub fn had_invalid_branches(&self) -> bool {
        self.branch_error_occurred.get()
    }

    /// @return list<string>
    pub fn get_empty_references(&self) -> Vec<String> {
        self.empty_references.borrow().clone()
    }

    /// @return array<'tags'|'branches', array<string, TransportException>>
    pub fn get_version_transport_exceptions(
        &self,
    ) -> IndexMap<String, IndexMap<String, TransportException>> {
        self.version_transport_exceptions.borrow().clone()
    }

    /// For testing only: drives `initialize` (which shells out to the VCS driver to discover
    /// tags/branches and load each package) and returns the packages collected by the inner
    /// `ArrayRepository`, mirroring the polymorphic `RepositoryInterface::getPackages` dispatch
    /// in PHP where `VcsRepository` inherits `getPackages` from `ArrayRepository`.
    pub fn __get_packages(&mut self) -> anyhow::Result<Vec<crate::package::BasePackageHandle>> {
        self.initialize()?;
        use crate::repository::RepositoryInterface;
        self.inner.get_packages()
    }

    // In PHP the inherited ArrayRepository methods lazily call the overridden initialize() to drive
    // the VCS driver and load each tag/branch package. Without virtual dispatch we trigger that load
    // here before delegating to the inner repository; ArrayRepository's own lazy check then sees the
    // populated array and skips re-initializing it.
    fn ensure_initialized(&self) -> anyhow::Result<()> {
        if !self.inner.is_initialized() {
            self.initialize()?;
        }
        Ok(())
    }

    pub fn initialize(&self) -> Result<()> {
        self.inner.initialize();

        let is_verbose = self.is_verbose;
        let is_very_verbose = self.is_very_verbose;

        let driver_url = self.url.clone();
        self.ensure_driver();
        if self.driver.borrow().is_none() {
            return Err(InvalidArgumentException {
                message: format!("No driver found to handle VCS repository {}", driver_url),
                code: 0,
            }
            .into());
        }
        *self.version_parser.borrow_mut() = Some(VersionParser::new());
        if self.loader.borrow().is_none() {
            let version_parser = self.version_parser.borrow().clone();
            *self.loader.borrow_mut() = Some(Box::new(ArrayLoader::new(version_parser, false)));
        }

        let mut has_root_identifier_composer_json = false;
        let root_identifier_result = self
            .driver
            .borrow_mut()
            .as_mut()
            .unwrap()
            .get_root_identifier();
        if let Ok(root_identifier) = root_identifier_result {
            let has_composer_file = self
                .driver
                .borrow_mut()
                .as_mut()
                .unwrap()
                .has_composer_file(&root_identifier);
            match has_composer_file {
                Ok(b) => {
                    has_root_identifier_composer_json = b;
                    if has_root_identifier_composer_json {
                        let composer_information = self
                            .driver
                            .borrow_mut()
                            .as_mut()
                            .unwrap()
                            .get_composer_information(&root_identifier);
                        match composer_information {
                            Ok(Some(data)) => {
                                *self.package_name.borrow_mut() = data
                                    .get("name")
                                    .and_then(|v| v.as_string())
                                    .filter(|s| !s.is_empty())
                                    .map(String::from);
                            }
                            Ok(None) => {}
                            Err(e) => {
                                if let Some(te) = e.downcast_ref::<TransportException>()
                                    && self.should_rethrow_transport_exception(te)
                                {
                                    return Err(e);
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
                    if let Some(te) = e.downcast_ref::<TransportException>()
                        && self.should_rethrow_transport_exception(te)
                    {
                        return Err(e);
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

        let tags = self.driver.borrow_mut().as_mut().unwrap().get_tags()?;
        for (tag, identifier) in tags {
            let mut tag = tag;
            let msg = format!(
                "Reading composer.json of <info>{}</info> (<comment>{}</comment>)",
                self.package_name
                    .borrow()
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
            match cached_package {
                CachedPackageResult::Package(pkg) => {
                    self.inner.add_package(pkg)?;
                    continue;
                }
                CachedPackageResult::Missing => {
                    self.empty_references.borrow_mut().push(identifier.clone());
                    continue;
                }
                CachedPackageResult::None => {}
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
                self.io
                    .overwrite_error4(&msg, false, None, io_interface::NORMAL);
            }

            let result: Result<()> = (|| -> Result<()> {
                let data_opt = self
                    .driver
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .get_composer_information(&identifier)?;
                if data_opt.is_none() {
                    if is_very_verbose {
                        self.io.write_error(&format!(
                            "<warning>Skipped tag {}, no composer file</warning>",
                            tag
                        ));
                    }
                    self.empty_references.borrow_mut().push(identifier.clone());
                    return Ok(());
                }
                let mut data = data_opt.unwrap();

                // manually versioned package
                if data.contains_key("version") {
                    let normalized = self.version_parser.borrow().as_ref().unwrap().normalize(
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
                    .borrow()
                    .clone()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| {
                        data.get("name")
                            .and_then(|v| v.as_string())
                            .unwrap_or("")
                            .to_string()
                    });
                if let Some(existing_package) = self.inner.find_package_internal(
                    &tag_package_name,
                    crate::repository::FindPackageConstraint::Constraint(
                        SimpleConstraint::new(
                            "=".to_string(),
                            version_normalized.to_string(),
                            None,
                        )
                        .into(),
                    ),
                )? {
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

                let processed = {
                    let driver_ref = self.driver.borrow();
                    let driver = driver_ref.as_ref().unwrap();
                    self.pre_process(&**driver, data, &identifier)?
                };
                let loaded = self
                    .loader
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .load(processed, None)?;
                self.inner.add_package(loaded)?;
                Ok(())
            })();
            if let Err(e) = result {
                if let Some(te) = e.downcast_ref::<TransportException>() {
                    self.version_transport_exceptions
                        .borrow_mut()
                        .entry("tags".to_string())
                        .or_default()
                        .insert(tag.clone(), te.clone());
                    if te.get_code() == 404 {
                        self.empty_references.borrow_mut().push(identifier.clone());
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
            self.io
                .overwrite_error4("", false, None, io_interface::NORMAL);
        }

        let mut branches = self.driver.borrow_mut().as_mut().unwrap().get_branches()?;
        // make sure the root identifier branch gets loaded first
        let root_identifier = self
            .driver
            .borrow_mut()
            .as_mut()
            .unwrap()
            .get_root_identifier()?;
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
                    .borrow()
                    .clone()
                    .unwrap_or_else(|| self.url.clone()),
                branch
            );
            if is_very_verbose {
                self.io.write_error(&msg);
            } else if is_verbose {
                self.io
                    .overwrite_error4(&msg, false, None, io_interface::NORMAL);
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

            let is_default_branch = self
                .driver
                .borrow_mut()
                .as_mut()
                .unwrap()
                .get_root_identifier()?
                == branch;
            let cached_package = self.get_cached_package_version(
                &version,
                &identifier,
                is_verbose,
                is_very_verbose,
                is_default_branch,
            )?;
            match cached_package {
                CachedPackageResult::Package(pkg) => {
                    self.inner.add_package(pkg)?;
                    continue;
                }
                CachedPackageResult::Missing => {
                    self.empty_references.borrow_mut().push(identifier.clone());
                    continue;
                }
                CachedPackageResult::None => {}
            }

            let result: Result<()> = (|| -> Result<()> {
                let data_opt = self
                    .driver
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .get_composer_information(&identifier)?;
                if data_opt.is_none() {
                    if is_very_verbose {
                        self.io.write_error(&format!(
                            "<warning>Skipped branch {}, no composer file</warning>",
                            branch
                        ));
                    }
                    self.empty_references.borrow_mut().push(identifier.clone());
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
                if self
                    .driver
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .get_root_identifier()?
                    == branch
                {
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

                let package_data = {
                    let driver_ref = self.driver.borrow();
                    let driver = driver_ref.as_ref().unwrap();
                    self.pre_process(&**driver, data, &identifier)?
                };
                let package = self
                    .loader
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .load(package_data.clone(), None)?;
                // PHP: `$this->loader instanceof ValidatingArrayLoader`.
                // TODO(phase-c): ValidatingArrayLoader does not implement LoaderInterface yet (its
                // `load` needs `&mut self`, requiring a LoaderInterface redesign), so it can never be
                // stored in `self.loader` and this downcast is always None. Production never calls
                // setLoader so the default ArrayLoader matches upstream, but the InvalidPackageException
                // path stays dead until the trait is reworked.
                let loader_ref = self.loader.borrow();
                let loader_as_validating = loader_ref
                    .as_ref()
                    .and_then(|l| l.as_any().downcast_ref::<ValidatingArrayLoader>());
                if let Some(validating) = loader_as_validating
                    && !validating.get_warnings().is_empty()
                {
                    return Err(InvalidPackageException::new(
                        validating.get_errors().to_vec(),
                        validating.get_warnings().to_vec(),
                        package_data,
                    )
                    .into());
                }
                drop(loader_ref);
                self.inner.add_package(package)?;
                Ok(())
            })();
            if let Err(e) = result {
                if let Some(te) = e.downcast_ref::<TransportException>() {
                    self.version_transport_exceptions
                        .borrow_mut()
                        .entry("branches".to_string())
                        .or_default()
                        .insert(branch.clone(), te.clone());
                    if te.get_code() == 404 {
                        self.empty_references.borrow_mut().push(identifier.clone());
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
                self.branch_error_occurred.set(true);
                self.io
                    .write_error(&format!("<error>Skipped branch {}, {}</error>", branch, e));
                self.io.write_error("");
                continue;
            }
        }
        self.driver.borrow_mut().as_mut().unwrap().cleanup()?;

        if !is_very_verbose {
            self.io
                .overwrite_error4("", false, None, io_interface::NORMAL);
        }

        if self.inner.get_packages_internal().is_empty() {
            return Err(InvalidRepositoryException::new(format!(
                "No valid composer.json was found in any branch or tag of {}, could not load a package from it.",
                self.url
            )).into());
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
            .borrow()
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
            let dist = driver.get_dist(identifier)?;
            data.insert(
                "dist".to_string(),
                match dist {
                    Some(m) => PhpMixed::Array(
                        m.into_iter()
                            .map(|(k, v)| (k, PhpMixed::String(v)))
                            .collect(),
                    ),
                    None => PhpMixed::Null,
                },
            );
        }
        if !data.contains_key("source") {
            let source = driver.get_source(identifier)?;
            data.insert(
                "source".to_string(),
                PhpMixed::Array(
                    source
                        .into_iter()
                        .map(|(k, v)| (k, PhpMixed::String(v)))
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
        if dist_is_array
            && dist_lacks_reference
            && source_reference.is_some()
            && let Some(PhpMixed::Array(dist_map)) = data.get_mut("dist")
        {
            dist_map.insert("reference".to_string(), source_reference.unwrap());
        }

        Ok(data)
    }

    /// @return string|false
    fn validate_branch(&self, branch: &str) -> Option<String> {
        let result = self
            .version_parser
            .borrow()
            .as_ref()
            .unwrap()
            .normalize_branch(branch);
        if let Ok(normalized_branch) = result {
            // validate that the branch name has no weird characters conflicting with constraints
            if self
                .version_parser
                .borrow()
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
            .borrow()
            .as_ref()
            .unwrap()
            .normalize(version, None)
            .ok()
    }

    /// @return \Composer\Package\CompletePackage|\Composer\Package\CompleteAliasPackage|null|false null if no cache present, false if the absence of a version was cached
    fn get_cached_package_version(
        &self,
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
                    .borrow()
                    .clone()
                    .unwrap_or_else(|| self.url.clone()),
                version
            );
            if is_very_verbose {
                self.io.write_error(&msg);
            } else if is_verbose {
                self.io
                    .overwrite_error4(&msg, false, None, io_interface::NORMAL);
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
            if let Some(existing_package) = self.inner.find_package_internal(
                &name,
                crate::repository::FindPackageConstraint::Constraint(
                    SimpleConstraint::new("=".to_string(), version_normalized.to_string(), None)
                        .into(),
                ),
            )? {
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
            let loaded = self.loader.borrow().as_ref().unwrap().load(data, None)?;
            return Ok(CachedPackageResult::Package(loaded));
        }

        Ok(CachedPackageResult::None)
    }

    fn should_rethrow_transport_exception(&self, e: &TransportException) -> bool {
        in_array(
            PhpMixed::Int(e.get_code()),
            &PhpMixed::List(vec![
                PhpMixed::Int(401),
                PhpMixed::Int(403),
                PhpMixed::Int(429),
            ]),
            true,
        ) || e.get_code() >= 500
    }
}

impl RepositoryInterface for VcsRepository {
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
        VcsRepository::get_repo_name(self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn set_self_handle(&self, weak: RepositoryInterfaceWeakHandle) {
        self.inner.set_self_handle(weak);
    }
}

#[derive(Debug)]
enum CachedPackageResult {
    None,
    Missing,
    Package(crate::package::PackageInterfaceHandle),
}
