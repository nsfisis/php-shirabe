//! ref: composer/src/Composer/Downloader/ArchiveDownloader.php

use crate::dependency_resolver::operation::InstallOperation;
use crate::downloader::DownloaderInterface;
use crate::downloader::FileDownloader;
use crate::io::IOInterfaceImmutable;
use crate::package::PackageInterfaceHandle;
use crate::util::Filesystem;
use crate::util::Platform;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::finder::Finder;
use shirabe_php_shim::{
    DIRECTORY_SEPARATOR, PhpMixed, RuntimeException, bin2hex, file_exists, is_dir, random_bytes,
    realpath,
};
use std::path::{Path, PathBuf};

pub trait ArchiveDownloader {
    fn inner(&self) -> &FileDownloader;
    fn cleanup_executed(&self) -> &std::cell::RefCell<IndexMap<String, bool>>;

    async fn extract(
        &self,
        package: PackageInterfaceHandle,
        file: &str,
        path: &str,
    ) -> anyhow::Result<Option<PhpMixed>>;

    async fn prepare(
        &self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.cleanup_executed()
            .borrow_mut()
            .shift_remove(&package.get_name());
        self.inner().prepare(r#type, package, path, prev_package).await
    }

    async fn cleanup(
        &self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.cleanup_executed()
            .borrow_mut()
            .insert(package.get_name(), true);
        self.inner().cleanup(r#type, package, path, prev_package).await
    }

    /// @inheritDoc
    ///
    /// @throws \RuntimeException
    /// @throws \UnexpectedValueException
    async fn install(
        &self,
        package: PackageInterfaceHandle,
        path: &str,
        output: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        if output {
            self.inner().io.borrow().write_error(&format!(
                "  - {}{}",
                InstallOperation::format(package.clone(), false),
                self.get_install_operation_appendix(package.clone(), path)
            ));
        }

        let vendor_dir = self
            .inner()
            .config
            .borrow_mut()
            .get("vendor-dir")
            .as_string()
            .unwrap_or("")
            .to_string();

        // clean up the target directory, unless it contains the vendor dir, as the vendor dir contains
        // the archive to be extracted. This is the case when installing with create-project in the current directory
        // but in that case we ensure the directory is empty already in ProjectInstaller so no need to empty it here.
        if !self
            .inner()
            .filesystem
            .borrow()
            .normalize_path(&vendor_dir)
            .contains(
                &self
                    .inner()
                    .filesystem
                    .borrow()
                    .normalize_path(&format!("{}{}", path, DIRECTORY_SEPARATOR)),
            )
        {
            self.inner()
                .filesystem
                .borrow_mut()
                .empty_directory(path, true);
        }

        let temporary_dir = loop {
            let candidate = format!("{}/composer/{}", vendor_dir, bin2hex(&random_bytes(4)));
            if !is_dir(&candidate) {
                break candidate;
            }
        };

        self.inner().add_cleanup_path(package.clone(), &temporary_dir);
        // avoid cleaning up $path if installing in "." for eg create-project as we can not
        // delete the directory we are currently in on windows
        if !is_dir(path) || realpath(path) != Some(Platform::get_cwd(false).unwrap_or_default()) {
            self.inner().add_cleanup_path(package.clone(), path);
        }

        self.inner()
            .filesystem
            .borrow_mut()
            .ensure_directory_exists(&temporary_dir);
        let file_name = self.inner().get_file_name(package.clone(), path);

        match self
            .extract(package.clone(), &file_name, &temporary_dir)
            .await
        {
            Err(e) => {
                install_cleanup(self.inner(), package.clone(), path, &temporary_dir)?;
                Err(e)
            }
            Ok(_) => {
                if file_exists(&file_name) {
                    self.inner().filesystem.borrow().unlink(&file_name)?;
                }

                let mut rename_as_one = false;
                if !file_exists(path) {
                    rename_as_one = true;
                } else if self.inner().filesystem.borrow().is_dir_empty(path) {
                    let removed = self
                        .inner()
                        .filesystem
                        .borrow_mut()
                        .remove_directory_php(path);
                    match removed {
                        Ok(true) => {
                            rename_as_one = true;
                        }
                        Ok(false) => {}
                        Err(e) => {
                            // ignore error, and simply do not renameAsOne
                            if e.downcast_ref::<RuntimeException>().is_none() {
                                return Err(e);
                            }
                        }
                    }
                }

                let content_dir = get_folder_content(&temporary_dir);
                let single_dir_at_top_level =
                    content_dir.len() == 1 && content_dir.first().map(is_dir).unwrap_or(false);

                if rename_as_one {
                    // if the target $path is clear, we can rename the whole package in one go instead of looping over the contents
                    let extracted_dir: PathBuf = if single_dir_at_top_level {
                        content_dir.first().unwrap().clone()
                    } else {
                        PathBuf::from(&temporary_dir)
                    };
                    self.inner()
                        .filesystem
                        .borrow_mut()
                        .rename(&extracted_dir, path)?;
                } else {
                    // only one dir in the archive, extract its contents out of it
                    let mut from = PathBuf::from(&temporary_dir);
                    if single_dir_at_top_level {
                        from = content_dir.first().unwrap().clone();
                    }

                    rename_recursively(
                        &self.inner().filesystem,
                        package.clone(),
                        &from,
                        Path::new(path),
                    )?;
                }

                Filesystem::remove_directory_async_via(&self.inner().filesystem, &temporary_dir)
                    .await?;
                self.inner().remove_cleanup_path(package.clone(), &temporary_dir);
                self.inner().remove_cleanup_path(package, path);

                Ok(None)
            }
        }
    }

    /// @inheritDoc
    fn get_install_operation_appendix(
        &self,
        _package: PackageInterfaceHandle,
        _path: &str,
    ) -> &str {
        ": Extracting archive"
    }
}

fn install_cleanup(
    inner: &FileDownloader,
    package: PackageInterfaceHandle,
    path: &str,
    temporary_dir: &str,
) -> anyhow::Result<()> {
    // remove cache if the file was corrupted
    inner.clear_last_cache_write(package.clone());

    // clean up
    inner
        .filesystem
        .borrow_mut()
        .remove_directory(temporary_dir)?;
    if is_dir(path) && realpath(path) != Some(Platform::get_cwd(false).unwrap_or_default()) {
        inner.filesystem.borrow_mut().remove_directory(path)?;
    }
    inner.remove_cleanup_path(package.clone(), temporary_dir);
    let realpath = realpath(path);
    if let Some(realpath) = realpath {
        inner.remove_cleanup_path(package, &realpath);
    }

    Ok(())
}

/// Returns the folder content, excluding .DS_Store
fn get_folder_content(dir: impl AsRef<Path>) -> Vec<PathBuf> {
    let mut finder = Finder::create();
    finder
        .ignore_vcs(false)
        .ignore_dot_files(false)
        .not_name(".DS_Store")
        .depth(0)
        .r#in(dir.as_ref());

    finder.iter().collect()
}

/// Renames (and recursively merges if needed) a folder into another one
///
/// For custom installers, where packages may share paths, and given Composer 2's parallelism, we need to make sure
/// that the source directory gets merged into the target one if the target exists. Otherwise rename() by default would
/// put the source into the target e.g. src/ => target/src/ (assuming target exists) instead of src/ => target/
fn rename_recursively(
    filesystem: &std::rc::Rc<std::cell::RefCell<Filesystem>>,
    package: PackageInterfaceHandle,
    from: &Path,
    to: &Path,
) -> anyhow::Result<()> {
    let content_dir = get_folder_content(from);

    // move files back out of the temp dir
    for file in &content_dir {
        let target = to.join(
            file.file_name()
                .expect("Finder always yields entries with a file name"),
        );
        if is_dir(&target) {
            if !is_dir(file) {
                return Err(RuntimeException {
                    message: format!(
                        "Installing {} would lead to overwriting the {} directory with a file from the package, invalid operation.",
                        package,
                        target.display()
                    ),
                    code: 0,
                }
                .into());
            }
            rename_recursively(filesystem, package.clone(), file, &target)?;
        } else {
            filesystem.borrow_mut().rename(file, &target)?;
        }
    }

    Ok(())
}
