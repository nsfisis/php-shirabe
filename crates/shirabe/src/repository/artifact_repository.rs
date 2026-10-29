//! ref: composer/src/Composer/Repository/ArtifactRepository.php

use crate::io::{IOInterface, IOInterfaceImmutable, io_interface};
use crate::json::JsonFile;
use crate::package::loader::{ArrayLoader, LoaderInterface};
use crate::package::{BasePackageHandle, PackageInterfaceHandle};
use crate::repository::{
    ArrayRepository, ConfigurableRepositoryInterface, FindPackageConstraint, LoadPackagesResult,
    ProviderInfo, RepositoryInterface, RepositoryInterfaceWeakHandle, SearchResult,
};
use crate::util::{Platform, Tar, Zip};
use bstr::{B, BStr, BString};
use indexmap::IndexMap;
use shirabe_php_shim::{
    PhpMixed, RuntimeException, UnexpectedValueException, bconcat, extension_loaded, hash_file,
    legacy_lossy,
};
use shirabe_semver::constraint::AnyConstraint;
use std::path::Path;

pub struct ArtifactRepository {
    inner: ArrayRepository,
    loader: Box<dyn LoaderInterface>,
    lookup: BString,
    repo_config: IndexMap<BString, PhpMixed>,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
}

impl std::fmt::Debug for ArtifactRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArtifactRepository")
            .field("lookup", &self.lookup)
            .field("repo_config", &self.repo_config)
            .finish()
    }
}

impl ArtifactRepository {
    pub fn new(
        repo_config: IndexMap<BString, PhpMixed>,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> anyhow::Result<Self> {
        if !extension_loaded("zip") {
            return Err(RuntimeException::new(
                "The artifact repository requires PHP's zip extension".to_string(),
            )
            .into());
        }

        let url = legacy_lossy(&BString::from(
            repo_config[B("url")].as_string().unwrap_or_default(),
        ))
        .into_owned();
        let lookup = Platform::expand_path(&url);
        Ok(Self {
            inner: ArrayRepository::new(Vec::new())?,
            loader: Box::new(ArrayLoader::new(None, true)),
            lookup: bstr::BString::from(lookup),
            repo_config,
            io,
        })
    }

    pub fn get_repo_name(&self) -> BString {
        bconcat!("artifact repo (", self.lookup, ")")
    }

    /// For testing only: drives `initialize` (scanning the lookup directory) and returns the
    /// packages collected by the inner `ArrayRepository`, mirroring the polymorphic
    /// `RepositoryInterface::getPackages` dispatch in PHP.
    pub fn __get_packages(&mut self) -> anyhow::Result<Vec<crate::package::BasePackageHandle>> {
        self.initialize()?;
        use crate::repository::RepositoryInterface;
        self.inner.get_packages()
    }

    fn initialize(&self) -> anyhow::Result<()> {
        self.inner.initialize();
        let lookup = legacy_lossy(&BString::from(&self.lookup.clone()[..])).into_owned();
        self.scan_directory(&lookup)
    }

    // In PHP the inherited ArrayRepository methods lazily call the overridden initialize() to scan
    // the lookup directory. Without virtual dispatch we trigger that scan here before delegating to
    // the inner repository; ArrayRepository's own lazy check then sees the populated array and skips
    // re-initializing it.
    fn ensure_initialized(&self) -> anyhow::Result<()> {
        if !self.inner.is_initialized() {
            self.initialize()?;
        }
        Ok(())
    }

    fn scan_directory(&self, path: &str) -> anyhow::Result<()> {
        let entries = std::fs::read_dir(path)?;
        for entry in entries {
            let entry = entry?;
            let file_path = entry.path();

            if file_path.is_symlink() {
                let resolved = std::fs::canonicalize(&file_path)?;
                if resolved.is_dir() {
                    self.scan_directory(resolved.to_str().unwrap_or_default())?;
                    continue;
                }
            }

            if file_path.is_dir() {
                self.scan_directory(file_path.to_str().unwrap_or_default())?;
                continue;
            }

            if !file_path.is_file() {
                continue;
            }

            let ext = file_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default()
                .to_lowercase();
            if !matches!(ext.as_str(), "zip" | "tar" | "gz" | "tgz") {
                continue;
            }

            let package = self.get_composer_information(&file_path)?;
            let basename = file_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            match package {
                None => {
                    self.io.write_error3(
                        &format!(
                            "File <comment>{}</comment> doesn't seem to hold a package",
                            basename
                        ),
                        true,
                        io_interface::VERBOSE,
                    );
                }
                Some(package) => {
                    self.io.write_error3(&format!(
                        "Found package <info>{}</info> (<comment>{}</comment>) in file <info>{}</info>",
                        package.get_name().display_lossy(),
                        package.get_pretty_version().display_lossy(),
                        basename,
                    ), true, io_interface::VERBOSE);
                    self.inner.add_package(package);
                }
            }
        }
        Ok(())
    }

    fn get_composer_information(
        &self,
        file: &Path,
    ) -> anyhow::Result<Option<crate::package::PackageInterfaceHandle>> {
        let mut json: Option<BString> = None;
        let file_extension = file
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_lowercase();

        let file_type: &str;
        if matches!(file_extension.as_str(), "gz" | "tar" | "tgz") {
            file_type = "tar";
        } else if file_extension == "zip" {
            file_type = "zip";
        } else {
            return Err(RuntimeException::new(format!(
                "Files with \"{}\" extensions aren't supported. Only ZIP and TAR/TAR.GZ/TGZ archives are supported.",
                file_extension
            ))
            .into());
        }

        let pathname = file.to_str().unwrap_or_default();
        let get_result = if file_type == "tar" {
            Tar::get_composer_json(file)
        } else {
            Zip::get_composer_json(file)
        };
        match get_result {
            Ok(j) => json = j.map(bstr::BString::from),
            Err(exception) => {
                self.io.write3(
                    &format!("Failed loading package {}: {}", pathname, exception),
                    false,
                    io_interface::VERBOSE,
                );
            }
        }

        if json.is_none() {
            return Ok(None);
        }

        let json_str = json.unwrap();
        let pathname_label = format!("{}#composer.json", pathname);
        let mut package = JsonFile::parse_json(Some(&json_str), Some(&pathname_label))?;
        let url_normalized = pathname.replace('\\', "/");
        let real_path = file
            .canonicalize()
            .ok()
            .and_then(|p| p.to_str().map(|s| s.to_string()))
            .unwrap_or_default();
        let shasum = hash_file("sha1", &real_path).unwrap_or_default();

        let mut dist = IndexMap::new();
        dist.insert("type".into(), PhpMixed::string(file_type.to_string()));
        dist.insert("url".into(), PhpMixed::string(url_normalized));
        dist.insert("shasum".into(), PhpMixed::string(shasum));
        if let Some(arr) = package.as_array_mut() {
            arr.insert("dist".into(), PhpMixed::Array(dist));
        }

        let cfg: IndexMap<BString, PhpMixed> = package
            .as_array()
            .cloned()
            .map(|m| m.into_iter().collect())
            .unwrap_or_default();
        match self.loader.load(cfg, None) {
            Ok(package) => Ok(Some(package)),
            Err(exception) => {
                let message = format!("Failed loading package in {}: {}", pathname, exception);
                Err(UnexpectedValueException::with_code_and_previous(
                    message,
                    0,
                    Some(std::sync::Arc::new(exception)),
                )
                .into())
            }
        }
    }
}

impl ConfigurableRepositoryInterface for ArtifactRepository {
    fn get_repo_config(&self) -> IndexMap<BString, PhpMixed> {
        self.repo_config.clone()
    }
}

impl RepositoryInterface for ArtifactRepository {
    // The structural methods are inherited from ArrayRepository in PHP, where the lazy directory
    // scan is driven by the overridden initialize(). Here each one first ensures that scan has
    // happened (see ensure_initialized), then delegates to the inner ArrayRepository.
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
        package_name_map: IndexMap<BString, Option<AnyConstraint>>,
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
        Ok(ArtifactRepository::get_repo_name(self))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn set_self_handle(&self, weak: RepositoryInterfaceWeakHandle) {
        self.inner.set_self_handle(weak);
    }
}
