//! ref: composer/src/Composer/Repository/ArtifactRepository.php

use std::path::Path;

use indexmap::IndexMap;
use shirabe_php_shim::{extension_loaded, hash_file, PhpMixed, RuntimeException, UnexpectedValueException};

use crate::io::io_interface::IOInterface;
use crate::json::json_file::JsonFile;
use crate::package::base_package::BasePackage;
use crate::package::loader::array_loader::ArrayLoader;
use crate::package::loader::loader_interface::LoaderInterface;
use crate::repository::array_repository::ArrayRepository;
use crate::repository::configurable_repository_interface::ConfigurableRepositoryInterface;
use crate::util::platform::Platform;
use crate::util::tar::Tar;
use crate::util::zip::Zip;

pub struct ArtifactRepository {
    inner: ArrayRepository,
    pub(crate) loader: Box<dyn LoaderInterface>,
    pub(crate) lookup: String,
    pub(crate) repo_config: IndexMap<String, PhpMixed>,
    io: Box<dyn IOInterface>,
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
    pub fn new(repo_config: IndexMap<String, PhpMixed>, io: Box<dyn IOInterface>) -> anyhow::Result<Self> {
        if !extension_loaded("zip") {
            return Err(RuntimeException {
                message: "The artifact repository requires PHP's zip extension".to_string(),
                code: 0,
            }
            .into());
        }

        let url = repo_config["url"].as_string().unwrap_or("").to_string();
        let lookup = Platform::expand_path(&url);
        Ok(Self {
            inner: ArrayRepository::new(),
            loader: Box::new(ArrayLoader::new()),
            lookup,
            repo_config,
            io,
        })
    }

    pub fn get_repo_name(&self) -> String {
        format!("artifact repo ({})", self.lookup)
    }

    fn initialize(&mut self) -> anyhow::Result<()> {
        self.inner.initialize()?;
        let lookup = self.lookup.clone();
        self.scan_directory(&lookup)
    }

    fn scan_directory(&mut self, path: &str) -> anyhow::Result<()> {
        let entries = std::fs::read_dir(path)?;
        for entry in entries {
            let entry = entry?;
            let file_path = entry.path();

            if file_path.is_symlink() {
                let resolved = std::fs::canonicalize(&file_path)?;
                if resolved.is_dir() {
                    self.scan_directory(resolved.to_str().unwrap_or(""))?;
                    continue;
                }
            }

            if file_path.is_dir() {
                self.scan_directory(file_path.to_str().unwrap_or(""))?;
                continue;
            }

            if !file_path.is_file() {
                continue;
            }

            let ext = file_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if !matches!(ext.as_str(), "zip" | "tar" | "gz" | "tgz") {
                continue;
            }

            let package = self.get_composer_information(&file_path)?;
            let basename = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            match package {
                None => {
                    self.io.write_error(
                        &format!("File <comment>{}</comment> doesn't seem to hold a package", basename),
                        true,
                        IOInterface::VERBOSE,
                    );
                }
                Some(package) => {
                    self.io.write_error(
                        &format!(
                            "Found package <info>{}</info> (<comment>{}</comment>) in file <info>{}</info>",
                            package.get_name(),
                            package.get_pretty_version(),
                            basename,
                        ),
                        true,
                        IOInterface::VERBOSE,
                    );
                    self.inner.add_package(package);
                }
            }
        }
        Ok(())
    }

    fn get_composer_information(&self, file: &Path) -> anyhow::Result<Option<Box<BasePackage>>> {
        let mut json: Option<String> = None;
        let file_extension = file
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let file_type: &str;
        if matches!(file_extension.as_str(), "gz" | "tar" | "tgz") {
            file_type = "tar";
        } else if file_extension == "zip" {
            file_type = "zip";
        } else {
            return Err(RuntimeException {
                message: format!(
                    "Files with \"{}\" extensions aren't supported. Only ZIP and TAR/TAR.GZ/TGZ archives are supported.",
                    file_extension
                ),
                code: 0,
            }
            .into());
        }

        let pathname = file.to_str().unwrap_or("");
        let get_result = if file_type == "tar" {
            Tar::get_composer_json(pathname)
        } else {
            Zip::get_composer_json(pathname)
        };
        match get_result {
            Ok(j) => json = j,
            Err(exception) => {
                self.io.write(
                    &format!("Failed loading package {}: {}", pathname, exception),
                    false,
                    IOInterface::VERBOSE,
                );
            }
        }

        if json.is_none() {
            return Ok(None);
        }

        let mut package = JsonFile::parse_json(&json.unwrap(), &format!("{}#composer.json", pathname))?;
        let url_normalized = pathname.replace('\\', '/');
        let real_path = file
            .canonicalize()
            .ok()
            .and_then(|p| p.to_str().map(|s| s.to_string()))
            .unwrap_or_default();
        let shasum = hash_file("sha1", &real_path).unwrap_or_default();

        let mut dist = IndexMap::new();
        dist.insert("type".to_string(), Box::new(PhpMixed::String(file_type.to_string())));
        dist.insert("url".to_string(), Box::new(PhpMixed::String(url_normalized)));
        dist.insert("shasum".to_string(), Box::new(PhpMixed::String(shasum)));
        package.insert("dist".to_string(), Box::new(PhpMixed::Array(dist)));

        match self.loader.load(package, None) {
            Ok(package) => Ok(Some(package)),
            Err(exception) => Err(UnexpectedValueException {
                message: format!("Failed loading package in {}: {}", pathname, exception),
                code: 0,
            }
            .into()),
        }
    }
}

impl ConfigurableRepositoryInterface for ArtifactRepository {
    fn get_repo_config(&self) -> IndexMap<String, PhpMixed> {
        self.repo_config.clone()
    }
}
