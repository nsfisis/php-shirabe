//! ref: composer/src/Composer/Repository/ArtifactRepository.php

use crate::io::io_interface;
use std::path::Path;

use indexmap::IndexMap;
use shirabe_php_shim::{
    PhpMixed, RuntimeException, UnexpectedValueException, extension_loaded, hash_file,
};

use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::json::JsonFile;
use crate::package::BasePackage;
use crate::package::loader::ArrayLoader;
use crate::package::loader::LoaderInterface;
use crate::repository::ArrayRepository;
use crate::repository::ConfigurableRepositoryInterface;
use crate::util::Platform;
use crate::util::Tar;
use crate::util::Zip;

pub struct ArtifactRepository {
    inner: ArrayRepository,
    pub(crate) loader: Box<dyn LoaderInterface>,
    pub(crate) lookup: String,
    pub(crate) repo_config: IndexMap<String, PhpMixed>,
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
        repo_config: IndexMap<String, PhpMixed>,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> anyhow::Result<Self> {
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
            inner: ArrayRepository::new(Vec::new())?,
            loader: Box::new(ArrayLoader::new(None, true)),
            lookup,
            repo_config,
            io,
        })
    }

    pub fn get_repo_name(&self) -> String {
        format!("artifact repo ({})", self.lookup)
    }

    fn initialize(&mut self) -> anyhow::Result<()> {
        self.inner.initialize();
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
                        package.get_name(),
                        package.get_pretty_version(),
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
        dist.insert("type".to_string(), PhpMixed::String(file_type.to_string()));
        dist.insert("url".to_string(), PhpMixed::String(url_normalized));
        dist.insert("shasum".to_string(), PhpMixed::String(shasum));
        if let Some(arr) = package.as_array_mut() {
            arr.insert("dist".to_string(), PhpMixed::Array(dist));
        }

        let cfg: IndexMap<String, PhpMixed> = package
            .as_array()
            .cloned()
            .map(|m| m.into_iter().collect())
            .unwrap_or_default();
        match self.loader.load(cfg, None) {
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
