//! ref: composer/src/Composer/Downloader/ArchiveDownloader.php

use crate::dependency_resolver::operation::install_operation::InstallOperation;
use crate::downloader::file_downloader::FileDownloader;
use crate::package::package_interface::PackageInterface;
use crate::util::platform::Platform;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use shirabe_external_packages::symfony::component::finder::finder::Finder;
use shirabe_php_shim::{
    DIRECTORY_SEPARATOR, RuntimeException, bin2hex, file_exists, is_dir, random_bytes, realpath,
};

#[derive(Debug)]
pub struct ArchiveDownloader {
    pub(crate) inner: FileDownloader,
    pub(crate) cleanup_executed: IndexMap<String, bool>,
}

impl ArchiveDownloader {
    pub fn prepare(
        &mut self,
        r#type: &str,
        package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Box<dyn PromiseInterface>> {
        self.cleanup_executed.remove(package.get_name());

        self.inner.prepare(r#type, package, path, prev_package)
    }

    pub fn cleanup(
        &mut self,
        r#type: &str,
        package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Box<dyn PromiseInterface>> {
        self.cleanup_executed
            .insert(package.get_name().to_string(), true);

        self.inner.cleanup(r#type, package, path, prev_package)
    }

    pub fn install(
        &mut self,
        package: &dyn PackageInterface,
        path: &str,
        output: bool,
    ) -> Result<Box<dyn PromiseInterface>> {
        if output {
            self.inner.io.write_error(&format!(
                "  - {}{}",
                InstallOperation::format(package, false),
                self.get_install_operation_appendix(package, path)
            ));
        }

        let vendor_dir = self.inner.config.get("vendor-dir");

        // clean up the target directory, unless it contains the vendor dir, as the vendor dir contains
        // the archive to be extracted. This is the case when installing with create-project in the current directory
        // but in that case we ensure the directory is empty already in ProjectInstaller so no need to empty it here.
        if !self.inner.filesystem.normalize_path(&vendor_dir).contains(
            &self
                .inner
                .filesystem
                .normalize_path(&format!("{}{}", path, DIRECTORY_SEPARATOR)),
        ) {
            self.inner.filesystem.empty_directory(path);
        }

        let temporary_dir;
        loop {
            temporary_dir = format!("{}/composer/{}", vendor_dir, bin2hex(&random_bytes(4)));
            if !is_dir(&temporary_dir) {
                break;
            }
        }

        self.inner.add_cleanup_path(package, &temporary_dir);
        // avoid cleaning up $path if installing in "." for eg create-project as we can not
        // delete the directory we are currently in on windows
        if !is_dir(path) || realpath(path) != Platform::get_cwd() {
            self.inner.add_cleanup_path(package, path);
        }

        self.inner
            .filesystem
            .ensure_directory_exists(&temporary_dir);
        let file_name = self.inner.get_file_name(package, path);

        let filesystem = &self.inner.filesystem;

        let cleanup = move || {
            // remove cache if the file was corrupted
            self.inner.clear_last_cache_write(package);

            // clean up
            filesystem.remove_directory(&temporary_dir);
            if is_dir(path) && realpath(path) != Platform::get_cwd() {
                filesystem.remove_directory(path);
            }
            self.inner.remove_cleanup_path(package, &temporary_dir);
            let realpath_result = realpath(path);
            if let Some(realpath_val) = realpath_result {
                self.inner.remove_cleanup_path(package, &realpath_val);
            }
        };

        let promise = match self.extract(package, &file_name, &temporary_dir) {
            Ok(p) => p,
            Err(e) => {
                cleanup();
                return Err(e);
            }
        };

        Ok(promise.then(
            Box::new(move || -> Result<Box<dyn PromiseInterface>> {
                if file_exists(&file_name) {
                    filesystem.unlink(&file_name);
                }

                let get_folder_content = |dir: &str| -> Vec<std::path::PathBuf> {
                    let finder = Finder::create()
                        .ignore_vcs(false)
                        .ignore_dot_files(false)
                        .not_name(".DS_Store")
                        .depth(0)
                        .in_(dir);

                    finder.into_iter().collect()
                };

                let mut rename_recursively: Option<Box<dyn Fn(&str, &str) -> Result<()>>> = None;
                // Renames (and recursively merges if needed) a folder into another one
                //
                // For custom installers, where packages may share paths, and given Composer 2's parallelism, we need to make sure
                // that the source directory gets merged into the target one if the target exists. Otherwise rename() by default would
                // put the source into the target e.g. src/ => target/src/ (assuming target exists) instead of src/ => target/
                rename_recursively = Some(Box::new(move |from: &str, to: &str| -> Result<()> {
                    let content_dir = get_folder_content(from);

                    // move files back out of the temp dir
                    for file in &content_dir {
                        let file = file.to_string_lossy().to_string();
                        let file_basename = shirabe_php_shim::basename(&file);
                        if is_dir(&format!("{}/{}", to, file_basename)) {
                            if !is_dir(&file) {
                                return Err(RuntimeException {
                                    message: format!("Installing {} would lead to overwriting the {}/{} directory with a file from the package, invalid operation.", package, to, file_basename),
                                    code: 0,
                                }.into());
                            }
                            rename_recursively.as_ref().unwrap()(&file, &format!("{}/{}", to, file_basename))?;
                        } else {
                            filesystem.rename(&file, &format!("{}/{}", to, file_basename));
                        }
                    }

                    Ok(())
                }));

                let mut rename_as_one = false;
                if !file_exists(path) {
                    rename_as_one = true;
                } else if filesystem.is_dir_empty(path) {
                    match filesystem.remove_directory_php(path) {
                        Ok(true) => {
                            rename_as_one = true;
                        }
                        _ => {
                            // ignore error, and simply do not renameAsOne
                        }
                    }
                }

                let content_dir = get_folder_content(&temporary_dir);
                let single_dir_at_top_level = content_dir.len() == 1 && is_dir(&content_dir[0].to_string_lossy().to_string());

                if rename_as_one {
                    // if the target $path is clear, we can rename the whole package in one go instead of looping over the contents
                    let extracted_dir = if single_dir_at_top_level {
                        content_dir[0].to_string_lossy().to_string()
                    } else {
                        temporary_dir.clone()
                    };
                    filesystem.rename(&extracted_dir, path);
                } else {
                    // only one dir in the archive, extract its contents out of it
                    let from = if single_dir_at_top_level {
                        content_dir[0].to_string_lossy().to_string()
                    } else {
                        temporary_dir.clone()
                    };

                    rename_recursively.as_ref().unwrap()(&from, path)?;
                }

                let promise = filesystem.remove_directory_async(&temporary_dir);

                Ok(promise.then(
                    Box::new(move || -> Result<()> {
                        self.inner.remove_cleanup_path(package, &temporary_dir);
                        self.inner.remove_cleanup_path(package, path);
                        Ok(())
                    }),
                    None,
                ))
            }),
            Box::new(move |e: anyhow::Error| -> Result<()> {
                cleanup();
                Err(e)
            }),
        ))
    }

    pub fn get_install_operation_appendix(
        &self,
        _package: &dyn PackageInterface,
        _path: &str,
    ) -> &str {
        ": Extracting archive"
    }

    pub(crate) fn extract(
        &self,
        _package: &dyn PackageInterface,
        _file: &str,
        _path: &str,
    ) -> Result<Box<dyn PromiseInterface>> {
        todo!()
    }
}
