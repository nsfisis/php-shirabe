//! ref: composer/src/Composer/Factory.php

use indexmap::IndexMap;

use shirabe_external_packages::symfony::component::console::formatter::OutputFormatter;
use shirabe_external_packages::symfony::component::console::formatter::OutputFormatterStyle;
use shirabe_external_packages::symfony::component::console::output::ConsoleOutput;
use shirabe_php_shim::{
    InvalidArgumentException, PATHINFO_EXTENSION, PHP_EOL, Phar, PhpMixed, RuntimeException,
    UnexpectedValueException, ZipArchive, array_keys, array_replace_recursive, class_exists,
    dirname, extension_loaded, file_exists, file_get_contents, file_put_contents, implode,
    in_array, is_array, is_dir, is_file, is_string, json_decode, pathinfo, realpath, str_replace,
    strpos, strtr, substr, trim,
};

use crate::autoload::AutoloadGenerator;
use crate::cache::Cache;
use crate::composer::{ComposerWeakHandle, PartialOrFullComposer};
use crate::composer::{PartialComposerHandle, PartialComposerWeakHandle};
use crate::config::Config;
use crate::config::JsonConfigSource;
use crate::downloader::DownloadManager;
use crate::downloader::FileDownloader;
use crate::downloader::FossilDownloader;
use crate::downloader::GitDownloader;
use crate::downloader::GzipDownloader;
use crate::downloader::HgDownloader;
use crate::downloader::PathDownloader;
use crate::downloader::PerforceDownloader;
use crate::downloader::PharDownloader;
use crate::downloader::RarDownloader;
use crate::downloader::SvnDownloader;
use crate::downloader::TarDownloader;
use crate::downloader::TransportException;
use crate::downloader::XzDownloader;
use crate::downloader::ZipDownloader;
use crate::event_dispatcher::Event;
use crate::event_dispatcher::EventDispatcher;
use crate::exception::NoSslException;
use crate::installer::BinaryInstaller;
use crate::installer::InstallationManager;
use crate::installer::LibraryInstaller;
use crate::installer::MetapackageInstaller;
use crate::installer::PluginInstaller;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::io::IOInterfaceMutable;
use crate::json::JsonFile;
use crate::json::JsonValidationException;
use crate::package::Locker;
use crate::package::RootPackageInterface;
use crate::package::RootPackageInterfaceHandle;
use crate::package::archiver::ArchiveManager;
use crate::package::archiver::PharArchiver;
use crate::package::archiver::ZipArchiver;
use crate::package::loader::RootPackageLoader;
use crate::package::version::VersionGuesser;
use crate::package::version::VersionParser;
use crate::plugin::PluginEvents;
use crate::plugin::PluginManager;
use crate::repository::FilesystemRepository;
use crate::repository::InstalledFilesystemRepository;
use crate::repository::InstalledRepositoryInterface;
use crate::repository::RepositoryFactory;
use crate::repository::RepositoryManager;
use crate::util::Filesystem;
use crate::util::HttpDownloader;
use crate::util::Platform;
use crate::util::ProcessExecutor;
use crate::util::Silencer;
use crate::util::r#loop::Loop;

/// Either a configuration array or a filename to read from. PHP's `$localConfig` accepts both.
pub enum LocalConfigInput {
    Path(String),
    Data(IndexMap<String, PhpMixed>),
}

/// PHP's `$disablePlugins` accepts `bool|'local'|'global'`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisablePlugins {
    None,
    All,
    Local,
    Global,
}

impl DisablePlugins {
    fn is_disabled_at_all(self) -> bool {
        !matches!(self, DisablePlugins::None)
    }
}

/// Creates a configured instance of composer.
pub struct Factory;

impl Factory {
    fn get_home_dir() -> anyhow::Result<String> {
        let home = Platform::get_env("COMPOSER_HOME");
        if let Some(h) = home {
            if !h.is_empty() {
                return Ok(h);
            }
        }

        if Platform::is_windows() {
            if Platform::get_env("APPDATA")
                .map(|s| s.is_empty())
                .unwrap_or(true)
            {
                return Err(anyhow::anyhow!(RuntimeException {
                    message:
                        "The APPDATA or COMPOSER_HOME environment variable must be set for composer to run correctly"
                            .to_string(),
                    code: 0,
                }));
            }

            let appdata = Platform::get_env("APPDATA").unwrap_or_default();
            return Ok(format!(
                "{}/Composer",
                trim(&strtr(&appdata, "\\", "/"), Some("/"))
            ));
        }

        let user_dir = Self::get_user_dir()?;
        let mut dirs: Vec<String> = Vec::new();

        if Self::use_xdg() {
            // XDG Base Directory Specifications
            let mut xdg_config = Platform::get_env("XDG_CONFIG_HOME").unwrap_or_default();
            if xdg_config.is_empty() {
                xdg_config = format!("{}/.config", user_dir);
            }

            dirs.push(format!("{}/composer", xdg_config));
        }

        dirs.push(format!("{}/.composer", user_dir));

        // select first dir which exists of: $XDG_CONFIG_HOME/composer or ~/.composer
        for dir in &dirs {
            let dir_copy = dir.clone();
            let exists =
                Silencer::call(|| Ok::<bool, anyhow::Error>(is_dir(&dir_copy))).unwrap_or(false);
            if exists {
                return Ok(dir.clone());
            }
        }

        // if none exists, we default to first defined one (XDG one if system uses it, or ~/.composer otherwise)
        Ok(dirs[0].clone())
    }

    fn get_cache_dir(home: &str) -> anyhow::Result<String> {
        let cache_dir = Platform::get_env("COMPOSER_CACHE_DIR").unwrap_or_default();
        if !cache_dir.is_empty() {
            return Ok(cache_dir);
        }

        let home_env = Platform::get_env("COMPOSER_HOME").unwrap_or_default();
        if !home_env.is_empty() {
            return Ok(format!("{}/cache", home_env));
        }

        if Platform::is_windows() {
            let mut cache_dir = Platform::get_env("LOCALAPPDATA").unwrap_or_default();
            if !cache_dir.is_empty() {
                cache_dir = format!("{}/Composer", cache_dir);
            } else {
                cache_dir = format!("{}/cache", home);
            }

            return Ok(trim(&strtr(&cache_dir, "\\", "/"), Some("/")));
        }

        let user_dir = Self::get_user_dir()?;
        if Platform::php_os() == "Darwin" {
            // Migrate existing cache dir in old location if present
            if is_dir(&format!("{}/cache", home))
                && !is_dir(&format!("{}/Library/Caches/composer", user_dir))
            {
                let from = format!("{}/cache", home);
                let to = format!("{}/Library/Caches/composer", user_dir);
                let _ = Silencer::call(|| Ok::<bool, anyhow::Error>(Platform::rename(&from, &to)));
            }

            return Ok(format!("{}/Library/Caches/composer", user_dir));
        }

        if home == format!("{}/.composer", user_dir).as_str() && is_dir(&format!("{}/cache", home))
        {
            return Ok(format!("{}/cache", home));
        }

        if Self::use_xdg() {
            let xdg_cache = Platform::get_env("XDG_CACHE_HOME").unwrap_or_default();
            let xdg_cache = if xdg_cache.is_empty() {
                format!("{}/.cache", user_dir)
            } else {
                xdg_cache
            };

            return Ok(format!("{}/composer", xdg_cache));
        }

        Ok(format!("{}/cache", home))
    }

    fn get_data_dir(home: &str) -> anyhow::Result<String> {
        let home_env = Platform::get_env("COMPOSER_HOME").unwrap_or_default();
        if !home_env.is_empty() {
            return Ok(home_env);
        }

        if Platform::is_windows() {
            return Ok(strtr(home, "\\", "/"));
        }

        let user_dir = Self::get_user_dir()?;
        if home != format!("{}/.composer", user_dir) && Self::use_xdg() {
            let xdg_data = Platform::get_env("XDG_DATA_HOME").unwrap_or_default();
            let xdg_data = if xdg_data.is_empty() {
                format!("{}/.local/share", user_dir)
            } else {
                xdg_data
            };

            return Ok(format!("{}/composer", xdg_data));
        }

        Ok(home.to_string())
    }

    pub fn create_config(
        io: Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>,
        cwd: Option<&str>,
    ) -> anyhow::Result<Config> {
        let cwd = match cwd {
            Some(s) => s.to_string(),
            None => Platform::get_cwd(true)?,
        };

        let mut config = Config::new(true, Some(cwd));

        // determine and add main dirs to the config
        let home = Self::get_home_dir()?;
        let mut defaults: IndexMap<String, PhpMixed> = IndexMap::new();
        let mut inner: IndexMap<String, PhpMixed> = IndexMap::new();
        inner.insert("home".to_string(), PhpMixed::String(home.clone()));
        inner.insert(
            "cache-dir".to_string(),
            PhpMixed::String(Self::get_cache_dir(&home)?),
        );
        inner.insert(
            "data-dir".to_string(),
            PhpMixed::String(Self::get_data_dir(&home)?),
        );
        defaults.insert(
            "config".to_string(),
            PhpMixed::Array(inner.into_iter().map(|(k, v)| (k, Box::new(v))).collect()),
        );
        config.merge(&defaults, Config::SOURCE_DEFAULT);

        // load global config
        let global_config_path = format!("{}/config.json", config.get_str("home")?);
        let mut file = JsonFile::new(global_config_path.clone(), None, io.clone())?;
        if file.exists() {
            if let Some(io_ref) = &io {
                io_ref.write_error3(
                    &format!("Loading config file {}", file.get_path()),
                    true,
                    crate::io::DEBUG,
                );
            }
            Self::validate_json_schema(
                io.clone(),
                ValidateJsonInput::File(&file),
                JsonFile::LAX_SCHEMA,
                None,
            )?;
            let read_data = match file.read()? {
                PhpMixed::Array(map) => map
                    .into_iter()
                    .map(|(k, v)| (k, *v))
                    .collect::<IndexMap<_, _>>(),
                _ => IndexMap::new(),
            };
            let file_path_owned = file.get_path().to_string();
            config.merge(&read_data, &file_path_owned);
        }
        config.set_config_source(Box::new(JsonConfigSource::new(
            std::rc::Rc::new(std::cell::RefCell::new(file)),
            false,
        )));

        let htaccess_protect = config.get("htaccess-protect").as_bool().unwrap_or(false);
        if htaccess_protect {
            // Protect directory against web access. Since HOME could be
            // the www-data's user home and be web-accessible it is a
            // potential security risk
            let dirs = [
                config.get_str("home")?,
                config.get_str("cache-dir")?,
                config.get_str("data-dir")?,
            ];
            for dir in &dirs {
                if !file_exists(&format!("{}/.htaccess", dir)) {
                    if !is_dir(dir) {
                        let dir_owned = dir.clone();
                        let _ = Silencer::call(|| {
                            Ok::<bool, anyhow::Error>(Platform::mkdir(&dir_owned, 0o777, true))
                        });
                    }
                    let path = format!("{}/.htaccess", dir);
                    let _ = Silencer::call(|| {
                        Ok::<Option<i64>, anyhow::Error>(file_put_contents(&path, b"Deny from all"))
                    });
                }
            }
        }

        // load global auth file
        let auth_file_path = format!("{}/auth.json", config.get_str("home")?);
        let mut auth_file = JsonFile::new(auth_file_path.clone(), None, io.clone())?;
        if auth_file.exists() {
            if let Some(io_ref) = &io {
                io_ref.write_error3(
                    &format!("Loading config file {}", auth_file.get_path()),
                    true,
                    crate::io::DEBUG,
                );
            }
            Self::validate_json_schema(
                io.clone(),
                ValidateJsonInput::File(&auth_file),
                JsonFile::AUTH_SCHEMA,
                None,
            )?;
            let read_data: IndexMap<String, PhpMixed> = match auth_file.read()? {
                PhpMixed::Array(map) => map.into_iter().map(|(k, v)| (k, *v)).collect(),
                _ => IndexMap::new(),
            };
            let mut wrapped: IndexMap<String, PhpMixed> = IndexMap::new();
            wrapped.insert(
                "config".to_string(),
                PhpMixed::Array(
                    read_data
                        .into_iter()
                        .map(|(k, v)| (k, Box::new(v)))
                        .collect(),
                ),
            );
            let auth_path_owned = auth_file.get_path().to_string();
            config.merge(&wrapped, &auth_path_owned);
        }
        config.set_auth_config_source(Box::new(JsonConfigSource::new(
            std::rc::Rc::new(std::cell::RefCell::new(auth_file)),
            true,
        )));

        Self::load_composer_auth_env(&mut config, io)?;

        Ok(config)
    }

    pub fn get_composer_file() -> anyhow::Result<String> {
        let env = Platform::get_env("COMPOSER");
        if let Some(env_str) = env {
            let env_trimmed = trim(&env_str, Some(" \t\n\r\0\u{0B}"));
            if env_trimmed != "" {
                if is_dir(&env_trimmed) {
                    return Err(anyhow::anyhow!(RuntimeException {
                        message: format!(
                            "The COMPOSER environment variable is set to {} which is a directory, this variable should point to a composer.json or be left unset.",
                            env_trimmed
                        ),
                        code: 0,
                    }));
                }

                return Ok(env_trimmed);
            }
        }

        Ok("./composer.json".to_string())
    }

    pub fn get_lock_file(composer_file: &str) -> String {
        let ext = pathinfo(
            PhpMixed::String(composer_file.to_string()),
            PATHINFO_EXTENSION,
        );
        let is_json = match ext {
            PhpMixed::String(s) => s == "json",
            _ => false,
        };
        if is_json {
            format!(
                "{}lock",
                substr(composer_file, 0, Some(composer_file.len() as i64 - 4))
            )
        } else {
            format!("{}.lock", composer_file)
        }
    }

    pub fn create_additional_styles() -> IndexMap<String, OutputFormatterStyle> {
        let mut styles: IndexMap<String, OutputFormatterStyle> = IndexMap::new();
        styles.insert(
            "highlight".to_string(),
            OutputFormatterStyle::new(Some("red"), None, Some(vec![])),
        );
        styles.insert(
            "warning".to_string(),
            OutputFormatterStyle::new(Some("black"), Some("yellow"), Some(vec![])),
        );
        styles
    }

    pub fn create_output() -> ConsoleOutput {
        let _styles = Self::create_additional_styles();
        // TODO(phase-b): OutputFormatter::new signature and ConsoleOutput::new_with_formatter missing
        todo!(
            "create_output: wire OutputFormatter into ConsoleOutput once the symfony console stubs are completed"
        )
    }

    /// Creates a Composer instance
    pub fn create_composer(
        &self,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        local_config: Option<LocalConfigInput>,
        disable_plugins: DisablePlugins,
        cwd: Option<&str>,
        full_load: bool,
        disable_scripts: bool,
    ) -> anyhow::Result<PartialComposerHandle> {
        // if a custom composer.json path is given, we change the default cwd to be that file's directory
        let mut local_config = local_config;
        let mut cwd = cwd.map(|s| s.to_string());
        if let Some(LocalConfigInput::Path(ref s)) = local_config {
            if is_file(s) && cwd.is_none() {
                cwd = Some(dirname(s));
            }
        }

        let cwd = match cwd {
            Some(s) => s,
            None => Platform::get_cwd(true)?,
        };

        // load Composer configuration
        if local_config.is_none() {
            local_config = Some(LocalConfigInput::Path(Self::get_composer_file()?));
        }

        let mut local_config_source = Config::SOURCE_UNKNOWN.to_string();
        let mut composer_file: Option<String> = None;
        let mut local_config_data: IndexMap<String, PhpMixed> = IndexMap::new();
        if let Some(LocalConfigInput::Path(path)) = &local_config {
            composer_file = Some(path.clone());

            let mut file = JsonFile::new(path.clone(), None, Some(io.clone()))?;

            if !file.exists() {
                let message = if path == "./composer.json" || path == "composer.json" {
                    format!("Composer could not find a composer.json file in {}", cwd)
                } else {
                    format!("Composer could not find the config file: {}", path)
                };
                let instructions = if full_load {
                    "To initialize a project, please create a composer.json file. See https://getcomposer.org/basic-usage"
                } else {
                    ""
                };
                return Err(anyhow::anyhow!(InvalidArgumentException {
                    message: format!("{}{}{}", message, PHP_EOL, instructions),
                    code: 0,
                }));
            }

            if !Platform::is_input_completion_process() {
                if let Err(e) = file.validate_schema(JsonFile::LAX_SCHEMA, None) {
                    if let Some(jve) = e.downcast_ref::<JsonValidationException>() {
                        let errors = format!(
                            " - {}",
                            implode(&format!("{} - ", PHP_EOL), jve.get_errors())
                        );
                        let message = format!("{}:{}{}", jve.get_message(), PHP_EOL, errors);
                        return Err(anyhow::anyhow!(JsonValidationException::new(
                            message,
                            jve.get_errors().clone(),
                        )));
                    }
                    return Err(e);
                }
            }

            local_config_data = file
                .read()?
                .as_array()
                .map(|m| m.iter().map(|(k, v)| (k.clone(), (**v).clone())).collect())
                .unwrap_or_default();
            local_config_source = file.get_path().to_string();
        } else if let Some(LocalConfigInput::Data(data)) = local_config {
            local_config_data = data;
        }

        // Load config and override with local config/auth config
        let mut config = Self::create_config(Some(io.clone()), Some(&cwd))?;
        let is_global = local_config_source != Config::SOURCE_UNKNOWN
            && realpath(&config.get_str("home")?) == realpath(&dirname(&local_config_source));
        config.merge(&local_config_data, &local_config_source);

        if let Some(ref composer_file_path) = composer_file {
            io.write_error3(
                &format!(
                    "Loading config file {} ({})",
                    composer_file_path,
                    realpath(composer_file_path).unwrap_or_default()
                ),
                true,
                crate::io::DEBUG,
            );
            config.set_config_source(Box::new(JsonConfigSource::new(
                std::rc::Rc::new(std::cell::RefCell::new(JsonFile::new(
                    realpath(composer_file_path).unwrap_or_default(),
                    None,
                    Some(io.clone()),
                )?)),
                false,
            )));

            let mut local_auth_file = JsonFile::new(
                format!(
                    "{}/auth.json",
                    dirname(&realpath(composer_file_path).unwrap_or_default())
                ),
                None,
                Some(io.clone()),
            )?;
            if local_auth_file.exists() {
                io.write_error3(
                    &format!("Loading config file {}", local_auth_file.get_path()),
                    true,
                    crate::io::DEBUG,
                );
                Self::validate_json_schema(
                    Some(io.clone()),
                    ValidateJsonInput::File(&local_auth_file),
                    JsonFile::AUTH_SCHEMA,
                    None,
                )?;
                let auth_read = local_auth_file.read()?;
                let mut wrapped: IndexMap<String, PhpMixed> = IndexMap::new();
                wrapped.insert("config".to_string(), auth_read);
                let auth_path = local_auth_file.get_path().to_string();
                config.merge(&wrapped, &auth_path);
                config.set_local_auth_config_source(Box::new(JsonConfigSource::new(
                    std::rc::Rc::new(std::cell::RefCell::new(local_auth_file)),
                    true,
                )));
            }
        }

        // make sure we load the auth env again over the local auth.json + composer.json config
        Self::load_composer_auth_env(&mut config, Some(io.clone()))?;

        let vendor_dir = config.get_str("vendor-dir")?;

        // wrap config into Rc<RefCell<...>> for shared ownership across composer + downloaders/utils
        let config = std::rc::Rc::new(std::cell::RefCell::new(config));

        // initialize composer
        //
        // Phase C: build the whole Composer graph at once with Rc::new_cyclic so that
        // back-references (the EventDispatcher's composer, etc.) can hold a
        // PartialComposerWeak (Weak<RefCell<InnerComposer>>). The closure cannot return a
        // Result, so construction errors are surfaced through `build_error`.
        let mut build_error: Option<anyhow::Error> = None;
        let composer = std::rc::Rc::new_cyclic(
            |composer_weak: &std::rc::Weak<std::cell::RefCell<PartialOrFullComposer>>| {
                let mut build = || -> anyhow::Result<PartialOrFullComposer> {
                    let mut composer: PartialOrFullComposer = if full_load {
                        PartialOrFullComposer::new_full()
                    } else {
                        PartialOrFullComposer::new_partial()
                    };
                    composer.set_config(config.clone());
                    if is_global {
                        composer.set_global();
                    }

                    if full_load {
                        // load auth configs into the IO instance
                        io.borrow_mut()
                            .load_configuration(&mut *config.borrow_mut())?;

                        // load existing Composer\InstalledVersions instance if available and scripts/plugins are allowed, as they might need it
                        // we only load if the InstalledVersions class wasn't defined yet so that this is only loaded once
                        let installed_versions_path = format!(
                            "{}/composer/installed.php",
                            config.borrow_mut().get_str("vendor-dir")?
                        );
                        if !disable_plugins.is_disabled_at_all()
                            && !disable_scripts
                            && !class_exists("Composer\\InstalledVersions")
                            && file_exists(&installed_versions_path)
                        {
                            // force loading the class at this point so it is loaded from the composer phar and not from the vendor dir
                            // as we cannot guarantee integrity of that file
                            if class_exists("Composer\\InstalledVersions") {
                                FilesystemRepository::safely_load_installed_versions(
                                    &installed_versions_path,
                                );
                            }
                        }
                    }

                    let http_downloader = std::rc::Rc::new(std::cell::RefCell::new(
                        Self::create_http_downloader(io.clone(), &config, IndexMap::new())?,
                    ));
                    let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(
                        Some(io.clone()),
                    )));
                    let r#loop = std::rc::Rc::new(std::cell::RefCell::new(Loop::new(
                        http_downloader.clone(),
                        Some(process.clone()),
                    )));
                    composer.set_loop(r#loop.clone());

                    // initialize event dispatcher with the Composer back-reference
                    let dispatcher = {
                        let mut d = EventDispatcher::new(
                            PartialComposerWeakHandle::from_weak(composer_weak.clone()),
                            io.clone(),
                            Some(process.clone()),
                        );
                        d.set_run_scripts(!disable_scripts);
                        std::rc::Rc::new(std::cell::RefCell::new(d))
                    };
                    composer.set_event_dispatcher(dispatcher.clone());

                    // initialize repository manager
                    let rm = std::rc::Rc::new(std::cell::RefCell::new(RepositoryFactory::manager(
                        io.clone(),
                        &config,
                        Some(http_downloader.clone()),
                        Some(dispatcher.clone()),
                        Some(process.clone()),
                    )?));

                    // force-set the version of the global package if not defined as
                    // guessing it adds no value and only takes time
                    if !full_load && !local_config_data.contains_key("version") {
                        local_config_data
                            .insert("version".to_string(), PhpMixed::String("1.0.0".to_string()));
                    }

                    // load package
                    let parser = VersionParser::new();
                    let guesser = VersionGuesser::new(
                        config.clone(),
                        process.clone(),
                        parser.clone(),
                        Some(io.clone()),
                    );
                    let mut loader = self.load_root_package(
                        rm.clone(),
                        config.clone(),
                        parser,
                        guesser,
                        io.clone(),
                    );
                    let package = loader.load(
                        local_config_data.clone(),
                        "Composer\\Package\\RootPackage",
                        Some(&cwd),
                    )?;
                    // TODO(phase-b): set_package expects RootPackageInterface; loader returns BasePackage
                    // composer.set_package(package);
                    let _ = package;

                    // load local repository
                    self.add_local_repository(
                        io.clone(),
                        &mut rm.borrow_mut(),
                        &vendor_dir,
                        composer.get_package().clone(),
                        Some(&process),
                    );
                    composer.set_repository_manager(rm.clone());

                    // initialize installation manager
                    let im = std::rc::Rc::new(std::cell::RefCell::new(
                        self.create_installation_manager(
                            r#loop.clone(),
                            io.clone(),
                            Some(dispatcher.clone()),
                        ),
                    ));
                    composer.set_installation_manager(im.clone());

                    if let PartialOrFullComposer::Full(ref mut composer_full) = composer {
                        // initialize download manager
                        let dm = self.create_download_manager(
                            io.clone(),
                            &config,
                            &http_downloader,
                            &process,
                            Some(&dispatcher),
                        )?;
                        composer_full.set_download_manager(dm.clone());

                        // initialize autoload generator
                        let generator =
                            AutoloadGenerator::new(dispatcher.clone(), Some(io.clone()));
                        composer_full.set_autoload_generator(std::rc::Rc::new(
                            std::cell::RefCell::new(generator),
                        ));

                        // initialize archive manager
                        let am = self.create_archive_manager(&*config.borrow(), &dm, &r#loop)?;
                        composer_full
                            .set_archive_manager(std::rc::Rc::new(std::cell::RefCell::new(am)));
                    }

                    // add installers to the manager (must happen after download manager is created since they read it out of $composer)
                    self.create_default_installers(&im, &composer, io.clone(), Some(&process));

                    // init locker if possible
                    if let PartialOrFullComposer::Full(ref mut composer_full) = composer {
                        if let Some(ref composer_file_path) = composer_file {
                            let lock_file = Self::get_lock_file(composer_file_path);
                            let lock_enabled = config
                                .borrow_mut()
                                .get("lock")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(true);
                            if !lock_enabled && file_exists(&lock_file) {
                                io.write_error3(
                        &format!(
                            "<warning>{} is present but ignored as the \"lock\" config option is disabled.</warning>",
                            lock_file
                        ),
                        true,
                        crate::io::NORMAL,
                    );
                            }

                            let locker = Locker::new(
                                io.clone(),
                                JsonFile::new(
                                    if lock_enabled {
                                        lock_file
                                    } else {
                                        Platform::get_dev_null()
                                    },
                                    None,
                                    Some(io.clone()),
                                )?,
                                im.clone(),
                                &file_get_contents(composer_file_path).unwrap_or_default(),
                                process.clone(),
                            );
                            composer_full
                                .set_locker(std::rc::Rc::new(std::cell::RefCell::new(locker)));
                        } else {
                            let lock_contents = JsonFile::encode(&PhpMixed::Array(
                                local_config_data
                                    .iter()
                                    .map(|(k, v)| (k.clone(), Box::new(v.clone())))
                                    .collect(),
                            ));
                            let locker = Locker::new(
                                io.clone(),
                                JsonFile::new(Platform::get_dev_null(), None, Some(io.clone()))?,
                                im.clone(),
                                &lock_contents,
                                process.clone(),
                            );
                            composer_full
                                .set_locker(std::rc::Rc::new(std::cell::RefCell::new(locker)));
                        }
                    }

                    Ok(composer)
                };
                match build() {
                    Ok(composer) => std::cell::RefCell::new(composer),
                    Err(e) => {
                        build_error = Some(e);
                        std::cell::RefCell::new(PartialOrFullComposer::new_partial())
                    }
                }
            },
        );
        if let Some(e) = build_error {
            return Err(e);
        }

        // initialize plugin manager
        //
        // PluginManager::new upgrades the Composer back-reference to read its config and locker, so
        // it must be built after Rc::new_cyclic returns; inside the closure the Rc is not yet
        // constructed and the weak handle cannot upgrade.
        let (is_full, is_global) = {
            let c = composer.borrow();
            (c.is_full(), c.is_global())
        };
        if is_full {
            let global_composer = if !is_global {
                self.create_global_composer(
                    io.clone(),
                    &*config.borrow(),
                    disable_plugins,
                    disable_scripts,
                    false,
                )
            } else {
                None
            };

            let pm = self.create_plugin_manager(
                io.clone(),
                ComposerWeakHandle::from_weak(std::rc::Rc::downgrade(&composer)),
                global_composer,
                disable_plugins,
            );
            let pm = std::rc::Rc::new(std::cell::RefCell::new(pm));
            composer
                .borrow_mut()
                .as_full_mut()
                .unwrap()
                .set_plugin_manager(pm.clone());

            if is_global {
                pm.borrow_mut().set_running_in_global_dir(true);
            }
            pm.borrow_mut().load_installed_plugins()?;
        }

        if full_load {
            // The back-reference is now upgradeable, so dispatching the INIT event (which may read
            // the Composer through the dispatcher) is safe only after the Rc has been constructed.
            let init_event = Event::from_name(PluginEvents::INIT.to_string());
            let init_event_name = init_event.get_name().to_string();
            let dispatcher = composer.borrow().get_event_dispatcher();
            dispatcher
                .borrow_mut()
                .dispatch(Some(&init_event_name), Some(init_event))?;

            // once everything is initialized we can
            // purge packages from local repos if they have been deleted on the filesystem
            // TODO(phase-b): rm and im are owned by composer at this point; need to access via composer
            // self.purge_packages(rm.get_local_repository(), &mut im)?;
        }

        Ok(PartialComposerHandle::from_rc(composer))
    }

    pub fn create_global(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        disable_plugins: DisablePlugins,
        disable_scripts: bool,
    ) -> Option<PartialComposerHandle> {
        let factory = Self;

        let config = Self::create_config(Some(io.clone()), None).ok()?;
        factory.create_global_composer(io, &config, disable_plugins, disable_scripts, true)
    }

    fn add_local_repository(
        &self,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        rm: &mut RepositoryManager,
        vendor_dir: &str,
        root_package: RootPackageInterfaceHandle,
        process: Option<&std::rc::Rc<std::cell::RefCell<ProcessExecutor>>>,
    ) {
        let fs = process
            .map(|p| std::rc::Rc::new(std::cell::RefCell::new(Filesystem::new(Some(p.clone())))));

        rm.set_local_repository(crate::repository::RepositoryInterfaceHandle::new(
            InstalledFilesystemRepository::new(
                JsonFile::new(
                    format!("{}/composer/installed.json", vendor_dir),
                    None,
                    Some(io.clone()),
                )
                .expect("installed.json path is always valid"),
                true,
                Some(root_package),
                fs,
            )
            .expect("InstalledFilesystemRepository::new should not fail"),
        ));
    }

    fn create_global_composer(
        &self,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: &Config,
        disable_plugins: DisablePlugins,
        disable_scripts: bool,
        full_load: bool,
    ) -> Option<PartialComposerHandle> {
        // make sure if disable plugins was 'local' it is now turned off
        let disable_plugins = if matches!(
            disable_plugins,
            DisablePlugins::Global | DisablePlugins::All
        ) {
            DisablePlugins::All
        } else {
            DisablePlugins::None
        };

        match self.create_composer(
            io.clone(),
            Some(LocalConfigInput::Path(format!(
                "{}/composer.json",
                config.get_str("home").ok()?
            ))),
            disable_plugins,
            Some(&config.get_str("home").ok()?),
            full_load,
            disable_scripts,
        ) {
            Ok(c) => Some(c),
            Err(e) => {
                io.write_error3(
                    &format!("Failed to initialize global composer: {}", e),
                    true,
                    crate::io::DEBUG,
                );
                None
            }
        }
    }

    pub fn create_download_manager(
        &self,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: &std::rc::Rc<std::cell::RefCell<Config>>,
        http_downloader: &std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
        process: &std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
        event_dispatcher: Option<&std::rc::Rc<std::cell::RefCell<EventDispatcher>>>,
    ) -> anyhow::Result<std::rc::Rc<std::cell::RefCell<DownloadManager>>> {
        let cache_files_ttl = config
            .borrow_mut()
            .get("cache-files-ttl")
            .and_then(|v| v.as_int())
            .unwrap_or(0);
        let cache = if cache_files_ttl > 0 {
            let mut cache = Cache::new(
                io.clone(),
                &config.borrow_mut().get_str("cache-files-dir")?,
                Some("a-z0-9_./"),
                None,
                false,
            );
            cache.set_read_only(
                config
                    .borrow_mut()
                    .get("cache-read-only")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            );
            Some(std::rc::Rc::new(std::cell::RefCell::new(cache)))
        } else {
            None
        };

        let fs = std::rc::Rc::new(std::cell::RefCell::new(Filesystem::new(Some(
            process.clone(),
        ))));

        let mut dm = DownloadManager::new(io.clone(), false, Some(fs.clone()));
        let preferred = config.borrow_mut().get("preferred-install");
        match preferred.as_string() {
            Some("dist") => {
                dm.set_prefer_dist(true);
            }
            Some("source") => {
                dm.set_prefer_source(true);
            }
            Some("auto") | _ => {
                // noop
            }
        }

        if let PhpMixed::Array(prefs) = preferred {
            dm.set_preferences(
                prefs
                    .into_iter()
                    .map(|(k, v)| {
                        (
                            k,
                            match *v {
                                PhpMixed::String(s) => s,
                                _ => String::new(),
                            },
                        )
                    })
                    .collect(),
            );
        }

        dm.set_downloader(
            "git",
            std::rc::Rc::new(std::cell::RefCell::new(GitDownloader::new(
                io.clone(),
                config.clone(),
                Some(process.clone()),
                Some(fs.clone()),
            ))),
        );
        dm.set_downloader(
            "svn",
            std::rc::Rc::new(std::cell::RefCell::new(SvnDownloader::new(
                io.clone(),
                config.clone(),
                process.clone(),
                fs.clone(),
            ))),
        );
        dm.set_downloader(
            "fossil",
            std::rc::Rc::new(std::cell::RefCell::new(FossilDownloader::new(
                io.clone(),
                config.clone(),
                process.clone(),
                fs.clone(),
            ))),
        );
        dm.set_downloader(
            "hg",
            std::rc::Rc::new(std::cell::RefCell::new(HgDownloader::new(
                io.clone(),
                config.clone(),
                process.clone(),
                fs.clone(),
            ))),
        );
        dm.set_downloader(
            "perforce",
            std::rc::Rc::new(std::cell::RefCell::new(PerforceDownloader::new(
                io.clone(),
                config.clone(),
                process.clone(),
                fs.clone(),
            ))),
        );
        dm.set_downloader(
            "zip",
            std::rc::Rc::new(std::cell::RefCell::new(ZipDownloader::new(
                io.clone(),
                config.clone(),
                http_downloader.clone(),
                event_dispatcher.cloned(),
                cache.clone(),
                fs.clone(),
                process.clone(),
            ))),
        );
        dm.set_downloader(
            "rar",
            std::rc::Rc::new(std::cell::RefCell::new(RarDownloader::new(
                io.clone(),
                config.clone(),
                http_downloader.clone(),
                event_dispatcher.cloned(),
                cache.clone(),
                fs.clone(),
                process.clone(),
            ))),
        );
        dm.set_downloader(
            "tar",
            std::rc::Rc::new(std::cell::RefCell::new(TarDownloader::new(
                io.clone(),
                config.clone(),
                http_downloader.clone(),
                event_dispatcher.cloned(),
                cache.clone(),
                fs.clone(),
                process.clone(),
            ))),
        );
        dm.set_downloader(
            "gzip",
            std::rc::Rc::new(std::cell::RefCell::new(GzipDownloader::new(
                io.clone(),
                config.clone(),
                http_downloader.clone(),
                event_dispatcher.cloned(),
                cache.clone(),
                fs.clone(),
                process.clone(),
            ))),
        );
        dm.set_downloader(
            "xz",
            std::rc::Rc::new(std::cell::RefCell::new(XzDownloader::new(
                io.clone(),
                config.clone(),
                http_downloader.clone(),
                event_dispatcher.cloned(),
                cache.clone(),
                fs.clone(),
                process.clone(),
            ))),
        );
        dm.set_downloader(
            "phar",
            std::rc::Rc::new(std::cell::RefCell::new(PharDownloader::new(
                io.clone(),
                config.clone(),
                http_downloader.clone(),
                event_dispatcher.cloned(),
                cache.clone(),
                fs.clone(),
                process.clone(),
            ))),
        );
        dm.set_downloader(
            "file",
            std::rc::Rc::new(std::cell::RefCell::new(FileDownloader::new(
                io.clone(),
                config.clone(),
                http_downloader.clone(),
                event_dispatcher.cloned(),
                cache.clone(),
                Some(fs.clone()),
                Some(process.clone()),
            ))),
        );
        dm.set_downloader(
            "path",
            std::rc::Rc::new(std::cell::RefCell::new(PathDownloader::new(
                io.clone(),
                config.clone(),
                http_downloader.clone(),
                event_dispatcher.cloned(),
                cache.clone(),
                fs.clone(),
                process.clone(),
            ))),
        );

        Ok(std::rc::Rc::new(std::cell::RefCell::new(dm)))
    }

    pub fn create_archive_manager(
        &self,
        _config: &Config,
        dm: &std::rc::Rc<std::cell::RefCell<DownloadManager>>,
        r#loop: &std::rc::Rc<std::cell::RefCell<Loop>>,
    ) -> anyhow::Result<ArchiveManager> {
        let mut am = ArchiveManager::new(dm.clone(), r#loop.clone());
        if class_exists("ZipArchive") {
            am.add_archiver(Box::new(ZipArchiver::new()));
        }
        if class_exists("Phar") {
            am.add_archiver(Box::new(PharArchiver::new()));
        }

        Ok(am)
    }

    fn create_plugin_manager(
        &self,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        composer: ComposerWeakHandle,
        global_composer: Option<PartialComposerHandle>,
        disable_plugins: DisablePlugins,
    ) -> PluginManager {
        PluginManager::new(io, composer, global_composer, disable_plugins)
    }

    pub fn create_installation_manager(
        &self,
        r#loop: std::rc::Rc<std::cell::RefCell<Loop>>,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        event_dispatcher: Option<std::rc::Rc<std::cell::RefCell<EventDispatcher>>>,
    ) -> InstallationManager {
        InstallationManager::new(r#loop, io, event_dispatcher)
    }

    fn create_default_installers(
        &self,
        im: &std::rc::Rc<std::cell::RefCell<InstallationManager>>,
        composer: &PartialOrFullComposer,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        process: Option<&std::rc::Rc<std::cell::RefCell<ProcessExecutor>>>,
    ) {
        let fs = std::rc::Rc::new(std::cell::RefCell::new(Filesystem::new(
            process.map(std::rc::Rc::clone),
        )));
        let bin_dir = trim(
            &composer
                .get_config()
                .borrow_mut()
                .get_str("bin-dir")
                .unwrap_or_default(),
            Some("/"),
        );
        let bin_compat = composer
            .get_config()
            .borrow_mut()
            .get_str("bin-compat")
            .unwrap_or_default();
        let vendor_dir = trim(
            &composer
                .get_config()
                .borrow_mut()
                .get_str("vendor-dir")
                .unwrap_or_default(),
            Some("/"),
        );
        // TODO(phase-b): BinaryInstaller is a PHP class so it can't be cloned. Sharing requires
        // Rc<RefCell<BinaryInstaller>>; for now construct one per installer.
        let _binary_installer = BinaryInstaller::new(
            io.clone(),
            bin_dir.clone(),
            bin_compat.clone(),
            Some(fs.clone()),
            Some(vendor_dir.clone()),
        );

        // TODO(phase-b): InstallationManager not clone-able; need shared Rc<RefCell<>>
        let _ = im;
    }

    fn purge_packages(
        &self,
        repo: &mut dyn InstalledRepositoryInterface,
        im: &mut InstallationManager,
    ) -> anyhow::Result<()> {
        for package in repo.get_packages()? {
            if !im.is_package_installed(repo, package.clone())? {
                let _ = package;
            }
        }
        Ok(())
    }

    fn load_root_package(
        &self,
        rm: std::rc::Rc<std::cell::RefCell<RepositoryManager>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        parser: VersionParser,
        guesser: VersionGuesser,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> RootPackageLoader {
        RootPackageLoader::new(rm, config, Some(parser), Some(guesser), Some(io))
    }

    pub fn create(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: Option<LocalConfigInput>,
        disable_plugins: DisablePlugins,
        disable_scripts: bool,
    ) -> anyhow::Result<PartialComposerHandle> {
        let factory = Self;

        // for BC reasons, if a config is passed in either as array or a path that is not the default composer.json path
        // we disable local plugins as they really should not be loaded from CWD
        // If you want to avoid this behavior, you should be calling createComposer directly with a $cwd arg set correctly
        // to the path where the composer.json being loaded resides
        let default_composer_file = Self::get_composer_file()?;
        let config_is_default = matches!(
            config.as_ref(),
            Some(LocalConfigInput::Path(p)) if *p == default_composer_file
        );
        let disable_plugins = if config.is_some()
            && !config_is_default
            && matches!(disable_plugins, DisablePlugins::None)
        {
            DisablePlugins::Local
        } else {
            disable_plugins
        };

        let composer =
            factory.create_composer(io, config, disable_plugins, None, true, disable_scripts)?;
        if !composer.is_full() {
            // TODO(phase-b): unreachable when fullLoad=true; downcasting needs design.
            return Err(anyhow::anyhow!(RuntimeException {
                message: "Composer expected with fullLoad=true".to_string(),
                code: 0,
            }));
        }
        Ok(composer)
    }

    /// If you are calling this in a plugin, you probably should instead use `$composer->getLoop()->getHttpDownloader()`
    pub fn create_http_downloader(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: &std::rc::Rc<std::cell::RefCell<Config>>,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<HttpDownloader> {
        // TODO(plugin): static `$warned` flag — port as a OnceCell or atomic in Phase B.
        static mut WARNED: bool = false;
        let mut disable_tls = false;
        // allow running the config command if disable-tls is in the arg list, even if openssl is missing, to allow disabling it via the config command
        let argv = shirabe_php_shim::server_argv();
        if !argv.is_empty()
            && argv.contains(&"disable-tls".to_string())
            && (argv.contains(&"conf".to_string()) || argv.contains(&"config".to_string()))
        {
            unsafe { WARNED = true };
            disable_tls = !extension_loaded("openssl");
        } else if config
            .borrow_mut()
            .get("disable-tls")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            if !unsafe { WARNED } {
                io.write_error3(
                    "<warning>You are running Composer with SSL/TLS protection disabled.</warning>",
                    true,
                    crate::io::NORMAL,
                );
            }
            unsafe { WARNED = true };
            disable_tls = true;
        } else if !extension_loaded("openssl") {
            return Err(anyhow::anyhow!(NoSslException(RuntimeException {
                message:
                    "The openssl extension is required for SSL/TLS protection but is not available. If you can not enable the openssl extension, you can disable this error, at your own risk, by setting the 'disable-tls' option to true."
                        .to_string(),
                code: 0,
            })));
        }
        let mut http_downloader_options: IndexMap<String, PhpMixed> = IndexMap::new();
        if !disable_tls {
            if "" != config.borrow_mut().get_str("cafile").unwrap_or_default() {
                let mut ssl_map: IndexMap<String, PhpMixed> = IndexMap::new();
                ssl_map.insert(
                    "cafile".to_string(),
                    PhpMixed::String(config.borrow_mut().get_str("cafile").unwrap_or_default()),
                );
                http_downloader_options.insert(
                    "ssl".to_string(),
                    PhpMixed::Array(ssl_map.into_iter().map(|(k, v)| (k, Box::new(v))).collect()),
                );
            }
            if "" != config.borrow_mut().get_str("capath").unwrap_or_default() {
                let existing_ssl = http_downloader_options
                    .get("ssl")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                let mut ssl_map: IndexMap<String, Box<PhpMixed>> = existing_ssl;
                ssl_map.insert(
                    "capath".to_string(),
                    Box::new(PhpMixed::String(
                        config.borrow_mut().get_str("capath").unwrap_or_default(),
                    )),
                );
                http_downloader_options.insert("ssl".to_string(), PhpMixed::Array(ssl_map));
            }
            http_downloader_options =
                array_replace_recursive(http_downloader_options, options.clone());
        }
        let http_downloader_result: anyhow::Result<HttpDownloader> = Ok(HttpDownloader::new(
            io.clone(),
            config.clone(),
            http_downloader_options,
            disable_tls,
        ));
        let http_downloader = match http_downloader_result {
            Ok(h) => h,
            Err(e) => {
                if let Some(te) = e.downcast_ref::<TransportException>() {
                    if strpos(&te.get_message(), "cafile").is_some() {
                        io.write3(
                            "<error>Unable to locate a valid CA certificate file. You must set a valid 'cafile' option.</error>",
                            true,
                            crate::io::NORMAL,
                        );
                        io.write3(
                            "<error>A valid CA certificate file is required for SSL/TLS protection.</error>",
                            true,
                            crate::io::NORMAL,
                        );
                        io.write3(
                            "<error>You can disable this error, at your own risk, by setting the 'disable-tls' option to true.</error>",
                            true,
                            crate::io::NORMAL,
                        );
                    }
                }
                return Err(e);
            }
        };

        Ok(http_downloader)
    }

    fn load_composer_auth_env(
        config: &mut Config,
        io: Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>,
    ) -> anyhow::Result<()> {
        let composer_auth_env = Platform::get_env("COMPOSER_AUTH");
        let composer_auth_env_str = match composer_auth_env {
            Some(s) if !s.is_empty() => s,
            _ => return Ok(()),
        };

        let auth_data = json_decode(&composer_auth_env_str, false)?;
        if matches!(auth_data, PhpMixed::Null) {
            return Err(anyhow::anyhow!(UnexpectedValueException {
                message:
                    "COMPOSER_AUTH environment variable is malformed, should be a valid JSON object"
                        .to_string(),
                code: 0,
            }));
        }

        if let Some(io_ref) = &io {
            io_ref.write_error3(
                "Loading auth config from COMPOSER_AUTH",
                true,
                crate::io::DEBUG,
            );
        }
        Self::validate_json_schema(
            io,
            ValidateJsonInput::Data(auth_data.clone()),
            JsonFile::AUTH_SCHEMA,
            Some("COMPOSER_AUTH"),
        )?;
        let auth_data_assoc = json_decode(&composer_auth_env_str, true)?;
        if !matches!(auth_data_assoc, PhpMixed::Null) {
            let mut wrapped: IndexMap<String, PhpMixed> = IndexMap::new();
            wrapped.insert("config".to_string(), auth_data_assoc);
            config.merge(&wrapped, "COMPOSER_AUTH");
        }
        Ok(())
    }

    fn use_xdg() -> bool {
        // PHP: array_keys($_SERVER) — iterate env-style server vars
        for (key, _) in std::env::vars() {
            if strpos(&key, "XDG_") == Some(0) {
                return true;
            }
        }

        Silencer::call(|| Ok::<bool, anyhow::Error>(is_dir("/etc/xdg"))).unwrap_or(false)
    }

    fn get_user_dir() -> anyhow::Result<String> {
        let home = Platform::get_env("HOME").unwrap_or_default();
        if home.is_empty() {
            return Err(anyhow::anyhow!(RuntimeException {
                message:
                    "The HOME or COMPOSER_HOME environment variable must be set for composer to run correctly"
                        .to_string(),
                code: 0,
            }));
        }

        Ok(trim(&strtr(&home, "\\", "/"), Some("/")))
    }

    fn validate_json_schema(
        io: Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>,
        file_or_data: ValidateJsonInput<'_>,
        schema: i64,
        source: Option<&str>,
    ) -> anyhow::Result<()> {
        if Platform::is_input_completion_process() {
            return Ok(());
        }

        let result = match file_or_data {
            ValidateJsonInput::File(file) => file.validate_schema(schema, None),
            ValidateJsonInput::Data(data) => {
                let source = source.ok_or_else(|| {
                    anyhow::anyhow!(InvalidArgumentException {
                        message:
                            "$source is required to be provided if $fileOrData is arbitrary data"
                                .to_string(),
                        code: 0,
                    })
                })?;
                JsonFile::validate_json_schema(source, &data, schema, None)
            }
        };

        if let Err(e) = result {
            if let Some(jve) = e.downcast_ref::<JsonValidationException>() {
                let msg = format!(
                    "{}, this may result in errors and should be resolved:{} - {}",
                    jve.get_message(),
                    PHP_EOL,
                    implode(&format!("{} - ", PHP_EOL), jve.get_errors())
                );
                if let Some(io_ref) = io {
                    io_ref.write_error3(&format!("<warning>{}</>", msg), true, crate::io::NORMAL);
                } else {
                    return Err(anyhow::anyhow!(UnexpectedValueException {
                        message: msg,
                        code: 0
                    }));
                }
            } else {
                return Err(e);
            }
        }
        Ok(())
    }
}

enum ValidateJsonInput<'a> {
    File(&'a JsonFile),
    Data(PhpMixed),
}
