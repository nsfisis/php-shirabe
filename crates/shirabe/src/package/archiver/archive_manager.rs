//! ref: composer/src/Composer/Package/Archiver/ArchiveManager.php

use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{
    InvalidArgumentException, RuntimeException, bin2hex, file_exists, random_bytes, realpath,
    sys_get_temp_dir,
};

use crate::downloader::download_manager::DownloadManager;
use crate::json::json_file::JsonFile;
use crate::package::archiver::archiver_interface::ArchiverInterface;
use crate::package::archiver::phar_archiver::PharArchiver;
use crate::package::archiver::zip_archiver::ZipArchiver;
use crate::package::complete_package_interface::CompletePackageInterface;
use crate::package::root_package_interface::RootPackageInterface;
use crate::util::filesystem::Filesystem;
use crate::util::r#loop::Loop;
use crate::util::sync_helper::SyncHelper;

pub struct ArchiveManager {
    pub(crate) download_manager: DownloadManager,
    pub(crate) r#loop: Loop,
    pub(crate) archivers: Vec<Box<dyn ArchiverInterface>>,
    pub(crate) overwrite_files: bool,
}

impl std::fmt::Debug for ArchiveManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArchiveManager")
            .field("overwrite_files", &self.overwrite_files)
            .finish()
    }
}

impl ArchiveManager {
    pub fn new(download_manager: DownloadManager, r#loop: Loop) -> Self {
        Self {
            download_manager,
            r#loop,
            archivers: vec![],
            overwrite_files: true,
        }
    }

    pub fn add_archiver(&mut self, archiver: Box<dyn ArchiverInterface>) {
        self.archivers.push(archiver);
    }

    pub fn set_overwrite_files(&mut self, overwrite_files: bool) -> &mut Self {
        self.overwrite_files = overwrite_files;
        self
    }

    pub fn get_package_filename_parts(
        &self,
        package: &dyn CompletePackageInterface,
    ) -> IndexMap<String, String> {
        let base_name = match package.get_archive_name() {
            Some(name) => name.to_string(),
            None => Preg::replace("#[^a-z0-9-_]#i", "-", package.get_name()),
        };

        let mut parts: IndexMap<String, String> = IndexMap::new();
        parts.insert("base".to_string(), base_name);

        let dist_reference = package.get_dist_reference();
        if let Some(ref dist_ref) = dist_reference {
            if Preg::is_match("{^[a-f0-9]{40}$}", dist_ref).unwrap_or(false) {
                parts.insert("dist_reference".to_string(), dist_ref.clone());
                if let Some(dist_type) = package.get_dist_type() {
                    parts.insert("dist_type".to_string(), dist_type.to_string());
                }
            } else {
                parts.insert(
                    "version".to_string(),
                    package.get_pretty_version().to_string(),
                );
                parts.insert("dist_reference".to_string(), dist_ref.clone());
            }
        } else {
            parts.insert(
                "version".to_string(),
                package.get_pretty_version().to_string(),
            );
        }

        if let Some(source_reference) = package.get_source_reference() {
            let hash = shirabe_php_shim::hash("sha1", source_reference);
            parts.insert("source_reference".to_string(), hash[..6].to_string());
        }

        // array_filter removed null values; replace '/' with '-' in each value
        for val in parts.values_mut() {
            *val = val.replace('/', '-');
        }

        parts
    }

    pub fn get_package_filename_from_parts(&self, parts: &IndexMap<String, String>) -> String {
        let values: Vec<&str> = parts.values().map(|s| s.as_str()).collect();
        values.join("-")
    }

    pub fn get_package_filename(&self, package: &dyn CompletePackageInterface) -> String {
        let parts = self.get_package_filename_parts(package);
        self.get_package_filename_from_parts(&parts)
    }

    pub fn archive(
        &mut self,
        package: &mut dyn CompletePackageInterface,
        format: String,
        target_dir: String,
        file_name: Option<String>,
        ignore_filters: bool,
    ) -> anyhow::Result<String> {
        if format.is_empty() {
            return Err(anyhow::anyhow!(InvalidArgumentException {
                message: "Format must be specified".to_string(),
                code: 0,
            }));
        }

        let mut usable_archiver_idx: Option<usize> = None;
        for (i, archiver) in self.archivers.iter().enumerate() {
            if archiver.supports(
                format.clone(),
                package.get_source_type().map(|s| s.to_string()),
            ) {
                usable_archiver_idx = Some(i);
                break;
            }
        }

        let usable_archiver_idx = match usable_archiver_idx {
            Some(i) => i,
            None => {
                return Err(anyhow::anyhow!(RuntimeException {
                    message: format!("No archiver found to support {} format", format),
                    code: 0,
                }));
            }
        };

        let filesystem = Filesystem::new();

        let is_root = package.as_any().is::<dyn RootPackageInterface>();
        let source_path: String;

        if is_root {
            source_path = realpath(".").unwrap_or_else(|| ".".to_string());
        } else {
            let tmp_dir = sys_get_temp_dir();
            let random_suffix = bin2hex(&random_bytes(5));
            source_path = format!("{}/composer_archive{}", tmp_dir, random_suffix);
            filesystem.ensure_directory_exists(&source_path)?;

            let download_result = (|| -> anyhow::Result<()> {
                let promise = self.download_manager.download(package, &source_path)?;
                SyncHelper::r#await(&self.r#loop, promise)?;
                let promise = self.download_manager.install(package, &source_path)?;
                SyncHelper::r#await(&self.r#loop, promise)?;
                Ok(())
            })();

            if let Err(e) = download_result {
                filesystem.remove_directory(&source_path)?;
                return Err(e);
            }

            let composer_json_path = format!("{}/composer.json", source_path);
            if file_exists(&composer_json_path) {
                let json_file = JsonFile::new(composer_json_path, None, None);
                let json_data = json_file.read()?;
                if let Some(archive) = json_data.get("archive") {
                    if let Some(name) = archive.get("name").and_then(|v| v.as_str()) {
                        if !name.is_empty() {
                            package.set_archive_name(name.to_string());
                        }
                    }
                    if let Some(exclude) = archive.get("exclude") {
                        if let Some(excludes) = exclude.as_array() {
                            let excludes: Vec<String> = excludes
                                .iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect();
                            if !excludes.is_empty() {
                                package.set_archive_excludes(excludes);
                            }
                        }
                    }
                }
            }
        }

        let supported_formats = self.get_supported_formats();
        let package_name_parts = match file_name {
            None => self.get_package_filename_parts(package),
            Some(f) => {
                let mut parts = IndexMap::new();
                parts.insert("base".to_string(), f);
                parts
            }
        };

        let package_name = self.get_package_filename_from_parts(&package_name_parts);
        let exclude_patterns = self.build_exclude_patterns(&package_name_parts, &supported_formats);

        filesystem.ensure_directory_exists(&target_dir)?;
        let target = format!(
            "{}/{}.{}",
            realpath(&target_dir).unwrap_or(target_dir.clone()),
            package_name,
            format
        );
        if let Some(parent) = std::path::Path::new(&target).parent() {
            filesystem.ensure_directory_exists(parent.to_str().unwrap_or(""))?;
        }

        if !self.overwrite_files && file_exists(&target) {
            return Ok(target);
        }

        let tmp_suffix = bin2hex(&random_bytes(5));
        let temp_target = format!(
            "{}/composer_archive{}.{}",
            sys_get_temp_dir(),
            tmp_suffix,
            format
        );
        if let Some(parent) = std::path::Path::new(&temp_target).parent() {
            filesystem.ensure_directory_exists(parent.to_str().unwrap_or(""))?;
        }

        let mut all_excludes = exclude_patterns;
        all_excludes.extend(package.get_archive_excludes());
        let archive_path = self.archivers[usable_archiver_idx].archive(
            source_path.clone(),
            temp_target.clone(),
            format,
            all_excludes,
            ignore_filters,
        )?;
        filesystem.rename(&archive_path, &target)?;

        if !is_root {
            filesystem.remove_directory(&source_path)?;
        }
        filesystem.remove(&temp_target)?;

        Ok(target)
    }

    fn build_exclude_patterns(
        &self,
        parts: &IndexMap<String, String>,
        formats: &[String],
    ) -> Vec<String> {
        let mut base = parts["base"].clone();
        if parts.len() > 1 {
            base.push_str("-*");
        }

        let mut patterns = vec![];
        for format in formats {
            patterns.push(format!("{}.{}", base, format));
        }

        patterns
    }

    fn get_supported_formats(&self) -> Vec<String> {
        let mut formats: Vec<String> = vec![];
        for archiver in &self.archivers {
            let items: Vec<String> = if archiver.as_any().is::<ZipArchiver>() {
                vec!["zip".to_string()]
            } else if archiver.as_any().is::<PharArchiver>() {
                vec![
                    "zip".to_string(),
                    "tar".to_string(),
                    "tar.gz".to_string(),
                    "tar.bz2".to_string(),
                ]
            } else {
                vec![]
            };
            formats.extend(items);
        }

        formats.dedup();
        formats
    }
}
