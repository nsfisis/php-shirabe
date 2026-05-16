//! ref: composer/src/Composer/Repository/PathRepository.php

use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{
    DIRECTORY_SEPARATOR, GLOB_BRACE, GLOB_MARK, GLOB_ONLYDIR, PhpMixed, RuntimeException, defined,
    file_exists, file_get_contents, glob_with_flags, hash, realpath, serialize,
};

use crate::config::Config;
use crate::event_dispatcher::event_dispatcher::EventDispatcher;
use crate::io::io_interface::IOInterface;
use crate::json::json_file::JsonFile;
use crate::package::loader::array_loader::ArrayLoader;
use crate::package::version::version_guesser::VersionGuesser;
use crate::package::version::version_parser::VersionParser;
use crate::repository::array_repository::ArrayRepository;
use crate::repository::configurable_repository_interface::ConfigurableRepositoryInterface;
use crate::util::filesystem::Filesystem;
use crate::util::git::Git as GitUtil;
use crate::util::http_downloader::HttpDownloader;
use crate::util::platform::Platform;
use crate::util::process_executor::ProcessExecutor;
use crate::util::url::Url;

#[derive(Debug)]
pub struct PathRepository {
    inner: ArrayRepository,
    loader: ArrayLoader,
    version_guesser: VersionGuesser,
    url: String,
    repo_config: IndexMap<String, PhpMixed>,
    process: ProcessExecutor,
    options: IndexMap<String, PhpMixed>,
}

impl ConfigurableRepositoryInterface for PathRepository {
    fn get_repo_config(&self) -> IndexMap<String, PhpMixed> {
        self.repo_config.clone()
    }
}

impl PathRepository {
    pub fn new(
        repo_config: IndexMap<String, PhpMixed>,
        io: Box<dyn IOInterface>,
        config: Config,
        http_downloader: Option<HttpDownloader>,
        dispatcher: Option<EventDispatcher>,
        process: Option<ProcessExecutor>,
    ) -> anyhow::Result<Self> {
        if !repo_config.contains_key("url") {
            return Err(RuntimeException {
                message: "You must specify the `url` configuration for the path repository"
                    .to_string(),
                code: 0,
            }
            .into());
        }

        let url_str = repo_config
            .get("url")
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .to_string();
        let url = Platform::expand_path(&url_str);
        let process = process.unwrap_or_else(|| ProcessExecutor::new(&*io));
        let version_guesser = VersionGuesser::new(&config, &process, VersionParser::new(), &*io);
        let mut options = repo_config
            .get("options")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|(k, v)| (k, *v))
            .collect::<IndexMap<String, PhpMixed>>();
        if !options.contains_key("relative") {
            let filesystem = Filesystem::new();
            let is_relative = !filesystem.is_absolute_path(&url);
            options.insert("relative".to_string(), PhpMixed::Bool(is_relative));
        }

        Ok(Self {
            inner: ArrayRepository::new(),
            loader: ArrayLoader::new(None, true),
            version_guesser,
            url,
            repo_config,
            process,
            options,
        })
    }

    pub fn get_repo_name(&self) -> String {
        format!(
            "path repo ({})",
            Url::sanitize(
                self.repo_config
                    .get("url")
                    .and_then(|v| v.as_string())
                    .unwrap_or("")
                    .to_string()
            )
        )
    }

    pub(crate) fn initialize(&mut self) -> anyhow::Result<()> {
        self.inner.initialize()?;

        let url_matches = self.get_url_matches()?;

        if url_matches.is_empty() {
            if Preg::is_match(r"{[*{}]}", &self.url).unwrap_or(false) {
                let mut url = self.url.clone();
                while Preg::is_match(r"{[*{}]}", &url).unwrap_or(false) {
                    url = shirabe_php_shim::dirname(&url);
                }
                // the parent directory before any wildcard exists, so we assume it is correctly configured but simply empty
                if shirabe_php_shim::is_dir(&url) {
                    return Ok(());
                }
            }

            return Err(RuntimeException {
                message: format!(
                    "The `url` supplied for the path ({}) repository does not exist",
                    self.url
                ),
                code: 0,
            }
            .into());
        }

        for url in url_matches {
            let path = format!("{}/", realpath(&url).unwrap_or_default());
            let composer_file_path = format!("{}composer.json", path);

            if !file_exists(&composer_file_path) {
                continue;
            }

            let json = file_get_contents(&composer_file_path).unwrap_or_default();
            let mut package =
                JsonFile::parse_json(&json, Some(&composer_file_path))?.unwrap_or_default();
            let dist = {
                let mut dist = IndexMap::new();
                dist.insert(
                    "type".to_string(),
                    Box::new(PhpMixed::String("path".to_string())),
                );
                dist.insert("url".to_string(), Box::new(PhpMixed::String(url.clone())));
                dist
            };
            package.insert("dist".to_string(), PhpMixed::Array(dist));

            let reference = self
                .options
                .get("reference")
                .and_then(|v| v.as_string())
                .unwrap_or("auto")
                .to_string();
            if reference == "none" {
                if let Some(PhpMixed::Array(ref mut dist)) = package.get_mut("dist") {
                    dist.insert("reference".to_string(), Box::new(PhpMixed::Null));
                }
            } else if reference == "config" || reference == "auto" {
                let options_mixed = PhpMixed::Array(
                    self.options
                        .iter()
                        .map(|(k, v)| (k.clone(), Box::new(v.clone())))
                        .collect(),
                );
                let ref_hash = hash("sha1", &format!("{}{}", json, serialize(&options_mixed)));
                if let Some(PhpMixed::Array(ref mut dist)) = package.get_mut("dist") {
                    dist.insert(
                        "reference".to_string(),
                        Box::new(PhpMixed::String(ref_hash)),
                    );
                }
            }

            // copy symlink/relative options to transport options
            let transport_options: IndexMap<String, Box<PhpMixed>> = self
                .options
                .iter()
                .filter(|(k, _)| k.as_str() == "symlink" || k.as_str() == "relative")
                .map(|(k, v)| (k.clone(), Box::new(v.clone())))
                .collect();
            package.insert(
                "transport-options".to_string(),
                PhpMixed::Array(transport_options),
            );

            // use the version provided as option if available
            if let Some(name) = package
                .get("name")
                .and_then(|v| v.as_string())
                .map(|s| s.to_string())
            {
                if let Some(version) = self
                    .options
                    .get("versions")
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.get(&name))
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_string())
                {
                    package.insert("version".to_string(), PhpMixed::String(version));
                }
            }

            // carry over the root package version if this path repo is in the same git repository as root package
            if !package.contains_key("version") {
                if let Some(root_version) = Platform::get_env("COMPOSER_ROOT_VERSION") {
                    if !root_version.is_empty() {
                        let mut ref1 = String::new();
                        let mut ref2 = String::new();
                        if self.process.execute(
                            &["git", "rev-parse", "HEAD"].map(|s| s.to_string()).to_vec(),
                            &mut ref1,
                            Some(path.clone()),
                        ) == 0
                            && self.process.execute(
                                &["git", "rev-parse", "HEAD"].map(|s| s.to_string()).to_vec(),
                                &mut ref2,
                                None,
                            ) == 0
                            && ref1 == ref2
                        {
                            package.insert(
                                "version".to_string(),
                                PhpMixed::String(self.version_guesser.get_root_version_from_env()),
                            );
                        }
                    }
                }
            }

            let mut output = String::new();
            let command = GitUtil::build_rev_list_command(&self.process, {
                let mut args = vec![
                    "-n1".to_string(),
                    "--format=%H".to_string(),
                    "HEAD".to_string(),
                ];
                args.extend(GitUtil::get_no_show_signature_flags(&self.process));
                args
            });
            if reference == "auto"
                && shirabe_php_shim::is_dir(&format!("{}/.git", path.trim_end_matches('/')))
                && self
                    .process
                    .execute(&command, &mut output, Some(path.clone()))
                    == 0
            {
                let ref_val = GitUtil::parse_rev_list_output(&output, &self.process)
                    .trim()
                    .to_string();
                if let Some(PhpMixed::Array(ref mut dist)) = package.get_mut("dist") {
                    dist.insert("reference".to_string(), Box::new(PhpMixed::String(ref_val)));
                }
            }

            if !package.contains_key("version") {
                let version_data = self.version_guesser.guess_version(&package, &path);
                if let Some(version_data) = version_data {
                    if let Some(pretty_version) = version_data
                        .get("pretty_version")
                        .and_then(|v| v.as_string())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                    {
                        // if there is a feature branch detected, we add a second package with the feature branch version
                        if let Some(feature_pretty_version) = version_data
                            .get("feature_pretty_version")
                            .and_then(|v| v.as_string())
                            .filter(|s| !s.is_empty())
                            .map(|s| s.to_string())
                        {
                            package.insert(
                                "version".to_string(),
                                PhpMixed::String(feature_pretty_version),
                            );
                            self.inner.add_package(self.loader.load(package.clone())?);
                        }

                        package.insert("version".to_string(), PhpMixed::String(pretty_version));
                    } else {
                        package.insert(
                            "version".to_string(),
                            PhpMixed::String("dev-main".to_string()),
                        );
                    }
                } else {
                    package.insert(
                        "version".to_string(),
                        PhpMixed::String("dev-main".to_string()),
                    );
                }
            }

            self.inner
                .add_package(
                    self.loader
                        .load(package.clone())
                        .map_err(|e| RuntimeException {
                            message: format!(
                                "Failed loading the package in {}",
                                composer_file_path
                            ),
                            code: 0,
                        })?,
                );
        }

        Ok(())
    }

    fn get_url_matches(&self) -> anyhow::Result<Vec<String>> {
        let mut flags = GLOB_MARK | GLOB_ONLYDIR;

        if defined("GLOB_BRACE") {
            flags |= GLOB_BRACE;
        } else if self.url.contains('{') || self.url.contains('}') {
            return Err(RuntimeException {
                message: format!(
                    "The operating system does not support GLOB_BRACE which is required for the url {}",
                    self.url
                ),
                code: 0,
            }
            .into());
        }

        // Ensure environment-specific path separators are normalized to URL separators
        Ok(glob_with_flags(&self.url, flags)
            .into_iter()
            .map(|val| {
                val.replace(DIRECTORY_SEPARATOR, "/")
                    .trim_end_matches('/')
                    .to_string()
            })
            .collect())
    }
}
