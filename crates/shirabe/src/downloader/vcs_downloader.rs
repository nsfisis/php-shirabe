//! ref: composer/src/Composer/Downloader/VcsDownloader.php

use crate::io::io_interface;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use shirabe_php_shim::{
    InvalidArgumentException, PhpMixed, RuntimeException, array_map, array_shift, count, explode,
    get_class, implode, rawurldecode, realpath, str_replace, strlen, strpos, substr, trim,
};

use crate::config::Config;
use crate::dependency_resolver::operation::install_operation::InstallOperation;
use crate::dependency_resolver::operation::uninstall_operation::UninstallOperation;
use crate::dependency_resolver::operation::update_operation::UpdateOperation;
use crate::downloader::change_report_interface::ChangeReportInterface;
use crate::downloader::downloader_interface::DownloaderInterface;
use crate::downloader::vcs_capable_downloader_interface::VcsCapableDownloaderInterface;
use crate::io::io_interface::IOInterface;
use crate::package::dumper::array_dumper::ArrayDumper;
use crate::package::package_interface::PackageInterface;
use crate::package::version::version_guesser::VersionGuesser;
use crate::package::version::version_parser::VersionParser;
use crate::util::filesystem::Filesystem;
use crate::util::process_executor::ProcessExecutor;

#[derive(Debug)]
pub struct VcsDownloaderBase {
    pub io: Box<dyn IOInterface>,
    pub config: Config,
    pub process: ProcessExecutor,
    pub filesystem: Filesystem,
    pub has_cleaned_changes: IndexMap<String, bool>,
}

impl VcsDownloaderBase {
    pub fn new(
        io: Box<dyn IOInterface>,
        config: Config,
        process: Option<ProcessExecutor>,
        fs: Option<Filesystem>,
    ) -> Self {
        let process = process.unwrap_or_else(|| ProcessExecutor::new(None, None));
        let filesystem = fs.unwrap_or_else(|| Filesystem::new(None));
        Self {
            io,
            config,
            process,
            filesystem,
            has_cleaned_changes: IndexMap::new(),
        }
    }
}

pub trait VcsDownloader:
    DownloaderInterface + ChangeReportInterface + VcsCapableDownloaderInterface
{
    fn io(&self) -> &dyn IOInterface;
    fn io_mut(&mut self) -> &mut dyn IOInterface;
    fn config(&self) -> &Config;
    fn config_mut(&mut self) -> &mut Config;
    fn process(&self) -> &ProcessExecutor;
    fn process_mut(&mut self) -> &mut ProcessExecutor;
    fn filesystem(&self) -> &Filesystem;
    fn filesystem_mut(&mut self) -> &mut Filesystem;
    fn has_cleaned_changes(&self) -> &IndexMap<String, bool>;
    fn has_cleaned_changes_mut(&mut self) -> &mut IndexMap<String, bool>;

    /// Downloads data needed to run an install/update later
    fn do_download(
        &mut self,
        package: &dyn PackageInterface,
        path: &str,
        url: &str,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Box<dyn PromiseInterface>>;

    /// Downloads specific package into specific folder.
    fn do_install(
        &mut self,
        package: &dyn PackageInterface,
        path: &str,
        url: &str,
    ) -> Result<Box<dyn PromiseInterface>>;

    /// Updates specific package in specific folder from initial to target version.
    fn do_update(
        &mut self,
        initial: &dyn PackageInterface,
        target: &dyn PackageInterface,
        path: &str,
        url: &str,
    ) -> Result<Box<dyn PromiseInterface>>;

    /// Fetches the commit logs between two commits
    fn get_commit_logs(&self, from_reference: &str, to_reference: &str, path: &str) -> String;

    /// Checks if VCS metadata repository has been initialized
    /// repository example: .git|.svn|.hg
    fn has_metadata_repository(&self, path: &str) -> bool;

    fn get_installation_source(&self) -> String {
        "source".to_string()
    }

    fn download(
        &mut self,
        package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Box<dyn PromiseInterface>> {
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
            // TODO(phase-b): use anyhow::Result<Result<T, E>> to model PHP try/catch
            let attempt: Result<Box<dyn PromiseInterface>> =
                self.do_download(package, path, &url, prev_package);
            match attempt {
                Ok(promise) => return Ok(promise),
                Err(e) => {
                    // rethrow phpunit exceptions to avoid hard to debug bug failures
                    // TODO(phase-b): downcast to PHPUnit\Framework\Exception
                    let is_phpunit_exception = false;
                    if is_phpunit_exception {
                        return Err(e);
                    }
                    if self.io().is_debug() {
                        self.io_mut().write_error(
                            PhpMixed::String(format!("Failed: [{}] {}", get_class(&e), e,)),
                            true,
                            io_interface::NORMAL,
                        );
                    } else if count(&PhpMixed::List(
                        urls.iter()
                            .map(|s| Box::new(PhpMixed::String(s.clone())))
                            .collect(),
                    )) > 0
                    {
                        self.io_mut().write_error(
                            PhpMixed::String("    Failed, trying the next URL".to_string()),
                            true,
                            io_interface::NORMAL,
                        );
                    }
                    if count(&PhpMixed::List(
                        urls.iter()
                            .map(|s| Box::new(PhpMixed::String(s.clone())))
                            .collect(),
                    )) == 0
                    {
                        return Err(e);
                    }
                }
            }
        }

        Ok(shirabe_external_packages::react::promise::resolve(None))
    }

    fn prepare(
        &mut self,
        r#type: &str,
        package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Box<dyn PromiseInterface>> {
        if r#type == "update" {
            self.clean_changes(prev_package.unwrap(), path, true)?;
            self.has_cleaned_changes_mut()
                .insert(prev_package.unwrap().get_unique_name(), true);
        } else if r#type == "install" {
            self.filesystem_mut().empty_directory(path);
        } else if r#type == "uninstall" {
            self.clean_changes(package, path, false)?;
        }

        Ok(shirabe_external_packages::react::promise::resolve(None))
    }

    fn cleanup(
        &mut self,
        r#type: &str,
        _package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Box<dyn PromiseInterface>> {
        if r#type == "update"
            && prev_package
                .map(|p| {
                    self.has_cleaned_changes()
                        .contains_key(&p.get_unique_name())
                })
                .unwrap_or(false)
        {
            self.reapply_changes(path);
            self.has_cleaned_changes_mut()
                .shift_remove(&prev_package.unwrap().get_unique_name());
        }

        Ok(shirabe_external_packages::react::promise::resolve(None))
    }

    fn install(
        &mut self,
        package: &dyn PackageInterface,
        path: &str,
    ) -> Result<Box<dyn PromiseInterface>> {
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

        self.io_mut().write_error(
            PhpMixed::String(format!(
                "  - {}: ",
                InstallOperation::format(package, false)
            )),
            false,
            io_interface::NORMAL,
        );

        let mut urls = self.prepare_urls(package.get_source_urls());
        while let Some(url) = array_shift(&mut urls) {
            // TODO(phase-b): use anyhow::Result<Result<T, E>> to model PHP try/catch
            let attempt: Result<Box<dyn PromiseInterface>> = self.do_install(package, path, &url);
            match attempt {
                Ok(_) => break,
                Err(e) => {
                    // rethrow phpunit exceptions to avoid hard to debug bug failures
                    // TODO(phase-b): downcast to PHPUnit\Framework\Exception
                    let is_phpunit_exception = false;
                    if is_phpunit_exception {
                        return Err(e);
                    }
                    if self.io().is_debug() {
                        self.io_mut().write_error(
                            PhpMixed::String(format!("Failed: [{}] {}", get_class(&e), e,)),
                            true,
                            io_interface::NORMAL,
                        );
                    } else if count(&PhpMixed::List(
                        urls.iter()
                            .map(|s| Box::new(PhpMixed::String(s.clone())))
                            .collect(),
                    )) > 0
                    {
                        self.io_mut().write_error(
                            PhpMixed::String("    Failed, trying the next URL".to_string()),
                            true,
                            io_interface::NORMAL,
                        );
                    }
                    if count(&PhpMixed::List(
                        urls.iter()
                            .map(|s| Box::new(PhpMixed::String(s.clone())))
                            .collect(),
                    )) == 0
                    {
                        return Err(e);
                    }
                }
            }
        }

        Ok(shirabe_external_packages::react::promise::resolve(None))
    }

    fn update(
        &mut self,
        initial: &dyn PackageInterface,
        target: &dyn PackageInterface,
        path: &str,
    ) -> Result<Box<dyn PromiseInterface>> {
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

        self.io_mut().write_error(
            PhpMixed::String(format!(
                "  - {}: ",
                UpdateOperation::format(initial, target, false),
            )),
            false,
            io_interface::NORMAL,
        );

        let mut urls = self.prepare_urls(target.get_source_urls());

        let mut exception: Option<anyhow::Error> = None;
        while let Some(url) = array_shift(&mut urls) {
            // TODO(phase-b): use anyhow::Result<Result<T, E>> to model PHP try/catch
            let attempt: Result<Box<dyn PromiseInterface>> =
                self.do_update(initial, target, path, &url);
            match attempt {
                Ok(_) => {
                    exception = None;
                    break;
                }
                Err(e) => {
                    // rethrow phpunit exceptions to avoid hard to debug bug failures
                    // TODO(phase-b): downcast to PHPUnit\Framework\Exception
                    let is_phpunit_exception = false;
                    if is_phpunit_exception {
                        return Err(e);
                    }
                    if self.io().is_debug() {
                        self.io_mut().write_error(
                            PhpMixed::String(format!("Failed: [{}] {}", get_class(&e), e,)),
                            true,
                            io_interface::NORMAL,
                        );
                    } else if count(&PhpMixed::List(
                        urls.iter()
                            .map(|s| Box::new(PhpMixed::String(s.clone())))
                            .collect(),
                    )) > 0
                    {
                        self.io_mut().write_error(
                            PhpMixed::String("    Failed, trying the next URL".to_string()),
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
            let mut message = "Pulling in changes:";
            let mut logs = self.get_commit_logs(
                initial.get_source_reference().unwrap_or(""),
                target.get_source_reference().unwrap_or(""),
                path,
            );

            if trim(&logs, None) == "" {
                message = "Rolling back changes:";
                logs = self.get_commit_logs(
                    target.get_source_reference().unwrap_or(""),
                    initial.get_source_reference().unwrap_or(""),
                    path,
                );
            }

            if trim(&logs, None) != "" {
                let prefixed: Vec<String> = array_map(
                    |line: &String| format!("      {}", line),
                    &explode("\n", &logs),
                );
                logs = implode("\n", &prefixed);

                // escape angle brackets for proper output in the console
                logs = str_replace("<", "\\<", &logs);

                self.io_mut().write_error(
                    PhpMixed::String(format!("    {}", message)),
                    true,
                    io_interface::NORMAL,
                );
                self.io_mut()
                    .write_error(PhpMixed::String(logs), true, io_interface::NORMAL);
            }
        }

        if urls.is_empty() {
            if let Some(e) = exception {
                return Err(e);
            }
        }

        Ok(shirabe_external_packages::react::promise::resolve(None))
    }

    fn remove(
        &mut self,
        package: &dyn PackageInterface,
        path: &str,
    ) -> Result<Box<dyn PromiseInterface>> {
        self.io_mut().write_error(
            PhpMixed::String(format!(
                "  - {}",
                UninstallOperation::format(package, false)
            )),
            true,
            io_interface::NORMAL,
        );

        let promise = self.filesystem_mut().remove_directory_async(path);

        let path = path.to_string();
        Ok(
            promise.then(Box::new(move |result: PhpMixed| -> Result<()> {
                let result_bool = result.as_bool().unwrap_or(false);
                if !result_bool {
                    return Err(RuntimeException {
                        message: format!("Could not completely delete {}, aborting.", path),
                        code: 0,
                    }
                    .into());
                }
                Ok(())
            })),
        )
    }

    fn get_vcs_reference(&self, package: &dyn PackageInterface, path: &str) -> Option<String> {
        let parser = VersionParser::new();
        let guesser = VersionGuesser::new(self.config(), self.process(), &parser, self.io());
        let dumper = ArrayDumper::new();

        let package_config = dumper.dump(package);
        if let Some(package_version) = guesser.guess_version(&package_config, path) {
            return package_version
                .get("commit")
                .and_then(|v| v.as_string())
                .map(|s| s.to_string());
        }

        None
    }

    /// Prompt the user to check if changes should be stashed/removed or the operation aborted
    ///
    /// @param  bool $update  if true (update) the changes can be stashed and reapplied after an update,
    ///                       if false (remove) the changes should be assumed to be lost if the operation is not aborted
    fn clean_changes(
        &self,
        package: &dyn PackageInterface,
        path: &str,
        _update: bool,
    ) -> Result<Box<dyn PromiseInterface>> {
        // the default implementation just fails if there are any changes, override in child classes to provide stash-ability
        if self.get_local_changes(package, path.to_string()).is_some() {
            return Err(RuntimeException {
                message: format!("Source directory {} has uncommitted changes.", path),
                code: 0,
            }
            .into());
        }

        Ok(shirabe_external_packages::react::promise::resolve(None))
    }

    /// Reapply previously stashed changes if applicable, only called after an update (regardless if successful or not)
    fn reapply_changes(&self, _path: &str) {}

    fn prepare_urls(&self, mut urls: Vec<String>) -> Vec<String> {
        for index in 0..urls.len() {
            let mut url = urls[index].clone();
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

                urls[index] = realpath(&url).unwrap_or_default();

                if is_file_protocol {
                    urls[index] = format!("{}{}", file_protocol, urls[index]);
                }
            }
        }

        urls
    }
}
