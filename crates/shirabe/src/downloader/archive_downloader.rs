//! ref: composer/src/Composer/Downloader/ArchiveDownloader.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use shirabe_external_packages::symfony::component::finder::finder::Finder;
use shirabe_php_shim::{
    DIRECTORY_SEPARATOR, RuntimeException, bin2hex, file_exists, is_dir, random_bytes, realpath,
};

use crate::dependency_resolver::operation::install_operation::InstallOperation;
use crate::downloader::downloader_interface::DownloaderInterface;
use crate::downloader::file_downloader::FileDownloader;
use crate::package::package_interface::PackageInterface;
use crate::util::platform::Platform;

pub trait ArchiveDownloader {
    fn inner(&self) -> &FileDownloader;
    fn inner_mut(&mut self) -> &mut FileDownloader;
    fn cleanup_executed(&self) -> &IndexMap<String, bool>;
    fn cleanup_executed_mut(&mut self) -> &mut IndexMap<String, bool>;

    fn extract(
        &self,
        package: &dyn PackageInterface,
        file: &str,
        path: &str,
    ) -> Result<Box<dyn PromiseInterface>>;

    fn prepare(
        &mut self,
        r#type: &str,
        package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Box<dyn PromiseInterface>> {
        self.cleanup_executed_mut().remove(package.get_name());
        self.inner_mut()
            .prepare(r#type, package, path, prev_package)
    }

    fn cleanup(
        &mut self,
        r#type: &str,
        package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Box<dyn PromiseInterface>> {
        self.cleanup_executed_mut()
            .insert(package.get_name().to_string(), true);
        self.inner_mut()
            .cleanup(r#type, package, path, prev_package)
    }

    /// @inheritDoc
    ///
    /// @throws \RuntimeException
    /// @throws \UnexpectedValueException
    fn install(
        &mut self,
        package: &dyn PackageInterface,
        path: &str,
        output: bool,
    ) -> Result<Box<dyn PromiseInterface>> {
        if output {
            self.inner().io.write_error(&format!(
                "  - {}{}",
                InstallOperation::format(package, false),
                self.get_install_operation_appendix(package, path)
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
            self.inner_mut()
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

        self.inner_mut().add_cleanup_path(package, &temporary_dir);
        // avoid cleaning up $path if installing in "." for eg create-project as we can not
        // delete the directory we are currently in on windows
        if !is_dir(path) || realpath(path) != Some(Platform::get_cwd(false).unwrap_or_default()) {
            self.inner_mut().add_cleanup_path(package, path);
        }

        self.inner_mut()
            .filesystem
            .borrow_mut()
            .ensure_directory_exists(&temporary_dir);
        let file_name = self.inner().get_file_name(package, path);

        let _ = file_name;

        let promise = self.extract(package, "", &temporary_dir)?;

        // TODO(phase-b): the original PHP chains React promise `.then(onFulfilled, onRejected)`
        // callbacks that capture `$this`, `$filesystem`, `$package`, `$path`, `$temporaryDir`,
        // `$fileName`, and a recursive `$renameRecursively` closure. PromiseInterface::then in
        // Rust expects `FnOnce(Option<PhpMixed>) -> Option<PhpMixed>` and the callbacks here
        // need both `&mut self` access and to return another promise. This needs a structural
        // rework (likely splitting the trait or adding a `then_boxed_result` adapter), plus a
        // way to share `&mut self` with the closure (probably `Rc<RefCell<...>>`).
        let _ = (&promise, &temporary_dir, package, path);
        todo!(
            "ArchiveDownloader::install: rewire .then(onFulfilled, onRejected) chain to match PromiseInterface signature"
        )
    }

    /// @inheritDoc
    fn get_install_operation_appendix(&self, _package: &dyn PackageInterface, _path: &str) -> &str {
        ": Extracting archive"
    }
}
