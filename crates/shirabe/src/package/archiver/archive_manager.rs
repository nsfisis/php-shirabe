//! ref: composer/src/Composer/Package/Archiver/ArchiveManager.php sha1:b6fd583e4dcb3b3bb4b0bd49437949771e66d7cd

use crate::downloader::DownloadManager;
use crate::json::JsonFile;
use crate::package::CompletePackageInterfaceHandle;
use crate::package::archiver::{ArchiverInterface, PharArchiver, ZipArchiver};
use crate::util::r#loop::Loop;
use crate::util::{Filesystem, SyncHelper};
use bstr::{BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{
    InvalidArgumentException, RuntimeException, bconcat, bin2hex, file_exists, hash, implode,
    path_from_bytes, php_regex, preg_is_match, preg_replace, random_bytes, realpath, str_replace,
    sys_get_temp_dir,
};
use std::path::PathBuf;

pub struct ArchiveManager {
    download_manager: std::rc::Rc<std::cell::RefCell<DownloadManager>>,
    r#loop: std::rc::Rc<std::cell::RefCell<Loop>>,
    archivers: Vec<Box<dyn ArchiverInterface>>,
    overwrite_files: bool,
}

impl std::fmt::Debug for ArchiveManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArchiveManager")
            .field("overwrite_files", &self.overwrite_files)
            .finish()
    }
}

impl ArchiveManager {
    pub fn new(
        download_manager: std::rc::Rc<std::cell::RefCell<DownloadManager>>,
        r#loop: std::rc::Rc<std::cell::RefCell<Loop>>,
    ) -> Self {
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

    pub fn set_overwrite_files(&mut self, overwrite_files: bool) {
        self.overwrite_files = overwrite_files;
    }

    pub fn get_package_filename_parts(
        &self,
        package: CompletePackageInterfaceHandle,
    ) -> anyhow::Result<IndexMap<BString, BString>> {
        let base_name = match package.get_archive_name() {
            Some(name) => name,
            None => preg_replace(php_regex!("#[^a-z0-9-_]#i"), "-", package.get_name()),
        };

        let mut parts: IndexMap<BString, BString> = IndexMap::new();
        parts.insert(BString::from("base"), base_name);

        let dist_reference = package.get_dist_reference();
        if let Some(ref dist_ref) = dist_reference {
            if preg_is_match(php_regex!("{^[a-f0-9]{40}$}"), dist_ref) {
                parts.insert(BString::from("dist_reference"), dist_ref.clone());
                if let Some(dist_type) = package.get_dist_type() {
                    parts.insert(BString::from("dist_type"), dist_type);
                }
            } else {
                parts.insert(BString::from("version"), package.get_pretty_version());
                parts.insert(BString::from("dist_reference"), dist_ref.clone());
            }
        } else {
            parts.insert(BString::from("version"), package.get_pretty_version());
        }

        if let Some(source_reference) = package.get_source_reference() {
            let hash = hash("sha1", &source_reference);
            parts.insert(BString::from("source_reference"), BString::from(&hash[..6]));
        }

        // array_filter removed null values; replace '/' with '-' in each value
        for val in parts.values_mut() {
            *val = str_replace("/", "-", &*val);
        }

        Ok(parts)
    }

    pub fn get_package_filename_from_parts(&self, parts: &IndexMap<BString, BString>) -> BString {
        let values: Vec<&BString> = parts.values().collect();
        implode("-", &values)
    }

    pub fn get_package_filename(
        &self,
        package: CompletePackageInterfaceHandle,
    ) -> anyhow::Result<BString> {
        let parts = self.get_package_filename_parts(package)?;
        Ok(self.get_package_filename_from_parts(&parts))
    }

    pub async fn archive(
        &mut self,
        package: CompletePackageInterfaceHandle,
        format: BString,
        target_dir: PathBuf,
        file_name: Option<BString>,
        ignore_filters: bool,
    ) -> anyhow::Result<PathBuf> {
        if format.is_empty() {
            return Err(
                InvalidArgumentException::new(BString::from("Format must be specified")).into(),
            );
        }

        let mut usable_archiver_idx: Option<usize> = None;
        for (i, archiver) in self.archivers.iter().enumerate() {
            if archiver.supports(
                format.as_bstr(),
                package.get_source_type().as_ref().map(|s| s.as_bstr()),
            ) {
                usable_archiver_idx = Some(i);
                break;
            }
        }

        let usable_archiver_idx = match usable_archiver_idx {
            Some(i) => i,
            None => {
                return Err(RuntimeException::new(bconcat!(
                    "No archiver found to support ",
                    format,
                    " format"
                ))
                .into());
            }
        };

        let mut filesystem = Filesystem::new(None);

        let is_root = package.as_rc().borrow().is_root();
        let source_path: PathBuf;

        if is_root {
            source_path = realpath(".").unwrap_or_else(|| PathBuf::from("."));
        } else {
            let random_suffix = bin2hex(random_bytes(5));
            source_path = path_from_bytes(&bconcat!(
                sys_get_temp_dir(),
                "/composer_archive",
                random_suffix
            ))
            .to_path_buf();
            filesystem.ensure_directory_exists(&source_path)?;

            let download_result: anyhow::Result<()> = async {
                SyncHelper::r#await(
                    &self.r#loop,
                    Some(Box::pin(async {
                        self.download_manager
                            .borrow()
                            .download(package.clone().into(), &source_path, None)
                            .await
                            .map(|_| ())
                    })),
                )
                .await?;
                SyncHelper::r#await(
                    &self.r#loop,
                    Some(Box::pin(async {
                        self.download_manager
                            .borrow()
                            .install(package.clone().into(), &source_path)
                            .await
                            .map(|_| ())
                    })),
                )
                .await?;
                Ok(())
            }
            .await;

            if let Err(e) = download_result {
                filesystem.remove_directory(&source_path)?;
                return Err(e);
            }

            let composer_json_path = bconcat!(&source_path, "/composer.json");
            let composer_json_path = path_from_bytes(&composer_json_path);
            if file_exists(composer_json_path) {
                let json_file = JsonFile::new(composer_json_path, None, None)?;
                let json_data = json_file.read()?;
                if let Some(archive) = json_data.get("archive") {
                    if let Some(name) = archive.get("name").and_then(|v| v.as_string())
                        && !name.is_empty()
                    {
                        package.set_archive_name(BString::from(name));
                    }
                    if let Some(exclude) = archive.get("exclude")
                        && let Some(excludes) = exclude.as_array()
                    {
                        let excludes: Vec<BString> = excludes
                            .values()
                            .filter_map(|v| v.as_string().map(BString::from))
                            .collect();
                        if !excludes.is_empty() {
                            package.set_archive_excludes(excludes);
                        }
                    }
                }
            }
        }

        let supported_formats = self.get_supported_formats();
        let package_name_parts = match file_name {
            None => self.get_package_filename_parts(package.clone())?,
            Some(f) => {
                let mut parts = IndexMap::new();
                parts.insert(BString::from("base"), f);
                parts
            }
        };

        let package_name = self.get_package_filename_from_parts(&package_name_parts);
        let exclude_patterns = self.build_exclude_patterns(&package_name_parts, &supported_formats);

        filesystem.ensure_directory_exists(&target_dir)?;
        let target = path_from_bytes(&bconcat!(
            realpath(&target_dir).unwrap_or_else(|| target_dir.clone()),
            "/",
            package_name,
            ".",
            &format
        ))
        .to_path_buf();
        if let Some(parent) = target.parent() {
            filesystem.ensure_directory_exists(parent)?;
        }

        if !self.overwrite_files && file_exists(&target) {
            return Ok(target);
        }

        let tmp_suffix = bin2hex(random_bytes(5));
        let temp_target = path_from_bytes(&bconcat!(
            sys_get_temp_dir(),
            "/composer_archive",
            tmp_suffix,
            ".",
            &format
        ))
        .to_path_buf();
        if let Some(parent) = temp_target.parent() {
            filesystem.ensure_directory_exists(parent)?;
        }

        let mut all_excludes = exclude_patterns;
        all_excludes.extend(package.get_archive_excludes().iter().cloned());
        let archive_path = self.archivers[usable_archiver_idx].archive(
            &source_path,
            &temp_target,
            format.as_bstr(),
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
        parts: &IndexMap<BString, BString>,
        formats: &[BString],
    ) -> Vec<BString> {
        let mut base = parts[bstr::B("base")].clone();
        if parts.len() > 1 {
            base = bconcat!(&base, "-*");
        }

        let mut patterns = vec![];
        for format in formats {
            patterns.push(bconcat!(&base, ".", format));
        }

        patterns
    }

    fn get_supported_formats(&self) -> Vec<BString> {
        let mut formats: Vec<BString> = vec![];
        for archiver in &self.archivers {
            let items: Vec<BString> = if archiver.as_any().is::<ZipArchiver>() {
                vec![BString::from("zip")]
            } else if archiver.as_any().is::<PharArchiver>() {
                vec![
                    BString::from("zip"),
                    BString::from("tar"),
                    BString::from("tar.gz"),
                    BString::from("tar.bz2"),
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

// Composer's Composer::setArchiveManager() accepts any ArchiveManager subclass, so plugins may
// swap in a replacement. The interface captures the methods reached through Composer's accessor.
#[async_trait::async_trait(?Send)]
pub trait ArchiveManagerInterface: std::fmt::Debug {
    async fn archive(
        &mut self,
        package: CompletePackageInterfaceHandle,
        format: BString,
        target_dir: PathBuf,
        file_name: Option<BString>,
        ignore_filters: bool,
    ) -> anyhow::Result<PathBuf>;
}

#[async_trait::async_trait(?Send)]
impl ArchiveManagerInterface for ArchiveManager {
    async fn archive(
        &mut self,
        package: CompletePackageInterfaceHandle,
        format: BString,
        target_dir: PathBuf,
        file_name: Option<BString>,
        ignore_filters: bool,
    ) -> anyhow::Result<PathBuf> {
        self.archive(package, format, target_dir, file_name, ignore_filters)
            .await
    }
}
