//! ref: composer/src/Composer/Downloader/VcsDownloader.php

use crate::io::io_interface;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_php_shim::{
    InvalidArgumentException, PhpMixed, RuntimeException, array_map, array_shift, count, explode,
    get_class, get_class_err, implode, rawurldecode, realpath, str_replace, strlen, strpos, substr,
    trim,
};

use crate::config::Config;
use crate::dependency_resolver::operation::InstallOperation;
use crate::dependency_resolver::operation::UninstallOperation;
use crate::dependency_resolver::operation::UpdateOperation;
use crate::downloader::ChangeReportInterface;
use crate::downloader::DownloaderInterface;
use crate::downloader::VcsCapableDownloaderInterface;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::package::PackageInterfaceHandle;
use crate::package::dumper::ArrayDumper;
use crate::package::version::VersionGuesser;
use crate::package::version::VersionParser;
use crate::util::Filesystem;
use crate::util::ProcessExecutor;

#[derive(Debug)]
pub struct VcsDownloaderBase {
    pub io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    pub config: std::rc::Rc<std::cell::RefCell<Config>>,
    pub process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    pub filesystem: std::rc::Rc<std::cell::RefCell<Filesystem>>,
    pub has_cleaned_changes: IndexMap<String, bool>,
}

impl VcsDownloaderBase {
    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        process: Option<std::rc::Rc<std::cell::RefCell<ProcessExecutor>>>,
        fs: Option<std::rc::Rc<std::cell::RefCell<Filesystem>>>,
    ) -> Self {
        let process = process.unwrap_or_else(|| {
            std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(None)))
        });
        let filesystem =
            fs.unwrap_or_else(|| std::rc::Rc::new(std::cell::RefCell::new(Filesystem::new(None))));
        Self {
            io,
            config,
            process,
            filesystem,
            has_cleaned_changes: IndexMap::new(),
        }
    }

    pub fn get_vcs_reference(&self, package: PackageInterfaceHandle, path: &str) -> Option<String> {
        let parser = VersionParser::new();
        let mut guesser = VersionGuesser::new(
            self.config.clone(),
            self.process.clone(),
            parser.clone(),
            Some(self.io.clone()),
        );
        let dumper = ArrayDumper::new();

        let package_config = dumper.dump(package.clone());
        if let Ok(Some(package_version)) = guesser.guess_version(&package_config, path) {
            return package_version.commit.clone();
        }

        None
    }
}

pub trait VcsDownloader:
    DownloaderInterface + ChangeReportInterface + VcsCapableDownloaderInterface
{
    fn io(&self) -> std::rc::Rc<std::cell::RefCell<dyn IOInterface>>;
    fn config(&self) -> &std::rc::Rc<std::cell::RefCell<Config>>;
    fn process(&self) -> &std::rc::Rc<std::cell::RefCell<ProcessExecutor>>;
    fn filesystem(&self) -> &std::rc::Rc<std::cell::RefCell<Filesystem>>;
    fn has_cleaned_changes(&self) -> &IndexMap<String, bool>;
    fn has_cleaned_changes_mut(&mut self) -> &mut IndexMap<String, bool>;

    /// Downloads data needed to run an install/update later
    async fn do_download(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        url: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>>;

    /// Downloads specific package into specific folder.
    async fn do_install(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        url: &str,
    ) -> Result<Option<PhpMixed>>;

    /// Updates specific package in specific folder from initial to target version.
    async fn do_update(
        &mut self,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &str,
        url: &str,
    ) -> Result<Option<PhpMixed>>;

    /// Fetches the commit logs between two commits
    fn get_commit_logs(
        &mut self,
        from_reference: &str,
        to_reference: &str,
        path: &str,
    ) -> Result<String>;

    /// Checks if VCS metadata repository has been initialized
    /// repository example: .git|.svn|.hg
    fn has_metadata_repository(&self, path: &str) -> bool;

    fn get_installation_source(&self) -> String {
        "source".to_string()
    }

    async fn download(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        if package.get_source_reference().is_none() {
            return Err(InvalidArgumentException {
                message: format!(
                    "Package {} is missing reference information",
                    package.get_pretty_name(),
                ),
                code: 0,
            }
            .into());
        }

        let mut urls = self.prepare_urls(package.get_source_urls());

        while let Some(url) = array_shift(&mut urls) {
            let attempt: Result<Option<PhpMixed>> = self
                .do_download(package.clone(), path, &url, prev_package.clone())
                .await;
            match attempt {
                Ok(promise) => return Ok(promise),
                Err(e) => {
                    // rethrow phpunit exceptions to avoid hard to debug bug failures.
                    // PHPUnit\Framework\Exception is out of scope (the test framework is not
                    // ported), so this instanceof check is always false.
                    let is_phpunit_exception = false;
                    if is_phpunit_exception {
                        return Err(e);
                    }
                    if self.io().is_debug() {
                        self.io().write_error3(
                            &format!("Failed: [{}] {}", get_class_err(&e), e,),
                            true,
                            io_interface::NORMAL,
                        );
                    } else if !urls.is_empty() {
                        self.io().write_error3(
                            "    Failed, trying the next URL",
                            true,
                            io_interface::NORMAL,
                        );
                    }
                    if urls.is_empty() {
                        return Err(e);
                    }
                }
            }
        }

        Ok(None)
    }

    async fn prepare(
        &mut self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        if r#type == "update" {
            self.clean_changes(prev_package.clone().unwrap(), path, true)
                .await?;
            self.has_cleaned_changes_mut()
                .insert(prev_package.unwrap().get_unique_name(), true);
        } else if r#type == "install" {
            self.filesystem().borrow_mut().empty_directory(path, true)?;
        } else if r#type == "uninstall" {
            self.clean_changes(package, path, false).await?;
        }

        Ok(None)
    }

    async fn cleanup(
        &mut self,
        r#type: &str,
        _package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        if r#type == "update"
            && prev_package
                .clone()
                .map(|p| {
                    self.has_cleaned_changes()
                        .contains_key(&p.get_unique_name())
                })
                .unwrap_or(false)
        {
            self.reapply_changes(path)?;
            self.has_cleaned_changes_mut()
                .shift_remove(&prev_package.unwrap().get_unique_name());
        }

        Ok(None)
    }

    async fn install(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
    ) -> Result<Option<PhpMixed>> {
        if package.get_source_reference().is_none() {
            return Err(InvalidArgumentException {
                message: format!(
                    "Package {} is missing reference information",
                    package.get_pretty_name(),
                ),
                code: 0,
            }
            .into());
        }

        self.io().write_error3(
            &format!("  - {}: ", InstallOperation::format(package.clone(), false)),
            false,
            io_interface::NORMAL,
        );

        let mut urls = self.prepare_urls(package.get_source_urls());
        while let Some(url) = array_shift(&mut urls) {
            let attempt: Result<Option<PhpMixed>> =
                self.do_install(package.clone(), path, &url).await;
            match attempt {
                Ok(_) => break,
                Err(e) => {
                    // rethrow phpunit exceptions to avoid hard to debug bug failures.
                    // PHPUnit\Framework\Exception is out of scope (the test framework is not
                    // ported), so this instanceof check is always false.
                    let is_phpunit_exception = false;
                    if is_phpunit_exception {
                        return Err(e);
                    }
                    if self.io().is_debug() {
                        self.io().write_error3(
                            &format!("Failed: [{}] {}", get_class_err(&e), e,),
                            true,
                            io_interface::NORMAL,
                        );
                    } else if !urls.is_empty() {
                        self.io().write_error3(
                            "    Failed, trying the next URL",
                            true,
                            io_interface::NORMAL,
                        );
                    }
                    if urls.is_empty() {
                        return Err(e);
                    }
                }
            }
        }

        Ok(None)
    }

    async fn update(
        &mut self,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &str,
    ) -> Result<Option<PhpMixed>> {
        if target.get_source_reference().is_none() {
            return Err(InvalidArgumentException {
                message: format!(
                    "Package {} is missing reference information",
                    target.get_pretty_name(),
                ),
                code: 0,
            }
            .into());
        }

        self.io().write_error3(
            &format!(
                "  - {}: ",
                UpdateOperation::format(initial.clone(), target.clone(), false),
            ),
            false,
            io_interface::NORMAL,
        );

        let mut urls = self.prepare_urls(target.get_source_urls());

        let mut exception: Option<anyhow::Error> = None;
        while let Some(url) = array_shift(&mut urls) {
            let attempt: Result<Option<PhpMixed>> = self
                .do_update(initial.clone(), target.clone(), path, &url)
                .await;
            match attempt {
                Ok(_) => {
                    exception = None;
                    break;
                }
                Err(e) => {
                    // rethrow phpunit exceptions to avoid hard to debug bug failures.
                    // PHPUnit\Framework\Exception is out of scope (the test framework is not
                    // ported), so this instanceof check is always false.
                    let is_phpunit_exception = false;
                    if is_phpunit_exception {
                        return Err(e);
                    }
                    if self.io().is_debug() {
                        self.io().write_error3(
                            &format!("Failed: [{}] {}", get_class_err(&e), e,),
                            true,
                            io_interface::NORMAL,
                        );
                    } else if !urls.is_empty() {
                        self.io().write_error3(
                            "    Failed, trying the next URL",
                            true,
                            io_interface::NORMAL,
                        );
                    }
                    exception = Some(e);
                }
            }
        }

        // print the commit logs if in verbose mode and VCS metadata is present
        // because in case of missing metadata code would trigger another exception
        if exception.is_none() && self.io().is_verbose() && self.has_metadata_repository(path) {
            let initial_ref = initial.get_source_reference().unwrap_or_default();
            let target_ref = target.get_source_reference().unwrap_or_default();
            let mut message = "Pulling in changes:";
            let mut logs = self.get_commit_logs(&initial_ref, &target_ref, path)?;

            if trim(&logs, None).is_empty() {
                message = "Rolling back changes:";
                logs = self.get_commit_logs(&target_ref, &initial_ref, path)?;
            }

            if !trim(&logs, None).is_empty() {
                let prefixed: Vec<String> = array_map(
                    |line: &String| format!("      {}", line),
                    &explode("\n", &logs),
                );
                logs = implode("\n", &prefixed);

                // escape angle brackets for proper output in the console
                logs = str_replace("<", "\\<", &logs);

                self.io()
                    .write_error3(&format!("    {}", message), true, io_interface::NORMAL);
                self.io().write_error3(&logs, true, io_interface::NORMAL);
            }
        }

        if urls.is_empty()
            && let Some(e) = exception
        {
            return Err(e);
        }

        Ok(None)
    }

    async fn remove(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
    ) -> Result<Option<PhpMixed>> {
        self.io().write_error3(
            &format!("  - {}", UninstallOperation::format(package, false)),
            true,
            io_interface::NORMAL,
        );

        let result = self
            .filesystem()
            .borrow_mut()
            .remove_directory_async(path)
            .await?;
        if !result {
            return Err(RuntimeException {
                message: format!("Could not completely delete {}, aborting.", path),
                code: 0,
            }
            .into());
        }

        Ok(None)
    }

    fn get_vcs_reference(&self, package: PackageInterfaceHandle, path: &str) -> Option<String> {
        let parser = VersionParser::new();
        let guesser = VersionGuesser::new(
            self.config().clone(),
            self.process().clone(),
            parser.clone(),
            Some(self.io().clone()),
        );
        let dumper = ArrayDumper::new();

        let package_config = dumper.dump(package.clone());
        let mut guesser = guesser;
        if let Ok(Some(package_version)) = guesser.guess_version(&package_config, path) {
            return package_version.commit.clone();
        }

        None
    }

    /// Prompt the user to check if changes should be stashed/removed or the operation aborted
    ///
    /// @param  bool $update  if true (update) the changes can be stashed and reapplied after an update,
    ///                       if false (remove) the changes should be assumed to be lost if the operation is not aborted
    async fn clean_changes(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        _update: bool,
    ) -> Result<Option<PhpMixed>> {
        // the default implementation just fails if there are any changes, override in child classes to provide stash-ability
        if self.get_local_changes(package, path)?.is_some() {
            return Err(RuntimeException {
                message: format!("Source directory {} has uncommitted changes.", path),
                code: 0,
            }
            .into());
        }

        Ok(None)
    }

    /// Reapply previously stashed changes if applicable, only called after an update (regardless if successful or not)
    fn reapply_changes(&mut self, _path: &str) -> Result<()> {
        Ok(())
    }

    fn prepare_urls(&self, mut urls: Vec<String>) -> Vec<String> {
        for url_entry in &mut urls {
            let mut url = url_entry.clone();
            if Filesystem::is_local_path(&url) {
                // realpath() below will not understand
                // url that starts with "file://"
                let file_protocol = "file://";
                let mut is_file_protocol = false;
                if strpos(&url, file_protocol) == Some(0) {
                    url = substr(&url, strlen(file_protocol), None);
                    is_file_protocol = true;
                }

                // realpath() below will not understand %20 spaces etc.
                if strpos(&url, "%").is_some() {
                    url = rawurldecode(&url);
                }

                *url_entry = realpath(&url).unwrap_or_default();

                if is_file_protocol {
                    *url_entry = format!("{}{}", file_protocol, url_entry);
                }
            }
        }

        urls
    }
}
