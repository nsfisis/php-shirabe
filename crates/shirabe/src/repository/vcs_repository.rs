//! ref: composer/src/Composer/Repository/VcsRepository.php

use crate::config::Config;
use crate::downloader::TransportException;
use crate::event_dispatcher::EventDispatcher;
use crate::io::{IOInterface, IOInterfaceImmutable, io_interface};
use crate::package::loader::{
    ArrayLoader, InvalidPackageException, LoaderInterface, ValidatingArrayLoader,
};
use crate::package::version::VersionParser;
use crate::package::{BasePackageHandle, PackageInterfaceHandle};
use crate::repository::vcs::{VcsDriverInterface, VcsDriverKind};
use crate::repository::{
    ArrayRepository, ConfigurableRepositoryInterface, FindPackageConstraint,
    InvalidRepositoryException, LoadPackagesResult, ProviderInfo, RepositoryInterface,
    RepositoryInterfaceWeakHandle, SearchResult, VersionCacheInterface, VersionCacheResult,
};
use crate::util::{HttpDownloader, Platform, ProcessExecutor, Url};
use bstr::{B, BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{
    Catch as _, InvalidArgumentException, PhpClass, PhpMixed, bconcat, legacy_lossy, php_regex,
    preg_is_match, preg_replace, str_replace, strpos,
};
use shirabe_semver::constraint::SimpleConstraint;

// TODO(port): the driver registration should be refactored later.
#[derive(Debug)]
pub struct VcsRepository {
    inner: ArrayRepository,
    url: BString,
    package_name: std::cell::RefCell<Option<BString>>,
    is_verbose: bool,
    is_very_verbose: bool,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    config: std::rc::Rc<std::cell::RefCell<Config>>,
    version_parser: std::cell::RefCell<Option<VersionParser>>,
    r#type: BString,
    loader: std::cell::RefCell<Option<Box<dyn LoaderInterface>>>,
    repo_config: IndexMap<BString, PhpMixed>,
    http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
    process_executor: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    branch_error_occurred: std::cell::Cell<bool>,
    drivers: IndexMap<BString, VcsDriverKind>,
    driver: std::cell::RefCell<Option<Box<dyn VcsDriverInterface>>>,
    /// Kind of the resolved `driver`, used by `get_repo_name` to recover the driver type
    /// (PHP `array_search(get_class($driver), $this->drivers)`).
    driver_kind: std::cell::Cell<Option<VcsDriverKind>>,
    version_cache: Option<Box<dyn VersionCacheInterface>>,
    empty_references: std::cell::RefCell<Vec<BString>>,
    version_transport_exceptions:
        std::cell::RefCell<IndexMap<BString, IndexMap<BString, TransportException>>>,
    _dispatcher: Option<std::rc::Rc<std::cell::RefCell<EventDispatcher>>>,
}

impl ConfigurableRepositoryInterface for VcsRepository {
    fn get_repo_config(&self) -> IndexMap<BString, PhpMixed> {
        self.repo_config.clone()
    }
}

impl VcsRepository {
    #[allow(clippy::too_many_arguments, reason = "to keep PHP signature")]
    pub fn new(
        mut repo_config: IndexMap<BString, PhpMixed>,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
        dispatcher: Option<std::rc::Rc<std::cell::RefCell<EventDispatcher>>>,
        process: Option<std::rc::Rc<std::cell::RefCell<ProcessExecutor>>>,
        drivers: Option<IndexMap<BString, VcsDriverKind>>,
        version_cache: Option<Box<dyn VersionCacheInterface>>,
    ) -> anyhow::Result<Self> {
        let inner = ArrayRepository::new(vec![])?;
        let drivers = drivers.unwrap_or_else(|| {
            let mut m: IndexMap<BString, VcsDriverKind> = IndexMap::new();
            m.insert(
                bstr::BString::from("github".to_string()),
                VcsDriverKind::GitHub,
            );
            m.insert(
                bstr::BString::from("gitlab".to_string()),
                VcsDriverKind::GitLab,
            );
            m.insert(
                bstr::BString::from("bitbucket".to_string()),
                VcsDriverKind::GitBitbucket,
            );
            m.insert(
                bstr::BString::from("git-bitbucket".to_string()),
                VcsDriverKind::GitBitbucket,
            );
            m.insert(
                bstr::BString::from("forgejo".to_string()),
                VcsDriverKind::Forgejo,
            );
            m.insert(bstr::BString::from("git".to_string()), VcsDriverKind::Git);
            m.insert(bstr::BString::from("hg".to_string()), VcsDriverKind::Hg);
            m.insert(
                bstr::BString::from("perforce".to_string()),
                VcsDriverKind::Perforce,
            );
            m.insert(
                bstr::BString::from("fossil".to_string()),
                VcsDriverKind::Fossil,
            );
            // svn must be last because identifying a subversion server for sure is practically impossible
            m.insert(bstr::BString::from("svn".to_string()), VcsDriverKind::Svn);
            m
        });

        let url = Platform::expand_path(
            &repo_config
                .get(B("url"))
                .and_then(|v| v.as_string())
                .unwrap_or_default()
                .to_str_lossy(),
        );
        repo_config.insert("url".into(), PhpMixed::string(url.clone()));
        let r#type = BString::from(
            repo_config
                .get(B("type"))
                .and_then(|v| v.as_string())
                .unwrap_or("vcs".into()),
        );
        let is_verbose = io.is_verbose();
        let is_very_verbose = io.is_very_verbose();
        let process_executor = process.unwrap_or_else(|| {
            std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
                io.clone(),
            ))))
        });

        Ok(Self {
            inner,
            url: bstr::BString::from(url),
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

    pub fn get_repo_name(&self) -> BString {
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
                .unwrap_or_else(|| BString::from(kind.php_class_name())),
            None => BString::default(),
        };

        bconcat!(
            "vcs repo (",
            driver_type,
            " ",
            Url::sanitize(&self.url),
            ")"
        )
    }

    pub fn get_repo_config(&self) -> &IndexMap<BString, PhpMixed> {
        &self.repo_config
    }

    pub fn set_loader(&mut self, loader: Box<dyn LoaderInterface>) {
        *self.loader.borrow_mut() = Some(loader);
    }

    /// PHP `getDriver()` lazily instantiates and memoizes the matching VCS driver. This populates
    /// `self.driver`/`self.driver_kind` once.
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
                .supports(
                    self.io.clone(),
                    self.config.clone(),
                    self.url.as_bstr(),
                    false,
                )
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
                .supports(
                    self.io.clone(),
                    self.config.clone(),
                    self.url.as_bstr(),
                    true,
                )
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

    pub fn get_empty_references(&self) -> Vec<BString> {
        self.empty_references.borrow().clone()
    }

    pub fn get_version_transport_exceptions(
        &self,
    ) -> IndexMap<BString, IndexMap<BString, TransportException>> {
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

    pub fn initialize(&self) -> anyhow::Result<()> {
        self.inner.initialize();

        let is_verbose = self.is_verbose;
        let is_very_verbose = self.is_very_verbose;

        let driver_url = self.url.clone();
        self.ensure_driver();
        if self.driver.borrow().is_none() {
            return Err(InvalidArgumentException::new(format!(
                "No driver found to handle VCS repository {}",
                driver_url.display_lossy()
            ))
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
                .has_composer_file(root_identifier.as_bstr());
            match has_composer_file {
                Ok(b) => {
                    has_root_identifier_composer_json = b;
                    if has_root_identifier_composer_json {
                        let composer_information = self
                            .driver
                            .borrow_mut()
                            .as_mut()
                            .unwrap()
                            .get_composer_information(root_identifier.as_bstr());
                        match composer_information {
                            Ok(Some(data)) => {
                                *self.package_name.borrow_mut() = data
                                    .get(B("name"))
                                    .and_then(|v| v.as_string())
                                    .filter(|s| !s.is_empty())
                                    .map(bstr::BString::from);
                            }
                            Ok(None) => {}
                            Err(e) => {
                                if let Some(te) = e.catch::<TransportException>()
                                    && self.should_rethrow_transport_exception(te)
                                {
                                    return Err(e);
                                }
                                if is_very_verbose {
                                    self.io.write_error(&format!(
                                        "<error>Skipped parsing {}, {}</error>",
                                        root_identifier.display_lossy(),
                                        e
                                    ));
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    if let Some(te) = e.catch::<TransportException>()
                        && self.should_rethrow_transport_exception(te)
                    {
                        return Err(e);
                    }
                    if is_very_verbose {
                        self.io.write_error(&format!(
                            "<error>Skipped parsing {}, {}</error>",
                            root_identifier.display_lossy(),
                            e
                        ));
                    }
                }
            }
        }

        let tags = self.driver.borrow_mut().as_mut().unwrap().get_tags()?;
        for (tag, identifier) in tags {
            let mut tag = legacy_lossy(&tag).into_owned();
            let msg = format!(
                "Reading composer.json of <info>{}</info> (<comment>{}</comment>)",
                legacy_lossy(
                    &self
                        .package_name
                        .borrow()
                        .clone()
                        .unwrap_or_else(|| self.url.clone())
                ),
                tag
            );

            // strip the release- prefix from tags if present
            tag = legacy_lossy(&BString::from(&str_replace("release-", "", &tag)[..])).into_owned();

            let cached_package = self.get_cached_package_version(
                &tag,
                identifier.as_bstr(),
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

            let result: anyhow::Result<()> = (|| -> anyhow::Result<()> {
                let data_opt = self
                    .driver
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .get_composer_information(identifier.as_bstr())?;
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
                if data.contains_key(B("version")) {
                    let normalized = self.version_parser.borrow().as_ref().unwrap().normalize(
                        data.get(B("version"))
                            .and_then(|v| v.as_string())
                            .unwrap_or_default(),
                        None,
                    )?;
                    data.insert("version_normalized".into(), PhpMixed::string(normalized));
                } else {
                    // auto-versioned package, read value from tag
                    data.insert("version".into(), PhpMixed::string(tag.clone()));
                    data.insert(
                        "version_normalized".into(),
                        PhpMixed::string(parsed_tag.clone()),
                    );
                }

                // make sure tag packages have no -dev flag
                data.insert(
                    "version".into(),
                    PhpMixed::string(preg_replace(
                        php_regex!(r"{[.-]?dev$}i"),
                        "",
                        data.get(B("version"))
                            .and_then(|v| v.as_string())
                            .unwrap_or_default(),
                    )),
                );
                data.insert(
                    "version_normalized".into(),
                    PhpMixed::string(preg_replace(
                        php_regex!(r"{(^dev-|[.-]?dev$)}i"),
                        "",
                        data.get(B("version_normalized"))
                            .and_then(|v| v.as_string())
                            .unwrap_or_default(),
                    )),
                );

                // make sure tag do not contain the default-branch marker
                data.shift_remove(B("default-branch"));

                let version_normalized = BString::from(
                    data.get(B("version_normalized"))
                        .and_then(|v| v.as_string())
                        .unwrap_or_default(),
                );

                // broken package, version doesn't match tag
                if version_normalized != parsed_tag {
                    if is_very_verbose {
                        if preg_is_match(php_regex!(r"{(^dev-|[.-]?dev$)}i"), &parsed_tag) {
                            self.io.write_error(&format!(
                                "<warning>Skipped tag {}, invalid tag name, tags can not use dev prefixes or suffixes</warning>",
                                tag
                            ));
                        } else {
                            self.io.write_error(&format!(
                                "<warning>Skipped tag {}, tag ({}) does not match version ({}) in composer.json</warning>",
                                tag, parsed_tag.display_lossy(), version_normalized.display_lossy()
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
                        BString::from(
                            data.get(B("name"))
                                .and_then(|v| v.as_string())
                                .unwrap_or_default(),
                        )
                    });
                if let Some(existing_package) = self.inner.find_package_internal(
                    tag_package_name.as_bstr(),
                    crate::repository::FindPackageConstraint::Constraint(
                        SimpleConstraint::new("=", BString::from(&version_normalized[..]), None)
                            .into(),
                    ),
                )? {
                    if is_very_verbose {
                        self.io.write_error(&format!(
                            "<warning>Skipped tag {}, it conflicts with an another tag ({}) as both resolve to {} internally</warning>",
                            tag, existing_package.get_pretty_version().display_lossy(), version_normalized.display_lossy()
                        ));
                    }
                    return Ok(());
                }

                if is_very_verbose {
                    self.io.write_error(&format!(
                        "Importing tag {} ({})",
                        tag,
                        version_normalized.display_lossy()
                    ));
                }

                let processed = {
                    let driver_ref = self.driver.borrow();
                    let driver = driver_ref.as_ref().unwrap();
                    self.pre_process(&**driver, data, identifier.as_bstr())?
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
                if let Some(te) = e.catch::<TransportException>() {
                    self.version_transport_exceptions
                        .borrow_mut()
                        .entry(bstr::BString::from("tags".to_string()))
                        .or_default()
                        .insert(bstr::BString::from(tag.clone()), te.clone());
                    if te.get_code() == 404 {
                        self.empty_references.borrow_mut().push(identifier.clone());
                    }
                    if self.should_rethrow_transport_exception(te) {
                        return Err(e);
                    }
                }
                if is_very_verbose {
                    let detail = if let Some(te) = e.catch::<TransportException>() {
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
            let mut new_branches: IndexMap<BString, BString> = IndexMap::new();
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
                legacy_lossy(
                    &self
                        .package_name
                        .borrow()
                        .clone()
                        .unwrap_or_else(|| self.url.clone())
                ),
                legacy_lossy(&branch)
            );
            if is_very_verbose {
                self.io.write_error(&msg);
            } else if is_verbose {
                self.io
                    .overwrite_error4(&msg, false, None, io_interface::NORMAL);
            }

            let parsed_branch_opt = self.validate_branch(branch.as_bstr());
            if parsed_branch_opt.is_none() {
                if is_very_verbose {
                    self.io.write_error(&format!(
                        "<warning>Skipped branch {}, invalid name</warning>",
                        branch.display_lossy()
                    ));
                }
                continue;
            }
            let mut parsed_branch = parsed_branch_opt.unwrap();

            // make sure branch packages have a dev flag
            let version: BString;
            if strpos(&parsed_branch, "dev-") == Some(0)
                || VersionParser::DEFAULT_BRANCH_ALIAS == parsed_branch
            {
                version = bconcat!("dev-", str_replace("#", "+", &branch));
                parsed_branch = BString::from(&str_replace("#", "+", &parsed_branch)[..]);
            } else {
                let prefix = if strpos(&branch, "v") == Some(0) {
                    "v"
                } else {
                    ""
                };
                version = bconcat!(prefix, preg_replace(r"{(\.9{7})+}", ".x", &parsed_branch));
            }

            let is_default_branch = self
                .driver
                .borrow_mut()
                .as_mut()
                .unwrap()
                .get_root_identifier()?
                == branch;
            let cached_package = self.get_cached_package_version(
                &legacy_lossy(&version),
                identifier.as_bstr(),
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

            let result: anyhow::Result<()> = (|| -> anyhow::Result<()> {
                let data_opt = self
                    .driver
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .get_composer_information(identifier.as_bstr())?;
                if data_opt.is_none() {
                    if is_very_verbose {
                        self.io.write_error(&format!(
                            "<warning>Skipped branch {}, no composer file</warning>",
                            branch.display_lossy()
                        ));
                    }
                    self.empty_references.borrow_mut().push(identifier.clone());
                    return Ok(());
                }
                let mut data = data_opt.unwrap();

                // branches are always auto-versioned, read value from branch name
                data.insert("version".into(), PhpMixed::string(version.clone()));
                data.insert(
                    "version_normalized".into(),
                    PhpMixed::string(parsed_branch.clone()),
                );

                data.shift_remove(B("default-branch"));
                if self
                    .driver
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .get_root_identifier()?
                    == branch
                {
                    data.insert("default-branch".into(), PhpMixed::Bool(true));
                }

                if is_very_verbose {
                    self.io.write_error(&format!(
                        "Importing branch {} ({})",
                        branch.display_lossy(),
                        data.get(B("version"))
                            .and_then(|v| v.as_string())
                            .unwrap_or_default()
                            .display_lossy()
                    ));
                }

                let package_data = {
                    let driver_ref = self.driver.borrow();
                    let driver = driver_ref.as_ref().unwrap();
                    self.pre_process(&**driver, data, identifier.as_bstr())?
                };
                let package = self
                    .loader
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .load(package_data.clone(), None)?;
                // PHP: `$this->loader instanceof ValidatingArrayLoader`.
                let loader_ref = self.loader.borrow();
                let loader_as_validating = loader_ref
                    .as_ref()
                    .and_then(|l| l.as_any().downcast_ref::<ValidatingArrayLoader>());
                if let Some(validating) = loader_as_validating
                    && !validating.get_warnings().is_empty()
                {
                    return Err(InvalidPackageException::new(
                        validating.get_errors(),
                        validating.get_warnings(),
                        package_data,
                    )
                    .into());
                }
                drop(loader_ref);
                self.inner.add_package(package)?;
                Ok(())
            })();
            if let Err(e) = result {
                if let Some(te) = e.catch::<TransportException>() {
                    self.version_transport_exceptions
                        .borrow_mut()
                        .entry(bstr::BString::from("branches".to_string()))
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
                            branch.display_lossy(), te.get_code()
                        ));
                    }
                    continue;
                }
                if !is_very_verbose {
                    self.io.write_error("");
                }
                self.branch_error_occurred.set(true);
                self.io.write_error(&format!(
                    "<error>Skipped branch {}, {}</error>",
                    branch.display_lossy(),
                    e
                ));
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
                self.url.display_lossy()
            )).into());
        }

        Ok(())
    }

    fn pre_process(
        &self,
        driver: &dyn VcsDriverInterface,
        mut data: IndexMap<BString, PhpMixed>,
        identifier: &BStr,
    ) -> anyhow::Result<IndexMap<BString, PhpMixed>> {
        // keep the name of the main identifier for all packages
        // this ensures that a package can be renamed in one place and that all old tags
        // will still be installable using that new name without requiring re-tagging
        let data_package_name = data.get(B("name")).and_then(|v| v.as_string());
        let name_value = self
            .package_name
            .borrow()
            .clone()
            .filter(|s| !s.is_empty())
            .or(data_package_name.map(bstr::BString::from));
        data.insert(
            "name".into(),
            match name_value {
                Some(n) => PhpMixed::string(n),
                None => PhpMixed::Null,
            },
        );

        if !data.contains_key(B("dist")) {
            let dist = driver.get_dist(identifier);
            data.insert(
                "dist".into(),
                match dist {
                    Some(m) => PhpMixed::Array(
                        m.into_iter()
                            .map(|(k, v)| (k, PhpMixed::string(v)))
                            .collect(),
                    ),
                    None => PhpMixed::Null,
                },
            );
        }
        if !data.contains_key(B("source")) {
            let source = driver.get_source(identifier);
            data.insert(
                "source".into(),
                PhpMixed::Array(
                    source
                        .into_iter()
                        .map(|(k, v)| (k, PhpMixed::string(v)))
                        .collect(),
                ),
            );
        }

        // if custom dist info is provided but does not provide a reference, copy the source reference to it
        let dist_is_array = matches!(data.get(B("dist")), Some(PhpMixed::Array(_)));
        let dist_lacks_reference = data
            .get(B("dist"))
            .and_then(|v| match v {
                PhpMixed::Array(m) => Some(!m.contains_key(B("reference"))),
                _ => None,
            })
            .unwrap_or(false);
        let source_reference = data.get(B("source")).and_then(|v| match v {
            PhpMixed::Array(m) => m.get(B("reference")).cloned(),
            _ => None,
        });
        if dist_is_array
            && dist_lacks_reference
            && source_reference.is_some()
            && let Some(PhpMixed::Array(dist_map)) = data.get_mut(B("dist"))
        {
            dist_map.insert("reference".into(), source_reference.unwrap());
        }

        Ok(data)
    }

    fn validate_branch(&self, branch: &BStr) -> Option<BString> {
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

    fn validate_tag(&self, version: &str) -> Option<BString> {
        self.version_parser
            .borrow()
            .as_ref()
            .unwrap()
            .normalize(version, None)
            .ok()
    }

    fn get_cached_package_version(
        &self,
        version: &str,
        identifier: &BStr,
        is_verbose: bool,
        is_very_verbose: bool,
        is_default_branch: bool,
    ) -> anyhow::Result<CachedPackageResult> {
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
                    version,
                    identifier.display_lossy()
                ));
            }

            return Ok(CachedPackageResult::Missing);
        }

        if let VersionCacheResult::Package(ref mut data) = cached_package {
            let msg = format!(
                "Found cached composer.json of <info>{}</info> (<comment>{}</comment>)",
                legacy_lossy(
                    &self
                        .package_name
                        .borrow()
                        .clone()
                        .unwrap_or_else(|| self.url.clone())
                ),
                version
            );
            if is_very_verbose {
                self.io.write_error(&msg);
            } else if is_verbose {
                self.io
                    .overwrite_error4(&msg, false, None, io_interface::NORMAL);
            }

            data.shift_remove(B("default-branch"));
            if is_default_branch {
                data.insert("default-branch".into(), PhpMixed::Bool(true));
            }

            let name = BString::from(
                data.get(B("name"))
                    .and_then(|v| v.as_string())
                    .unwrap_or_default(),
            );
            let version_normalized = BString::from(
                data.get(B("version_normalized"))
                    .and_then(|v| v.as_string())
                    .unwrap_or_default(),
            );
            if let Some(existing_package) = self.inner.find_package_internal(
                name.as_bstr(),
                crate::repository::FindPackageConstraint::Constraint(
                    SimpleConstraint::new("=", BString::from(&version_normalized[..]), None).into(),
                ),
            )? {
                if is_very_verbose {
                    self.io.write_error(&format!(
                        "<warning>Skipped cached version {}, it conflicts with an another tag ({}) as both resolve to {} internally</warning>",
                        version, existing_package.get_pretty_version().display_lossy(), version_normalized.display_lossy()
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
        matches!(e.get_code(), 401 | 403 | 429) || e.get_code() >= 500
    }
}

impl RepositoryInterface for VcsRepository {
    // The structural methods are inherited from ArrayRepository in PHP, where the lazy package load
    // is driven by the overridden initialize(). Here each one first ensures that load has happened
    // (see ensure_initialized), then delegates to the inner ArrayRepository.
    fn count(&mut self) -> anyhow::Result<usize> {
        self.ensure_initialized()?;
        self.inner.count()
    }

    fn has_package(&mut self, package: PackageInterfaceHandle) -> anyhow::Result<bool> {
        self.ensure_initialized()?;
        self.inner.has_package(package)
    }

    fn find_package(
        &mut self,
        name: &BStr,
        constraint: FindPackageConstraint,
    ) -> anyhow::Result<Option<BasePackageHandle>> {
        self.ensure_initialized()?;
        self.inner.find_package(name, constraint)
    }

    fn find_packages(
        &mut self,
        name: &BStr,
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
        package_name_map: IndexMap<BString, Option<shirabe_semver::constraint::AnyConstraint>>,
        acceptable_stabilities: IndexMap<BString, i64>,
        stability_flags: IndexMap<BString, i64>,
        already_loaded: IndexMap<BString, IndexMap<BString, PackageInterfaceHandle>>,
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
        query: BString,
        mode: i64,
        r#type: Option<BString>,
    ) -> anyhow::Result<Vec<SearchResult>> {
        self.ensure_initialized()?;
        self.inner.search(query, mode, r#type)
    }

    fn get_providers(
        &mut self,
        package_name: BString,
    ) -> anyhow::Result<IndexMap<BString, ProviderInfo>> {
        self.ensure_initialized()?;
        self.inner.get_providers(package_name)
    }

    fn get_repo_name(&self) -> anyhow::Result<BString> {
        Ok(VcsRepository::get_repo_name(self))
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
